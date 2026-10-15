//! Relay transport (M4, I2–I4): sync without a shared folder. A relay is a
//! small server that stores opaque objects for a vault and tells connected
//! devices when one changes. Everything it holds is encrypted on the
//! device with a key the relay never sees, so the relay, its host and the
//! network between learn nothing but sizes and timing.
//!
//! What travels: each device's own `.graphite/sync/<device-id>/` folder
//! (the Automerge documents that folder sync would carry) and the vault's
//! assets up to the large-file limit. Notes themselves never do: a device
//! writes them from the documents it receives, exactly as it does for a
//! shared folder. The relay is therefore just another way to move the
//! files that `sync.rs` already reconciles.
//!
//! The key is 32 random bytes, made on the first device and pasted on the
//! others as an invite (`<relay url>#<key>`), kept outside the vault in the
//! OS config directory. From it the device derives the vault's id on the
//! relay and a bearer token, both by HMAC, so the relay can tell devices
//! that hold the key from ones that do not without being able to read
//! anything. Each object is `GRL1` + a random 24-byte nonce +
//! XChaCha20-Poly1305 over `[path length][path][flags][content]`, with
//! the object id (an HMAC of the path) as associated data; the id is what
//! the relay files it under, so even file names stay private.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chacha20poly1305::aead::{Aead, KeyInit, OsRng, Payload};
use chacha20poly1305::{AeadCore, XChaCha20Poly1305, XNonce};
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::device::is_valid_id;
use crate::error::{Error, Result};
use crate::sync::SYNC_DIR;
use crate::vault::{stat, Vault, STATE_DIR};
use crate::SyncReport;

/// `.graphite/relay/<device-id>.json`: what this device last exchanged.
pub const RELAY_DIR: &str = "relay";
const MAGIC: &[u8; 4] = b"GRL1";
const FLAG_DELETED: u8 = 1;
const MAX_PATH: usize = 4096;
/// Objects on the relay are capped so one runaway file cannot fill it.
pub const MAX_OBJECT_BYTES: u64 = 2 * 1024 * 1024 * 1024;

type HmacSha256 = Hmac<Sha256>;

/// Where a vault syncs to and the key that encrypts it. Lives outside the
/// vault, on each device.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RelayConfig {
    /// `https://relay.example.com` (or `http://` on a LAN).
    pub url: String,
    /// 32 bytes, base64url without padding.
    pub key: String,
}

impl RelayConfig {
    /// A new key for `url`; the first device of a vault makes one.
    pub fn new(url: &str) -> Result<RelayConfig> {
        let key = XChaCha20Poly1305::generate_key(&mut OsRng);
        Ok(RelayConfig {
            url: normalise_url(url)?,
            key: URL_SAFE_NO_PAD.encode(key),
        })
    }

    /// Parse what another device shows as its invite.
    pub fn from_invite(invite: &str) -> Result<RelayConfig> {
        let s = invite.trim();
        let s = s.strip_prefix("graphite-relay:").unwrap_or(s).trim();
        let (url, key) = s
            .rsplit_once('#')
            .ok_or_else(|| Error::Sync("an invite looks like https://relay.example.com#key".into()))?;
        let config = RelayConfig {
            url: normalise_url(url)?,
            key: key.trim().to_string(),
        };
        config.key_bytes()?;
        Ok(config)
    }

    /// The string to paste on another device.
    pub fn invite(&self) -> String {
        format!("{}#{}", self.url, self.key)
    }

    pub(crate) fn key_bytes(&self) -> Result<[u8; 32]> {
        let bytes = URL_SAFE_NO_PAD
            .decode(self.key.trim())
            .map_err(|_| Error::Sync("the key is not valid".into()))?;
        <[u8; 32]>::try_from(bytes).map_err(|_| Error::Sync("the key is not valid".into()))
    }

    /// The vault's name on the relay: derived from the key, says nothing.
    pub fn vault_id(&self) -> Result<String> {
        Ok(hex(&derive(&self.key_bytes()?, b"vault"))[..32].to_string())
    }

    /// What the relay checks before serving the vault. Knowing it does not
    /// help with reading the objects.
    pub fn auth_token(&self) -> Result<String> {
        Ok(hex(&derive(&self.key_bytes()?, b"auth")))
    }

    /// The WebSocket the relay announces changes on.
    pub fn socket_url(&self) -> Result<String> {
        let ws = if let Some(rest) = self.url.strip_prefix("https://") {
            format!("wss://{rest}")
        } else if let Some(rest) = self.url.strip_prefix("http://") {
            format!("ws://{rest}")
        } else {
            return Err(Error::Sync("the relay URL must start with http:// or https://".into()));
        };
        Ok(format!("{ws}/v1/{}/ws", self.vault_id()?))
    }
}

fn normalise_url(url: &str) -> Result<String> {
    let url = url.trim().trim_end_matches('/');
    if !(url.starts_with("http://") || url.starts_with("https://")) || url.len() < 10 {
        return Err(Error::Sync("the relay URL must start with http:// or https://".into()));
    }
    Ok(url.to_string())
}

fn derive(key: &[u8; 32], label: &[u8]) -> [u8; 32] {
    let mut mac = <HmacSha256 as Mac>::new_from_slice(key).expect("any key length works");
    mac.update(label);
    mac.finalize().into_bytes().into()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Encrypts and names objects.
struct Cipher {
    key: [u8; 32],
    aead: XChaCha20Poly1305,
}

impl Cipher {
    fn new(config: &RelayConfig) -> Result<Cipher> {
        let key = config.key_bytes()?;
        let aead = XChaCha20Poly1305::new((&derive(&key, b"content")).into());
        Ok(Cipher { key, aead })
    }

    /// The relay-side name of a vault path (64 hex characters).
    fn object_id(&self, path: &str) -> String {
        let mut mac = <HmacSha256 as Mac>::new_from_slice(&derive(&self.key, b"name")).expect("any key length works");
        mac.update(path.as_bytes());
        hex(&mac.finalize().into_bytes())
    }

    fn seal(&self, path: &str, deleted: bool, content: &[u8]) -> Result<(String, Vec<u8>)> {
        let id = self.object_id(path);
        let mut plain = Vec::with_capacity(path.len() + content.len() + 3);
        plain.extend_from_slice(&(path.len() as u16).to_be_bytes());
        plain.extend_from_slice(path.as_bytes());
        plain.push(if deleted { FLAG_DELETED } else { 0 });
        plain.extend_from_slice(content);
        let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
        let ct = self
            .aead
            .encrypt(
                &nonce,
                Payload {
                    msg: &plain,
                    aad: id.as_bytes(),
                },
            )
            .map_err(|_| Error::Sync("encryption failed".into()))?;
        let mut out = Vec::with_capacity(4 + 24 + ct.len());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ct);
        Ok((id, out))
    }

    /// `(path, deleted, content)` of an object, or an error if it was not
    /// made with this key or was tampered with.
    fn open(&self, id: &str, bytes: &[u8]) -> Result<(String, bool, Vec<u8>)> {
        if bytes.len() < 4 + 24 || &bytes[..4] != MAGIC {
            return Err(Error::Sync("not a relay object".into()));
        }
        let nonce = XNonce::from_slice(&bytes[4..28]);
        let plain = self
            .aead
            .decrypt(
                nonce,
                Payload {
                    msg: &bytes[28..],
                    aad: id.as_bytes(),
                },
            )
            .map_err(|_| Error::Sync("an object on the relay was not made with this key".into()))?;
        if plain.len() < 3 {
            return Err(Error::Sync("truncated relay object".into()));
        }
        let len = u16::from_be_bytes([plain[0], plain[1]]) as usize;
        if len > MAX_PATH || plain.len() < 2 + len + 1 {
            return Err(Error::Sync("truncated relay object".into()));
        }
        let path = String::from_utf8(plain[2..2 + len].to_vec())
            .map_err(|_| Error::Sync("bad path in relay object".into()))?;
        if self.object_id(&path) != id {
            return Err(Error::Sync("relay object filed under the wrong name".into()));
        }
        let deleted = plain[2 + len] & FLAG_DELETED != 0;
        Ok((path, deleted, plain[3 + len..].to_vec()))
    }
}

#[derive(Debug, Deserialize)]
struct ObjectMeta {
    id: String,
    seq: u64,
    #[allow(dead_code)]
    size: u64,
}

#[derive(Debug, Deserialize)]
struct Listing {
    seq: u64,
    objects: Vec<ObjectMeta>,
}

#[derive(Debug, Deserialize)]
struct PutResponse {
    seq: u64,
}

/// HTTP side of the relay protocol.
struct Client {
    agent: ureq::Agent,
    base: String,
    token: String,
}

impl Client {
    fn new(config: &RelayConfig) -> Result<Client> {
        let tls = ureq::tls::TlsConfig::builder()
            .root_certs(ureq::tls::RootCerts::PlatformVerifier)
            .build();
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .tls_config(tls)
            .user_agent("graphite")
            .timeout_global(Some(Duration::from_secs(600)))
            .http_status_as_error(true)
            .build()
            .into();
        Ok(Client {
            agent,
            base: format!("{}/v1/{}", config.url, config.vault_id()?),
            token: format!("Bearer {}", config.auth_token()?),
        })
    }

    fn err(e: ureq::Error) -> Error {
        Error::Sync(match e {
            ureq::Error::StatusCode(401) | ureq::Error::StatusCode(403) => {
                "the relay refused the key: another key is already in use for this vault".into()
            }
            ureq::Error::StatusCode(413) => "the relay refused a file as too large".into(),
            other => format!("relay: {other}"),
        })
    }

    fn list(&self, since: u64) -> Result<Listing> {
        let text = self
            .agent
            .get(format!("{}/objects?since={since}", self.base))
            .header("Authorization", &self.token)
            .call()
            .map_err(Self::err)?
            .body_mut()
            .read_to_string()
            .map_err(Self::err)?;
        serde_json::from_str(&text).map_err(|e| Error::Sync(format!("relay answered oddly: {e}")))
    }

    fn get(&self, id: &str) -> Result<Vec<u8>> {
        self.agent
            .get(format!("{}/objects/{id}", self.base))
            .header("Authorization", &self.token)
            .call()
            .map_err(Self::err)?
            .body_mut()
            .with_config()
            .limit(MAX_OBJECT_BYTES)
            .read_to_vec()
            .map_err(Self::err)
    }

    fn put(&self, id: &str, bytes: &[u8]) -> Result<u64> {
        let text = self
            .agent
            .put(format!("{}/objects/{id}", self.base))
            .header("Authorization", &self.token)
            .header("Content-Type", "application/octet-stream")
            .send(bytes)
            .map_err(Self::err)?
            .body_mut()
            .read_to_string()
            .map_err(Self::err)?;
        let r: PutResponse =
            serde_json::from_str(&text).map_err(|e| Error::Sync(format!("relay answered oddly: {e}")))?;
        Ok(r.seq)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
struct FileState {
    hash: String,
    seq: u64,
    mtime: i64,
    size: i64,
}

/// What this device has exchanged with the relay, so a pass only moves
/// what changed.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct RelayState {
    /// The relay's sequence number this device has seen up to.
    seq: u64,
    last: Option<DateTime<Utc>>,
    files: BTreeMap<String, FileState>,
    #[serde(default)]
    kept_local: Vec<String>,
}

impl RelayState {
    fn load(path: &Path) -> RelayState {
        fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string(self).map_err(std::io::Error::other)?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, text)?;
        fs::rename(&tmp, path)?;
        Ok(())
    }
}

/// What a relay pass moved.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RelayReport {
    /// Vault-relative paths sent to the relay.
    pub uploaded: Vec<String>,
    /// Paths written from the relay (documents and assets).
    pub downloaded: Vec<String>,
    /// Assets removed because another device deleted them.
    pub deleted: Vec<String>,
    /// Assets over the large-file limit, left on this device.
    pub kept_local: Vec<String>,
    /// The note reconciliation that ran with the pass.
    pub sync: SyncReport,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RelayStatus {
    pub url: String,
    pub last: Option<DateTime<Utc>>,
    /// Assets and documents this device has exchanged.
    pub files: usize,
    pub kept_local: Vec<String>,
}

/// Where a vault's relay settings are kept on this device: outside the
/// vault, under the OS config directory (or `GRAPHITE_RELAY_CONFIG_DIR`).
pub fn config_file(root: &Path) -> Option<PathBuf> {
    let dir = match std::env::var_os("GRAPHITE_RELAY_CONFIG_DIR") {
        Some(d) => PathBuf::from(d),
        None => dirs::config_dir()
            .or_else(dirs::home_dir)?
            .join("graphite")
            .join("relays"),
    };
    let name = hex(&Sha256::digest(root.to_string_lossy().as_bytes()))[..16].to_string();
    Some(dir.join(format!("{name}.json")))
}

pub(crate) fn load_config(root: &Path) -> Option<RelayConfig> {
    if let Ok(invite) = std::env::var("GRAPHITE_RELAY") {
        return RelayConfig::from_invite(&invite).ok();
    }
    let text = fs::read_to_string(config_file(root)?).ok()?;
    serde_json::from_str(&text).ok()
}

fn sha256_bytes(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn sha256_file(abs: &Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut f = fs::File::open(abs)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex(&hasher.finalize()))
}

fn write_atomically(abs: &Path, content: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = abs.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = abs.with_extension(format!(
        "{}.relaytmp",
        abs.extension().and_then(|e| e.to_str()).unwrap_or("")
    ));
    fs::write(&tmp, content)?;
    fs::rename(&tmp, abs)
}

impl Vault {
    fn relay_state_path(&self) -> PathBuf {
        self.root()
            .join(STATE_DIR)
            .join(RELAY_DIR)
            .join(format!("{}.json", self.device.id))
    }

    /// The relay this vault syncs through on this device, if any.
    pub fn relay_config(&self) -> Option<&RelayConfig> {
        self.relay.as_ref()
    }

    /// Connect to (or, with `None`, leave) a relay. Stored on this device.
    pub fn set_relay(&mut self, config: Option<RelayConfig>) -> Result<()> {
        if let Some(c) = &config {
            c.key_bytes()?;
        }
        if let Some(file) = config_file(self.root()) {
            match &config {
                Some(c) => {
                    if let Some(dir) = file.parent() {
                        fs::create_dir_all(dir)?;
                    }
                    fs::write(&file, serde_json::to_string_pretty(c).map_err(std::io::Error::other)?)?;
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        let _ = fs::set_permissions(&file, fs::Permissions::from_mode(0o600));
                    }
                }
                None => {
                    let _ = fs::remove_file(&file);
                }
            }
        }
        if config.is_none() {
            let _ = fs::remove_file(self.relay_state_path());
        }
        self.relay = config;
        Ok(())
    }

    pub fn relay_status(&self) -> Option<RelayStatus> {
        let config = self.relay.as_ref()?;
        let state = RelayState::load(&self.relay_state_path());
        Some(RelayStatus {
            url: config.url.clone(),
            last: state.last,
            files: state.files.len(),
            kept_local: state.kept_local,
        })
    }

    /// Whether a path from the relay is one this device accepts: another
    /// device's sync document, or an asset.
    fn relay_path_ok(&self, path: &str) -> bool {
        if path.is_empty() || path.len() > MAX_PATH || path.starts_with('/') || path.contains('\\') {
            return false;
        }
        if path.split('/').any(|seg| seg.is_empty() || seg == "." || seg == "..") {
            return false;
        }
        let sync_prefix = format!("{STATE_DIR}/{SYNC_DIR}/");
        if let Some(rest) = path.strip_prefix(&sync_prefix) {
            return match rest.split_once('/') {
                Some((dev, file)) => {
                    is_valid_id(dev) && dev != self.device.id && !file.contains('/') && !file.is_empty()
                }
                None => false,
            };
        }
        Vault::is_asset_path(path)
    }

    /// The hash of a local file, reusing the recorded one when the file
    /// has not been touched since.
    fn relay_local_hash(&self, abs: &Path, known: Option<&FileState>) -> std::io::Result<(String, i64, i64)> {
        let (mtime, size) = stat(&fs::metadata(abs)?);
        if let Some(k) = known {
            if k.mtime == mtime && k.size == size {
                return Ok((k.hash.clone(), mtime, size));
            }
        }
        // A device file changes its `last_sync` on every pass; only the
        // rest of it is worth sending.
        if abs.file_name().is_some_and(|n| n == "device.json") {
            if let Ok(mut v) = serde_json::from_slice::<serde_json::Value>(&fs::read(abs)?) {
                if let Some(obj) = v.as_object_mut() {
                    obj.remove("last_sync");
                }
                return Ok((sha256_bytes(v.to_string().as_bytes()), mtime, size));
            }
        }
        Ok((sha256_file(abs)?, mtime, size))
    }

    /// Files this device sends: its own sync documents, then every asset
    /// up to the large-file limit. Returns `(path, over_limit)`.
    fn relay_local_files(&self) -> Vec<(String, bool)> {
        let limit = self.large_file_bytes();
        let mut out = Vec::new();
        let my_dir = self.sync_dir().join(&self.device.id);
        if let Ok(entries) = fs::read_dir(&my_dir) {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if e.path().is_file() && name != "seen.json" && !name.ends_with(".tmp") {
                    out.push((format!("{STATE_DIR}/{SYNC_DIR}/{}/{name}", self.device.id), false));
                }
            }
        }
        for entry in ignore::WalkBuilder::new(self.root())
            .hidden(true)
            .git_ignore(false)
            .build()
            .flatten()
        {
            let p = entry.path();
            if !p.is_file() {
                continue;
            }
            let Ok(rel) = self.rel(p) else { continue };
            if !Vault::is_asset_path(&rel) || rel.ends_with(".relaytmp") {
                continue;
            }
            let over = fs::metadata(p).map(|m| m.len() > limit).unwrap_or(true);
            out.push((rel, over));
        }
        out
    }

    /// One exchange with the relay: reconcile notes, pull what other
    /// devices sent, push what changed here, reconcile again.
    pub fn relay_sync(&mut self) -> Result<RelayReport> {
        let config = self
            .relay
            .clone()
            .ok_or_else(|| Error::Sync("this vault is not connected to a relay".into()))?;
        let cipher = Cipher::new(&config)?;
        let client = Client::new(&config)?;
        let state_path = self.relay_state_path();
        let mut state = RelayState::load(&state_path);
        let mut report = RelayReport {
            sync: self.sync_now()?,
            ..Default::default()
        };

        // Pull.
        let listing = client.list(state.seq)?;
        let mine: HashSet<u64> = state.files.values().map(|f| f.seq).collect();
        let mut got_docs = false;
        let mut assets_changed = false;
        for obj in &listing.objects {
            if obj.seq <= state.seq || mine.contains(&obj.seq) {
                continue;
            }
            let bytes = client.get(&obj.id)?;
            let (path, deleted, content) = cipher.open(&obj.id, &bytes)?;
            if !self.relay_path_ok(&path) {
                tracing::warn!("relay: ignoring object for {path}");
                continue;
            }
            let abs = self.root().join(&path);
            let is_doc = path.starts_with(STATE_DIR);
            let known = state.files.get(&path).cloned();
            let local = if abs.is_file() {
                self.relay_local_hash(&abs, known.as_ref()).ok()
            } else {
                None
            };
            let local_edited = match (&local, &known) {
                (Some((h, _, _)), Some(k)) => *h != k.hash,
                (Some(_), None) => true,
                _ => false,
            };
            if deleted {
                if local.is_some() && !(local_edited && !is_doc) {
                    let _ = fs::remove_file(&abs);
                    report.deleted.push(path.clone());
                    assets_changed |= !is_doc;
                }
                state.files.remove(&path);
                continue;
            }
            let hash = sha256_bytes(&content);
            if local.as_ref().is_some_and(|(h, _, _)| *h == hash) {
                // Already here (for example, also delivered by a shared folder).
            } else {
                if local_edited && !is_doc {
                    // Both sides changed the file: the relay's wins, the
                    // local one stays next to it rather than vanishing.
                    let copy = self.materialise_conflict_copy(&path);
                    let _ = fs::rename(&abs, self.root().join(&copy));
                }
                write_atomically(&abs, &content)?;
                report.downloaded.push(path.clone());
                got_docs |= is_doc;
                assets_changed |= !is_doc;
            }
            let (mtime, size) = stat(&fs::metadata(&abs)?);
            state.files.insert(
                path,
                FileState {
                    hash,
                    seq: obj.seq,
                    mtime,
                    size,
                },
            );
        }
        state.seq = state.seq.max(listing.seq);
        state.save(&state_path)?;

        // Merge what came in before pushing, so this device's documents
        // go up already carrying the other devices' changes.
        if got_docs {
            let again = self.sync_now()?;
            report.sync.pulled.extend(again.pulled);
            report.sync.deleted.extend(again.deleted);
            report.sync.conflicts += again.conflicts;
            report.sync.pending = again.pending;
        }

        // Push.
        let mut present = HashSet::new();
        state.kept_local.clear();
        for (path, over) in self.relay_local_files() {
            present.insert(path.clone());
            if over {
                state.kept_local.push(path.clone());
                report.kept_local.push(path);
                continue;
            }
            let abs = self.root().join(&path);
            let known = state.files.get(&path).cloned();
            let Ok((hash, mtime, size)) = self.relay_local_hash(&abs, known.as_ref()) else {
                continue;
            };
            if known.as_ref().is_some_and(|k| k.hash == hash) {
                continue;
            }
            let content = fs::read(&abs)?;
            let (id, bytes) = cipher.seal(&path, false, &content)?;
            let seq = client.put(&id, &bytes)?;
            state.files.insert(path.clone(), FileState { hash, seq, mtime, size });
            state.seq = state.seq.max(seq);
            report.uploaded.push(path);
        }
        let gone: Vec<String> = state
            .files
            .keys()
            .filter(|p| !present.contains(*p) && !p.starts_with(STATE_DIR))
            .cloned()
            .collect();
        for path in gone {
            let (id, bytes) = cipher.seal(&path, true, &[])?;
            let seq = client.put(&id, &bytes)?;
            state.seq = state.seq.max(seq);
            state.files.remove(&path);
            report.uploaded.push(path);
        }
        state.last = Some(Utc::now());
        state.save(&state_path)?;

        if assets_changed {
            self.sync()?;
        }
        Ok(report)
    }

    /// A free name next to `path` for a copy this device made.
    fn materialise_conflict_copy(&self, path: &str) -> String {
        let (stem, ext) = match path.rsplit_once('.') {
            Some((s, e)) if !s.ends_with('/') && !e.contains('/') => (s.to_string(), format!(".{e}")),
            _ => (path.to_string(), String::new()),
        };
        let mut n = 0;
        loop {
            let suffix = if n == 0 { String::new() } else { format!(" {n}") };
            let candidate = format!("{stem} ({}'s conflicted copy{suffix}){ext}", self.device.name);
            if !self.root().join(&candidate).exists() {
                return candidate;
            }
            n += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invites_round_trip_and_derive_stable_names() {
        let a = RelayConfig::new("https://relay.example.com/").unwrap();
        assert_eq!(a.url, "https://relay.example.com");
        let b = RelayConfig::from_invite(&format!("  graphite-relay:{} ", a.invite())).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.vault_id().unwrap(), b.vault_id().unwrap());
        assert_eq!(a.vault_id().unwrap().len(), 32);
        assert_eq!(a.auth_token().unwrap().len(), 64);
        assert_ne!(a.vault_id().unwrap(), &a.auth_token().unwrap()[..32]);
        assert_eq!(
            a.socket_url().unwrap(),
            format!("wss://relay.example.com/v1/{}/ws", a.vault_id().unwrap())
        );
        let other = RelayConfig::new("http://10.0.0.2:7474").unwrap();
        assert_ne!(a.vault_id().unwrap(), other.vault_id().unwrap());
        assert!(RelayConfig::from_invite("relay.example.com#abc").is_err());
        assert!(RelayConfig::from_invite("https://relay.example.com#tooshort").is_err());
        assert!(RelayConfig::new("ftp://x").is_err());
    }

    #[test]
    fn objects_are_opaque_and_tamper_evident() {
        let config = RelayConfig::new("https://r.example").unwrap();
        let c = Cipher::new(&config).unwrap();
        let (id, bytes) = c.seal("attachments/photo.png", false, b"png bytes").unwrap();
        assert_eq!(id.len(), 64);
        assert_eq!(id, c.object_id("attachments/photo.png"));
        assert!(!bytes.windows(5).any(|w| w == b"photo"), "the path must not be visible");
        let (path, deleted, content) = c.open(&id, &bytes).unwrap();
        assert_eq!(path, "attachments/photo.png");
        assert!(!deleted);
        assert_eq!(content, b"png bytes");
        // Another key, a flipped byte or the wrong id all fail.
        let other = Cipher::new(&RelayConfig::new("https://r.example").unwrap()).unwrap();
        assert!(other.open(&id, &bytes).is_err());
        let mut bad = bytes.clone();
        let last = bad.len() - 1;
        bad[last] ^= 1;
        assert!(c.open(&id, &bad).is_err());
        assert!(c.open(&c.object_id("attachments/other.png"), &bytes).is_err());
        let (id2, tomb) = c.seal("attachments/photo.png", true, b"").unwrap();
        assert_eq!(id2, id);
        assert!(c.open(&id2, &tomb).unwrap().1);
    }

    #[test]
    fn only_other_devices_documents_and_assets_are_accepted() {
        let dir = tempfile::tempdir().unwrap();
        let v = Vault::open_as(dir.path(), crate::Device::new("aaaaaaaaaaaaaaaa", "A")).unwrap();
        assert!(v.relay_path_ok("attachments/photo.png"));
        assert!(v.relay_path_ok(".graphite/sync/bbbbbbbbbbbbbbbb/01X.automerge"));
        assert!(
            !v.relay_path_ok(".graphite/sync/aaaaaaaaaaaaaaaa/01X.automerge"),
            "never overwrite our own documents"
        );
        assert!(!v.relay_path_ok(".graphite/sync/bbbbbbbbbbbbbbbb/x/y"));
        assert!(!v.relay_path_ok(".graphite/devices/bbbbbbbbbbbbbbbb/index.sqlite"));
        assert!(!v.relay_path_ok("notes/Plan.md"));
        assert!(!v.relay_path_ok("../etc/passwd"));
        assert!(!v.relay_path_ok("/etc/passwd"));
        assert!(!v.relay_path_ok("attachments/..\\x"));
        assert!(!v.relay_path_ok(""));
    }

    #[test]
    fn relay_settings_live_outside_the_vault() {
        let dir = tempfile::tempdir().unwrap();
        let cfg_dir = tempfile::tempdir().unwrap();
        std::env::set_var("GRAPHITE_RELAY_CONFIG_DIR", cfg_dir.path());
        let mut v = Vault::open(dir.path()).unwrap();
        assert!(v.relay_config().is_none());
        assert!(v.relay_status().is_none());
        assert!(v.relay_sync().is_err());
        let config = RelayConfig::new("http://localhost:7474").unwrap();
        v.set_relay(Some(config.clone())).unwrap();
        assert_eq!(v.relay_config(), Some(&config));
        let reopened = Vault::open(dir.path()).unwrap();
        assert_eq!(reopened.relay_config(), Some(&config));
        assert!(!dir.path().join(".graphite").join("relay.json").exists());
        v.set_relay(None).unwrap();
        assert!(Vault::open(dir.path()).unwrap().relay_config().is_none());
        std::env::remove_var("GRAPHITE_RELAY_CONFIG_DIR");
    }
}
