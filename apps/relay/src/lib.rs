//! The Graphite relay: a store of opaque objects per vault, plus a
//! WebSocket that announces changes. It never sees a key, a note or a file
//! name; devices encrypt everything before it leaves them (see
//! `core/src/relay.rs` for the format).
//!
//! Protocol, all under `/v1/<vault-id>/` with `Authorization: Bearer
//! <token>` (both derived from the vault key on the device; the first
//! token used for a vault id is the one the relay keeps accepting):
//!
//! - `GET  objects?since=<seq>` → `{ "seq": n, "objects": [{ "id", "seq", "size" }] }`,
//!   the newest version of every object changed after `since`.
//! - `GET  objects/<id>` → the bytes.
//! - `PUT  objects/<id>` with the bytes → `{ "seq": n }`.
//! - `GET  ws` (or `ws?token=<token>`) → a WebSocket that sends
//!   `{ "seq": n }` whenever an object changes.
//!
//! Objects are files under `<data>/<vault-id>/`; the catalogue is a SQLite
//! database next to them.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::body::Bytes;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{DefaultBodyLimit, Path as UrlPath, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::broadcast;

pub struct Relay {
    data: PathBuf,
    db: Mutex<Connection>,
    channels: Mutex<HashMap<String, broadcast::Sender<u64>>>,
}

pub type Shared = Arc<Relay>;

impl Relay {
    pub fn open(data: &Path) -> std::io::Result<Shared> {
        std::fs::create_dir_all(data)?;
        let db = Connection::open(data.join("relay.sqlite")).map_err(std::io::Error::other)?;
        db.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS vaults (id TEXT PRIMARY KEY, token_hash TEXT NOT NULL, created INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS objects (vault TEXT NOT NULL, id TEXT NOT NULL, seq INTEGER NOT NULL, size INTEGER NOT NULL, PRIMARY KEY (vault, id));
             CREATE INDEX IF NOT EXISTS objects_seq ON objects (vault, seq);",
        )
        .map_err(std::io::Error::other)?;
        Ok(Arc::new(Relay {
            data: data.to_path_buf(),
            db: Mutex::new(db),
            channels: Mutex::new(HashMap::new()),
        }))
    }

    fn channel(&self, vault: &str) -> broadcast::Sender<u64> {
        let mut channels = self.channels.lock().expect("poisoned");
        channels
            .entry(vault.to_string())
            .or_insert_with(|| broadcast::channel(256).0)
            .clone()
    }

    /// Check a bearer token for a vault, registering it on first use.
    fn authorize(&self, vault: &str, token: &str) -> Result<(), StatusCode> {
        if !is_hex(vault, 32) || !is_hex(token, 64) {
            return Err(StatusCode::UNAUTHORIZED);
        }
        let hash = format!("{:x}", Sha256::digest(token.as_bytes()));
        let db = self.db.lock().expect("poisoned");
        let stored: Option<String> = db
            .query_row("SELECT token_hash FROM vaults WHERE id = ?1", [vault], |r| r.get(0))
            .optional()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        match stored {
            Some(s) if s == hash => Ok(()),
            Some(_) => Err(StatusCode::FORBIDDEN),
            None => {
                db.execute(
                    "INSERT INTO vaults (id, token_hash, created) VALUES (?1, ?2, ?3)",
                    params![vault, hash, now()],
                )
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                Ok(())
            }
        }
    }

    fn list(&self, vault: &str, since: u64) -> rusqlite::Result<Listing> {
        let db = self.db.lock().expect("poisoned");
        let seq: i64 = db.query_row(
            "SELECT COALESCE(MAX(seq), 0) FROM objects WHERE vault = ?1",
            [vault],
            |r| r.get(0),
        )?;
        let seq = seq as u64;
        let mut stmt = db.prepare("SELECT id, seq, size FROM objects WHERE vault = ?1 AND seq > ?2 ORDER BY seq")?;
        let objects = stmt
            .query_map(params![vault, since as i64], |r| {
                Ok(ObjectMeta {
                    id: r.get(0)?,
                    seq: r.get::<_, i64>(1)? as u64,
                    size: r.get::<_, i64>(2)? as u64,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(Listing { seq, objects })
    }

    fn object_path(&self, vault: &str, id: &str) -> PathBuf {
        self.data.join(vault).join(id)
    }

    fn put(&self, vault: &str, id: &str, bytes: &[u8]) -> std::io::Result<u64> {
        let dir = self.data.join(vault);
        std::fs::create_dir_all(&dir)?;
        let tmp = dir.join(format!("{id}.tmp"));
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(&tmp, dir.join(id))?;
        let db = self.db.lock().expect("poisoned");
        let seq: i64 = db
            .query_row(
                "SELECT COALESCE(MAX(seq), 0) + 1 FROM objects WHERE vault = ?1",
                [vault],
                |r| r.get(0),
            )
            .map_err(std::io::Error::other)?;
        db.execute(
            "INSERT INTO objects (vault, id, seq, size) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (vault, id) DO UPDATE SET seq = excluded.seq, size = excluded.size",
            params![vault, id, seq, bytes.len() as i64],
        )
        .map_err(std::io::Error::other)?;
        drop(db);
        let seq = seq as u64;
        let _ = self.channel(vault).send(seq);
        Ok(seq)
    }
}

#[derive(Serialize)]
struct ObjectMeta {
    id: String,
    seq: u64,
    size: u64,
}

#[derive(Serialize)]
struct Listing {
    seq: u64,
    objects: Vec<ObjectMeta>,
}

#[derive(Deserialize)]
struct Since {
    #[serde(default)]
    since: u64,
}

#[derive(Deserialize)]
struct WsQuery {
    token: Option<String>,
}

fn is_hex(s: &str, len: usize) -> bool {
    s.len() == len && s.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn bearer(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(|t| t.trim().to_string())
}

fn auth(relay: &Relay, vault: &str, headers: &HeaderMap, query_token: Option<&str>) -> Result<(), StatusCode> {
    let token = bearer(headers)
        .or_else(|| query_token.map(str::to_string))
        .ok_or(StatusCode::UNAUTHORIZED)?;
    relay.authorize(vault, &token)
}

async fn health() -> &'static str {
    "ok"
}

async fn list_objects(
    State(relay): State<Shared>,
    UrlPath(vault): UrlPath<String>,
    Query(q): Query<Since>,
    headers: HeaderMap,
) -> Response {
    if let Err(code) = auth(&relay, &vault, &headers, None) {
        return code.into_response();
    }
    match relay.list(&vault, q.since) {
        Ok(listing) => Json(listing).into_response(),
        Err(e) => {
            tracing::error!("list: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn get_object(
    State(relay): State<Shared>,
    UrlPath((vault, id)): UrlPath<(String, String)>,
    headers: HeaderMap,
) -> Response {
    if let Err(code) = auth(&relay, &vault, &headers, None) {
        return code.into_response();
    }
    if !is_hex(&id, 64) {
        return StatusCode::NOT_FOUND.into_response();
    }
    match tokio::fs::read(relay.object_path(&vault, &id)).await {
        Ok(bytes) => ([(header::CONTENT_TYPE, "application/octet-stream")], bytes).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn put_object(
    State(relay): State<Shared>,
    UrlPath((vault, id)): UrlPath<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Err(code) = auth(&relay, &vault, &headers, None) {
        return code.into_response();
    }
    if !is_hex(&id, 64) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let relay2 = relay.clone();
    let result = tokio::task::spawn_blocking(move || relay2.put(&vault, &id, &body)).await;
    match result {
        Ok(Ok(seq)) => Json(serde_json::json!({ "seq": seq })).into_response(),
        Ok(Err(e)) => {
            tracing::error!("put: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn ws_handler(
    State(relay): State<Shared>,
    UrlPath(vault): UrlPath<String>,
    Query(q): Query<WsQuery>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    if let Err(code) = auth(&relay, &vault, &headers, q.token.as_deref()) {
        return code.into_response();
    }
    let rx = relay.channel(&vault).subscribe();
    ws.on_upgrade(move |socket| announce(socket, rx))
}

async fn announce(socket: WebSocket, mut rx: broadcast::Receiver<u64>) {
    let (mut tx, mut incoming) = socket.split();
    let mut keepalive = tokio::time::interval(std::time::Duration::from_secs(30));
    loop {
        tokio::select! {
            changed = rx.recv() => {
                let seq = match changed {
                    Ok(seq) => seq,
                    Err(broadcast::error::RecvError::Lagged(_)) => 0,
                    Err(broadcast::error::RecvError::Closed) => break,
                };
                let text = serde_json::json!({ "seq": seq }).to_string();
                if tx.send(Message::Text(text.into())).await.is_err() {
                    break;
                }
            }
            _ = keepalive.tick() => {
                if tx.send(Message::Ping(Bytes::new())).await.is_err() {
                    break;
                }
            }
            msg = incoming.next() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }
}

/// The HTTP application, for `main` and for tests.
pub fn app(relay: Shared, max_object_bytes: usize) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/{vault}/objects", get(list_objects))
        .route("/v1/{vault}/objects/{id}", get(get_object).put(put_object))
        .route("/v1/{vault}/ws", get(ws_handler))
        .layer(DefaultBodyLimit::max(max_object_bytes))
        .with_state(relay)
}

/// Bind and serve until the process is told to stop. Returns the bound
/// address once listening.
pub async fn serve(relay: Shared, addr: SocketAddr, max_object_bytes: usize) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("graphite relay listening on {}", listener.local_addr()?);
    axum::serve(listener, app(relay, max_object_bytes))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
}
