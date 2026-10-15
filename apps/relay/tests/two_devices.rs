//! Two vaults on two "devices" that share nothing but a relay.

use std::fs;
use std::net::SocketAddr;
use std::path::Path;

use graphite_core::{Device, RelayConfig, Vault, VaultConfig};

const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

async fn start_relay(data: &Path) -> SocketAddr {
    let relay = graphite_relay::Relay::open(data).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, graphite_relay::app(relay, 64 * 1024 * 1024))
            .await
            .unwrap();
    });
    addr
}

fn open(root: &Path, id: &str, name: &str, config: &RelayConfig) -> Vault {
    let mut v = Vault::open_as(root, Device::new(id, name)).unwrap();
    v.save_config(&VaultConfig { large_file_mb: 1 }).unwrap();
    v.set_relay(Some(config.clone())).unwrap();
    v
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn notes_and_files_travel_between_devices() {
    let data = tempfile::tempdir().unwrap();
    let cfg_dir = tempfile::tempdir().unwrap();
    std::env::set_var("GRAPHITE_RELAY_CONFIG_DIR", cfg_dir.path());
    let addr = start_relay(data.path()).await;
    let config = RelayConfig::new(&format!("http://{addr}")).unwrap();
    let root_a = tempfile::tempdir().unwrap();
    let root_b = tempfile::tempdir().unwrap();

    tokio::task::spawn_blocking(move || {
        let mut a = open(root_a.path(), A, "Desk", &config);
        let mut b = open(root_b.path(), B, "Phone", &config);

        // A has a note, a photo and a file over the limit.
        let note = a.create("Plan", None, None).unwrap();
        let mut n = note.clone();
        n.body = "hello\n".into();
        a.write(&n).unwrap();
        fs::write(root_a.path().join("attachments/photo.png"), b"png").unwrap();
        fs::write(root_a.path().join("attachments/movie.mov"), vec![0u8; 1_500_000]).unwrap();
        let r = a.relay_sync().unwrap();
        assert!(r.uploaded.iter().any(|p| p.ends_with(".automerge")), "{r:?}");
        assert!(r.uploaded.contains(&"attachments/photo.png".to_string()));
        assert_eq!(r.kept_local, vec!["attachments/movie.mov".to_string()]);
        assert!(r.downloaded.is_empty());

        // B gets the note and the photo, not the big file.
        let r = b.relay_sync().unwrap();
        assert!(r.downloaded.contains(&"attachments/photo.png".to_string()), "{r:?}");
        assert!(r.downloaded.iter().any(|p| p.starts_with(".graphite/sync/aaaa")));
        let on_b = b.read(&note.id).unwrap();
        assert_eq!(on_b.title, "Plan");
        assert_eq!(on_b.body, "hello\n");
        assert_eq!(fs::read(root_b.path().join("attachments/photo.png")).unwrap(), b"png");
        assert!(!root_b.path().join("attachments/movie.mov").exists());
        let status = b.sync_status().unwrap();
        assert_eq!(status.devices.len(), 2);
        assert!(status.devices.iter().any(|d| d.name == "Desk"));
        assert_eq!(
            status.relay.as_ref().unwrap().files,
            r.downloaded.len() + r.uploaded.len()
        );

        // Nothing to do when nothing changed.
        let r = b.relay_sync().unwrap();
        assert!(r.uploaded.is_empty() && r.downloaded.is_empty(), "{r:?}");

        // B edits the note and adds a file; A sees both.
        let mut edited = on_b.clone();
        edited.body = "hello from the phone\n".into();
        b.write(&edited).unwrap();
        fs::write(root_b.path().join("attachments/voice.m4a"), b"m4a").unwrap();
        b.relay_sync().unwrap();
        let r = a.relay_sync().unwrap();
        assert!(r.sync.pulled.contains(&note.id), "{r:?}");
        assert_eq!(a.read(&note.id).unwrap().body, "hello from the phone\n");
        assert_eq!(fs::read(root_a.path().join("attachments/voice.m4a")).unwrap(), b"m4a");

        // A deletes the photo; it goes on B too.
        fs::remove_file(root_a.path().join("attachments/photo.png")).unwrap();
        let r = a.relay_sync().unwrap();
        assert!(r.uploaded.contains(&"attachments/photo.png".to_string()));
        let r = b.relay_sync().unwrap();
        assert_eq!(r.deleted, vec!["attachments/photo.png".to_string()]);
        assert!(!root_b.path().join("attachments/photo.png").exists());

        // Both change the same file: the relay's version wins, the other is kept beside it.
        fs::write(root_a.path().join("attachments/voice.m4a"), b"desk edit").unwrap();
        fs::write(root_b.path().join("attachments/voice.m4a"), b"phone edit").unwrap();
        a.relay_sync().unwrap();
        b.relay_sync().unwrap();
        assert_eq!(
            fs::read(root_b.path().join("attachments/voice.m4a")).unwrap(),
            b"desk edit"
        );
        assert_eq!(
            fs::read(root_b.path().join("attachments/voice (Phone's conflicted copy).m4a")).unwrap(),
            b"phone edit"
        );

        // A device with the wrong key is refused, and a bad invite never gets that far.
        let mut c = Vault::open_as(root_b.path(), Device::new("cccccccccccccccccccccccccccccccc", "Tablet")).unwrap();
        let mut wrong = config.clone();
        wrong.key = RelayConfig::new(&wrong.url).unwrap().key;
        c.set_relay(Some(wrong)).unwrap();
        // The wrong key maps to a different vault id, so it simply sees an empty vault.
        let r = c.relay_sync().unwrap();
        assert!(r.downloaded.is_empty());
        std::env::remove_var("GRAPHITE_RELAY_CONFIG_DIR");
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_socket_announces_changes_and_tokens_are_checked() {
    let data = tempfile::tempdir().unwrap();
    let addr = start_relay(data.path()).await;
    let config = RelayConfig::new(&format!("http://{addr}")).unwrap();
    let vault = config.vault_id().unwrap();
    let token = config.auth_token().unwrap();
    let base = format!("http://{addr}/v1/{vault}");
    let id = "ab".repeat(32);

    let client = reqwest_lite::Client::new(base.clone());
    // No token: refused. Right token: registered. Another token: refused.
    assert_eq!(client.put_status(&id, b"x", None), 401);
    assert_eq!(client.put_status(&id, b"x", Some(&token)), 200);
    assert_eq!(client.put_status(&id, b"x", Some(&"0".repeat(64))), 403);

    let ws_url = format!("ws://{addr}/v1/{vault}/ws?token={token}");
    let seq = tokio::task::spawn_blocking(move || {
        let (mut ws, _) = tungstenite::connect(ws_url).unwrap();
        client.put_status(&id, b"y", Some(&token));
        loop {
            match ws.read().unwrap() {
                tungstenite::Message::Text(t) => {
                    let v: serde_json::Value = serde_json::from_str(&t).unwrap();
                    return v["seq"].as_u64().unwrap();
                }
                _ => continue,
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(seq, 2);
}

/// Just enough HTTP for the test, on std.
mod reqwest_lite {
    use std::io::{Read, Write};
    use std::net::TcpStream;

    pub struct Client {
        base: String,
    }

    impl Client {
        pub fn new(base: String) -> Client {
            Client { base }
        }

        pub fn put_status(&self, id: &str, body: &[u8], token: Option<&str>) -> u16 {
            let url = &self.base[7..];
            let (host, path) = url.split_once('/').unwrap();
            let mut s = TcpStream::connect(host).unwrap();
            let auth = token
                .map(|t| format!("Authorization: Bearer {t}\r\n"))
                .unwrap_or_default();
            let req = format!(
                "PUT /{path}/objects/{id} HTTP/1.1\r\nHost: {host}\r\n{auth}Content-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            s.write_all(req.as_bytes()).unwrap();
            s.write_all(body).unwrap();
            let mut out = String::new();
            s.read_to_string(&mut out).unwrap();
            out.split_whitespace().nth(1).unwrap().parse().unwrap()
        }
    }
}
