//! Multi-device sync over a shared folder (M4, ADR-003).
//!
//! Every note has an Automerge document whose text, title, tags and
//! properties merge without losing concurrent edits. Each device keeps its
//! own copy of every document under `.graphite/sync/<device-id>/<note-id>
//! .automerge` and only ever writes its own folder; a file-sync client
//! (Dropbox, Syncthing, a USB stick) carries the folders between machines.
//! A sync pass does three things, per note:
//!
//! 1. **Disk into document.** If the note file changed since this device
//!    last looked (its content hash differs from the recorded one) and the
//!    new content is not something another device has already published,
//!    the file is diffed into the document as this device's edit. Missing
//!    files become tombstones after a grace period (or at once when the
//!    note was deleted through Graphite).
//! 2. **Merge.** Every other device's document that changed on disk is
//!    merged in.
//! 3. **Document onto disk.** The merged note is written back if it differs
//!    from the file, moved if its path changed, deleted if tombstoned, and
//!    the index and the recorded hash are updated.
//!
//! Sync clients that find a file changed on two machines keep one and write
//! the other as a "conflicted copy". Those copies are folded into the
//! document as a concurrent edit and removed, so nothing is lost even if a
//! document arrives after the note it describes.
//!
//! Edits a device records are deterministic: a file's new content is
//! diffed against the document state the file was last reconciled at, by
//! an actor derived from that state and content. Two devices that see the
//! same file before each other's documents therefore produce the identical
//! change, and it merges as one.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use automerge::transaction::{CommitOptions, Transactable};
use automerge::{ActorId, AutoCommit, ObjId, ObjType, ReadDoc, ScalarValue, Value, ROOT};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::device::{is_valid_id, Device};
use crate::error::{Error, Result};
use crate::note::{Note, DEFAULT_TYPE};
use crate::vault::{is_conflict_copy, safe_file_stem, to_rel_string, Vault, DEFAULT_NOTES_DIR, STATE_DIR};

/// `.graphite/sync/`
pub const SYNC_DIR: &str = "sync";
const DOC_EXT: &str = "automerge";
const DEVICE_FILE: &str = "device.json";
const SEEN_FILE: &str = "seen.json";
/// How many recent content hashes each device remembers per note. A hash
/// in the list means "a device wrote exactly this", which is how a file
/// delivered by the sync client is told apart from a local edit.
const SEEN_HASHES: usize = 64;
/// A note file that vanished without `Vault::delete` is only tombstoned
/// after this long, so a rename in flight is not mistaken for a deletion.
pub const DEFAULT_DELETE_GRACE: Duration = Duration::from_secs(60);
/// A note file changed by something other than Graphite is absorbed as a
/// local edit only after it has sat unchanged this long, so a note the
/// sync client delivers ahead of its document is merged, not re-recorded.
pub const DEFAULT_ABSORB_DELAY: Duration = Duration::from_secs(20);

/// Result of one `Vault::sync_now`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncReport {
    /// Documents this device (re)published.
    pub pushed: usize,
    /// Notes whose file on disk was written or moved by the merge.
    pub pulled: Vec<String>,
    /// Notes removed from disk because another device deleted them.
    pub deleted: Vec<String>,
    /// Conflicted copies left by the sync client that were folded in.
    pub conflicts: usize,
    /// Notes changed outside Graphite that wait for a later pass.
    pub pending: usize,
    /// Other devices whose documents were seen.
    pub devices: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncDevice {
    pub id: String,
    pub name: String,
    pub last_sync: Option<DateTime<Utc>>,
    /// Number of note documents the device has published.
    pub notes: usize,
    pub this_device: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncStatus {
    /// Whether this vault has a `.graphite/sync/` folder at all.
    pub enabled: bool,
    pub device: Device,
    pub devices: Vec<SyncDevice>,
    pub last_sync: Option<DateTime<Utc>>,
    /// The relay this device syncs through, when connected to one.
    #[serde(default)]
    pub relay: Option<crate::relay::RelayStatus>,
}

#[derive(Debug, Serialize, Deserialize)]
struct DeviceFile {
    id: String,
    name: String,
    last_sync: DateTime<Utc>,
}

/// A content hash of everything sync cares about in a note. Independent of
/// the file's path and of front-matter formatting.
pub fn content_hash(note: &Note) -> String {
    let created = note.created.map(fmt_time);
    let updated = note.updated.map(fmt_time);
    let canon = serde_json::json!([
        note.title,
        note.note_type,
        note.tags,
        note.aliases,
        created,
        updated,
        note.properties,
        note.body
    ]);
    let mut h = Sha256::new();
    h.update(canon.to_string().as_bytes());
    hex(&h.finalize())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn fmt_time(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn parse_time(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc))
}

fn sha_actor(seed: &str) -> ActorId {
    let digest = Sha256::digest(seed.as_bytes());
    ActorId::from(&digest[..16])
}

fn automerge_err(e: automerge::AutomergeError) -> Error {
    Error::Sync(e.to_string())
}

// ----- document shape --------------------------------------------------
//
// ROOT: path, title, type, tags (JSON), aliases (JSON), created, updated
//       (strings, last-writer-wins); state ("live:<content hash>" or
//       "deleted"); body (Text); props (map key -> JSON string).
//
// `state` carries the content hash so that every edit is a new value: a
// delete made at the same time as an edit then shows up as a conflict
// (Automerge drops a put of an unchanged value), and the edit wins.
//
// Which content hashes a device has written or absorbed is kept next to
// its documents in `seen.json`, not in the documents: that way two devices
// at the same content have the same heads, which is what makes absorbed
// edits deterministic.

fn genesis(note_id: &str) -> AutoCommit {
    let mut doc = AutoCommit::new().with_actor(sha_actor(&format!("graphite-note:{note_id}")));
    // Every device builds the identical first change, so documents created
    // independently for the same note share history and merge cleanly.
    doc.put_object(ROOT, "body", ObjType::Text).expect("new doc");
    doc.put_object(ROOT, "props", ObjType::Map).expect("new doc");
    doc.put(ROOT, "state", "new").expect("new doc");
    doc.commit_with(CommitOptions::default().with_time(0));
    doc
}

fn obj(doc: &AutoCommit, key: &str, kind: ObjType) -> Result<ObjId> {
    match doc.get(ROOT, key).map_err(automerge_err)? {
        Some((Value::Object(t), id)) if t == kind => Ok(id),
        _ => Err(Error::Sync(format!("document has no {key}"))),
    }
}

fn get_str(doc: &AutoCommit, obj: &ObjId, key: &str) -> Option<String> {
    match doc.get(obj, key).ok()?? {
        (Value::Scalar(s), _) => match s.as_ref() {
            ScalarValue::Str(s) => Some(s.to_string()),
            _ => None,
        },
        _ => None,
    }
}

const DELETED: &str = "deleted";

fn is_tombstoned(doc: &AutoCommit) -> bool {
    get_str(doc, &ROOT, "state").as_deref() == Some(DELETED)
}

fn tombstone(doc: &mut AutoCommit) -> Result<()> {
    if !is_tombstoned(doc) {
        doc.put(ROOT, "state", DELETED).map_err(automerge_err)?;
    }
    Ok(())
}

fn put_str(doc: &mut AutoCommit, key: &str, value: Option<&str>) -> Result<()> {
    let current = get_str(doc, &ROOT, key);
    if current.as_deref() == value {
        return Ok(());
    }
    match value {
        Some(v) => doc.put(ROOT, key, v).map_err(automerge_err),
        None => doc.delete(ROOT, key).map_err(automerge_err),
    }
}

fn note_from_doc(doc: &AutoCommit, id: &str) -> Result<Note> {
    let body = obj(doc, "body", ObjType::Text)?;
    let props = obj(doc, "props", ObjType::Map)?;
    let list = |key: &str| -> Vec<String> {
        get_str(doc, &ROOT, key)
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    };
    let mut properties = serde_json::Map::new();
    for key in doc.keys(&props) {
        if let Some(raw) = get_str(doc, &props, &key) {
            if let Ok(v) = serde_json::from_str(&raw) {
                properties.insert(key, v);
            }
        }
    }
    let path = get_str(doc, &ROOT, "path").unwrap_or_default();
    Ok(Note {
        id: id.to_string(),
        title: get_str(doc, &ROOT, "title").unwrap_or_else(|| crate::note::title_from_path(&path)),
        path,
        note_type: get_str(doc, &ROOT, "type").unwrap_or_else(|| DEFAULT_TYPE.to_string()),
        tags: list("tags"),
        aliases: list("aliases"),
        created: get_str(doc, &ROOT, "created").and_then(|s| parse_time(&s)),
        updated: get_str(doc, &ROOT, "updated").and_then(|s| parse_time(&s)),
        properties,
        body: doc.text(&body).map_err(automerge_err)?,
    })
}

/// Record `note` in the document as a diff against what it holds.
fn apply_note(doc: &mut AutoCommit, note: &Note, hash: &str) -> Result<()> {
    put_str(doc, "path", Some(&note.path))?;
    put_str(doc, "title", Some(&note.title))?;
    put_str(doc, "type", Some(&note.note_type))?;
    put_str(
        doc,
        "tags",
        Some(&serde_json::to_string(&note.tags).unwrap_or_default()),
    )?;
    put_str(
        doc,
        "aliases",
        Some(&serde_json::to_string(&note.aliases).unwrap_or_default()),
    )?;
    put_str(doc, "created", note.created.map(fmt_time).as_deref())?;
    put_str(doc, "updated", note.updated.map(fmt_time).as_deref())?;
    let props = obj(doc, "props", ObjType::Map)?;
    let existing: Vec<String> = doc.keys(&props).collect();
    for key in &existing {
        if !note.properties.contains_key(key) {
            doc.delete(&props, key.as_str()).map_err(automerge_err)?;
        }
    }
    for (key, value) in &note.properties {
        let raw = value.to_string();
        if get_str(doc, &props, key).as_deref() != Some(raw.as_str()) {
            doc.put(&props, key.as_str(), raw).map_err(automerge_err)?;
        }
    }
    let body = obj(doc, "body", ObjType::Text)?;
    if doc.text(&body).map_err(automerge_err)? != note.body {
        doc.update_text(&body, &note.body).map_err(automerge_err)?;
    }
    put_str(doc, "state", Some(&format!("live:{hash}")))?;
    Ok(())
}

/// Whether the merged document says the note is gone. A delete that raced
/// an edit (neither saw the other) loses: the edit is kept and the
/// conflict resolved in its favour.
fn resolve_deleted(doc: &mut AutoCommit) -> Result<bool> {
    let values = doc.get_all(ROOT, "state").map_err(automerge_err)?;
    let states: Vec<String> = values
        .iter()
        .filter_map(|(v, _)| match v {
            Value::Scalar(s) => match s.as_ref() {
                ScalarValue::Str(s) => Some(s.to_string()),
                _ => None,
            },
            _ => None,
        })
        .collect();
    if states.len() > 1 && states.iter().any(|s| s != DELETED) {
        let live = states.iter().find(|s| *s != DELETED).cloned().unwrap_or_default();
        doc.put(ROOT, "state", live).map_err(automerge_err)?;
        return Ok(false);
    }
    Ok(is_tombstoned(doc))
}

/// `seen.json`: per note, the content hashes a device wrote to disk or
/// absorbed from it, newest last. A file carrying one of these hashes came
/// from a device's document, so it is merged rather than re-absorbed.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Seen(HashMap<String, Vec<String>>);

impl Seen {
    fn load(dir: &Path) -> Seen {
        fs::read_to_string(dir.join(SEEN_FILE))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    fn save(&self, dir: &Path) -> Result<()> {
        write_atomic(
            &dir.join(SEEN_FILE),
            serde_json::to_string(self).unwrap_or_default().as_bytes(),
        )
    }

    fn record(&mut self, id: &str, hash: &str) -> bool {
        let list = self.0.entry(id.to_string()).or_default();
        if list.last().map(String::as_str) == Some(hash) {
            return false;
        }
        list.retain(|h| h != hash);
        list.push(hash.to_string());
        if list.len() > SEEN_HASHES {
            let drop = list.len() - SEEN_HASHES;
            list.drain(..drop);
        }
        true
    }

    fn forget(&mut self, id: &str) -> bool {
        self.0.remove(id).is_some()
    }
}

fn heads_to_string(heads: &[automerge::ChangeHash]) -> String {
    let mut v: Vec<String> = heads.iter().map(|h| h.to_string()).collect();
    v.sort();
    v.join(",")
}

fn heads_from_string(s: &str) -> Vec<automerge::ChangeHash> {
    s.split(',').filter_map(|h| h.parse().ok()).collect()
}

struct RemoteDoc {
    device: String,
    file: String,
    mtime: i64,
    size: i64,
    doc: AutoCommit,
}

/// `notes/A (x's conflicted copy 2026-10-15).md` -> `notes/A.md`.
pub fn conflict_copy_original(rel: &str) -> String {
    static MARKERS: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = MARKERS.get_or_init(|| {
        regex::Regex::new(r"(?i) \((?:[^()]*conflicted copy[^()]*|case conflict[^()]*)\)|\.sync-conflict-[^./]*")
            .unwrap()
    });
    re.replace_all(rel, "").into_owned()
}

fn doc_file_name(id: &str) -> String {
    format!("{id}.{DOC_EXT}")
}

fn note_id_of_doc_file(path: &Path) -> Option<String> {
    if path.extension().and_then(|e| e.to_str()) != Some(DOC_EXT) {
        return None;
    }
    let stem = path.file_stem()?.to_str()?;
    if stem.is_empty() || !stem.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return None;
    }
    Some(stem.to_string())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!("{DOC_EXT}.tmp"));
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

struct NoteSync<'a> {
    id: &'a str,
    my_actor: &'a ActorId,
    remote_docs: Vec<RemoteDoc>,
    copies: Vec<(String, Note)>,
    /// Content hashes some device wrote or absorbed for this note.
    known: HashSet<String>,
    now_ms: i64,
    first_pass: bool,
}

/// How far back in a document's history `absorb` looks for the state an
/// outside change was made on, and how many states it tries at most.
const ANCESTOR_DEPTH: usize = 24;
const MAX_CANDIDATES: usize = 64;

/// Record a file's content as an edit on top of one of `bases`, whichever
/// the content differs from least; with `explore`, recent states in the
/// document's history are candidates too (a file the sync client replaced
/// with another device's copy is an edit on a state that device was at,
/// not on what this device last wrote). The actor is derived from the
/// base and the content, so any device absorbing the same content from
/// the same base produces the identical change.
///
/// Returns the heads of the resulting state (the new change, or the base
/// itself when the content already matched it).
fn absorb(
    doc: &mut AutoCommit,
    id: &str,
    bases: &[Vec<automerge::ChangeHash>],
    explore: bool,
    note: &Note,
    hash: &str,
) -> Result<Vec<automerge::ChangeHash>> {
    let body = obj(doc, "body", ObjType::Text)?;
    let mut candidates: Vec<Vec<automerge::ChangeHash>> = bases.to_vec();
    if explore {
        // Walk back from the current heads: the state right after each
        // change, and the state right before it.
        let mut frontier = doc.get_heads();
        let mut visited: HashSet<automerge::ChangeHash> = HashSet::new();
        for _ in 0..ANCESTOR_DEPTH {
            let mut next = Vec::new();
            for h in &frontier {
                if !visited.insert(*h) {
                    continue;
                }
                let Some(change) = doc.get_change_by_hash(h) else {
                    continue;
                };
                let after = vec![*h];
                if !candidates.contains(&after) {
                    candidates.push(after);
                }
                let before = change.deps().to_vec();
                if !before.is_empty() && !candidates.contains(&before) {
                    candidates.push(before.clone());
                }
                for d in before {
                    if !visited.contains(&d) && !next.contains(&d) {
                        next.push(d);
                    }
                }
            }
            if next.is_empty() || candidates.len() >= MAX_CANDIDATES {
                break;
            }
            frontier = next;
        }
    }
    let mut best: Option<(usize, &Vec<automerge::ChangeHash>)> = None;
    for base in &candidates {
        let Ok(text) = doc.text_at(&body, base) else {
            continue;
        };
        let cost = edit_distance_bound(&text, &note.body);
        if best.is_none_or(|(c, _)| cost < c) {
            best = Some((cost, base));
        }
    }
    let (mut fork, seed) = match best {
        Some((_, base)) => (doc.fork_at(base).map_err(automerge_err)?, heads_to_string(base)),
        // No base we still know (a rebuilt document): diff against now.
        None => (doc.fork(), "now".to_string()),
    };
    // Everything that shapes the ops is in the seed: an actor must never
    // produce two different changes with the same sequence number.
    fork.set_actor(sha_actor(&format!("graphite-edit:{id}:{hash}:{}:{seed}", note.path)));
    apply_note(&mut fork, note, hash)?;
    let change = fork.commit_with(CommitOptions::default().with_time(0));
    doc.merge(&mut fork).map_err(automerge_err)?;
    Ok(match change {
        Some(h) => vec![h],
        None => best.map(|(_, b)| b.clone()).unwrap_or_else(|| doc.get_heads()),
    })
}

/// How much `a` must change to become `b`, counting everything outside
/// the common prefix and suffix. Cheap, and exact for a single edit.
fn edit_distance_bound(a: &str, b: &str) -> usize {
    let a = a.as_bytes();
    let b = b.as_bytes();
    let prefix = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let max_suffix = a.len().min(b.len()) - prefix;
    let suffix = a
        .iter()
        .rev()
        .zip(b.iter().rev())
        .take(max_suffix)
        .take_while(|(x, y)| x == y)
        .count();
    (a.len() - prefix - suffix) + (b.len() - prefix - suffix)
}

impl Vault {
    /// `.graphite/sync/`
    pub fn sync_dir(&self) -> PathBuf {
        self.root().join(STATE_DIR).join(SYNC_DIR)
    }

    fn my_sync_dir(&self) -> PathBuf {
        self.sync_dir().join(&self.device.id)
    }

    /// Whether sync has been turned on for this vault (by any device).
    pub fn sync_enabled(&self) -> bool {
        self.sync_dir().is_dir()
    }

    /// How long a note file may be missing before it counts as deleted.
    pub fn set_delete_grace(&mut self, grace: Duration) {
        self.delete_grace = grace;
    }

    /// How long a file changed outside Graphite must sit still before it
    /// is recorded as a local edit.
    pub fn set_absorb_delay(&mut self, delay: Duration) {
        self.absorb_delay = delay;
    }

    /// Everything known about the devices sharing this vault.
    pub fn sync_status(&self) -> Result<SyncStatus> {
        let mut devices = Vec::new();
        let mut last_sync = None;
        if let Ok(entries) = fs::read_dir(self.sync_dir()) {
            for entry in entries.flatten() {
                let id = entry.file_name().to_string_lossy().into_owned();
                if !entry.path().is_dir() || !is_valid_id(&id) {
                    continue;
                }
                let info: Option<DeviceFile> = fs::read_to_string(entry.path().join(DEVICE_FILE))
                    .ok()
                    .and_then(|t| serde_json::from_str(&t).ok());
                let notes = fs::read_dir(entry.path())
                    .map(|d| d.flatten().filter(|e| note_id_of_doc_file(&e.path()).is_some()).count())
                    .unwrap_or(0);
                let this_device = id == self.device.id;
                if this_device {
                    last_sync = info.as_ref().map(|i| i.last_sync);
                }
                devices.push(SyncDevice {
                    name: info.as_ref().map(|i| i.name.clone()).unwrap_or_else(|| {
                        if this_device {
                            self.device.name.clone()
                        } else {
                            "Unknown device".into()
                        }
                    }),
                    last_sync: info.map(|i| i.last_sync),
                    notes,
                    this_device,
                    id,
                });
            }
        }
        devices.sort_by(|a, b| b.this_device.cmp(&a.this_device).then_with(|| a.name.cmp(&b.name)));
        Ok(SyncStatus {
            enabled: self.sync_enabled(),
            device: self.device.clone(),
            devices,
            last_sync,
            relay: self.relay_status(),
        })
    }

    /// Reconcile the notes on disk, this device's documents and every other
    /// device's documents. Creates `.graphite/sync/` on first use.
    pub fn sync_now(&mut self) -> Result<SyncReport> {
        // Pick up anything the sync client or the user changed since the
        // last walk; the walk is stat-based and cheap.
        self.sync()?;
        // Turning sync on: everything here is this device's own.
        let first_pass = !self.sync_enabled();
        let my_dir = self.my_sync_dir();
        fs::create_dir_all(&my_dir)?;
        let now_ms = Utc::now().timestamp_millis();
        let mut report = SyncReport::default();
        let mut seen = Seen::load(&my_dir);
        let mut known: HashMap<String, HashSet<String>> = HashMap::new();
        for (id, hashes) in &seen.0 {
            known.entry(id.clone()).or_default().extend(hashes.iter().cloned());
        }

        // 0. Other devices' documents that changed since we last merged
        //    them, and everything they have written.
        let mut remote: HashMap<String, Vec<RemoteDoc>> = HashMap::new();
        let mut device_ids = HashSet::new();
        for entry in fs::read_dir(self.sync_dir())?.flatten() {
            let device = entry.file_name().to_string_lossy().into_owned();
            if device == self.device.id || !is_valid_id(&device) || !entry.path().is_dir() {
                continue;
            }
            device_ids.insert(device.clone());
            for (id, hashes) in Seen::load(&entry.path()).0 {
                known.entry(id).or_default().extend(hashes);
            }
            for file in fs::read_dir(entry.path())?.flatten() {
                let Some(note_id) = note_id_of_doc_file(&file.path()) else {
                    continue;
                };
                let name = file.file_name().to_string_lossy().into_owned();
                let meta = fs::metadata(file.path())?;
                let (mtime, size) = crate::vault::stat(&meta);
                if self.index.peer_stat(&device, &name)? == Some((mtime, size)) {
                    continue;
                }
                match fs::read(file.path())
                    .map_err(Error::from)
                    .and_then(|b| AutoCommit::load(&b).map_err(automerge_err))
                {
                    Ok(doc) => remote.entry(note_id).or_default().push(RemoteDoc {
                        device: device.clone(),
                        file: name,
                        mtime,
                        size,
                        doc,
                    }),
                    Err(e) => tracing::warn!("skipping unreadable sync document {}: {e}", file.path().display()),
                }
            }
        }
        report.devices = device_ids.len();

        // Conflicted copies the sync client left next to notes, by note id.
        let mut copies: HashMap<String, Vec<(String, Note)>> = HashMap::new();
        for entry in crate::vault::walk(self.root()).flatten() {
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let rel = self.rel(path)?;
            if !is_conflict_copy(&rel) {
                continue;
            }
            if let Ok(text) = fs::read_to_string(path) {
                // Parse it as if it were the original, so a title derived
                // from the file name does not carry the client's marker.
                if let Ok(note) = Note::parse(&conflict_copy_original(&rel), &text) {
                    if !note.id.is_empty() {
                        copies.entry(note.id.clone()).or_default().push((rel, note));
                    }
                }
            }
        }

        // Every note this pass has to look at.
        let mut ids: HashSet<String> = HashSet::new();
        ids.extend(self.index.list()?.into_iter().map(|n| n.id));
        ids.extend(self.index.sync_states()?.into_iter().map(|(id, _)| id));
        ids.extend(remote.keys().cloned());
        ids.extend(copies.keys().cloned());
        let mut ids: Vec<String> = ids.into_iter().collect();
        ids.sort();

        let me = self.device.id.clone();
        let my_actor = ActorId::try_from(me.as_str()).map_err(|_| Error::Sync("device id is not hex".into()))?;
        self.index.begin_batch()?;
        let mut seen_changed = false;
        let outcome = (|| -> Result<()> {
            for id in ids {
                let ctx = NoteSync {
                    id: &id,
                    my_actor: &my_actor,
                    remote_docs: remote.remove(&id).unwrap_or_default(),
                    copies: copies.remove(&id).unwrap_or_default(),
                    known: known.remove(&id).unwrap_or_default(),
                    now_ms,
                    first_pass,
                };
                seen_changed |= self.sync_note(ctx, &mut seen, &mut report)?;
            }
            Ok(())
        })();
        match outcome {
            Ok(()) => self.index.end_batch()?,
            Err(e) => {
                self.index.abort_batch();
                return Err(e);
            }
        }
        if seen_changed {
            seen.save(&my_dir)?;
        }

        let info = DeviceFile {
            id: me,
            name: self.device.name.clone(),
            last_sync: Utc::now(),
        };
        fs::write(
            my_dir.join(DEVICE_FILE),
            serde_json::to_string_pretty(&info).unwrap_or_default(),
        )?;
        Ok(report)
    }

    /// One note's pass. Returns whether `seen` changed.
    fn sync_note(&mut self, ctx: NoteSync<'_>, seen: &mut Seen, report: &mut SyncReport) -> Result<bool> {
        let NoteSync {
            id,
            my_actor,
            mut remote_docs,
            copies,
            mut known,
            now_ms,
            first_pass,
        } = ctx;
        let mut seen_changed = false;
        let doc_path = self.my_sync_dir().join(doc_file_name(id));
        let (mut doc, is_new) = match fs::read(&doc_path) {
            Ok(bytes) => (AutoCommit::load(&bytes).map_err(automerge_err)?, false),
            Err(_) => (genesis(id), true),
        };
        doc.set_actor(my_actor.clone());
        let heads_before = doc.get_heads();
        let state = self.index.sync_state(id)?;
        // The document state the file on disk was last reconciled at.
        let base_heads = match &state {
            Some(s) if !s.heads.is_empty() => heads_from_string(&s.heads),
            _ => heads_before.clone(),
        };

        // 1. Merge the other devices' documents. A changed file may be an
        //    edit on top of any of: the state it was last reconciled at,
        //    the state a remote device wrote it from (the sync client may
        //    have replaced it with that device's copy), or the merge.
        let mut bases = vec![base_heads.clone()];
        for r in &mut remote_docs {
            let heads = r.doc.get_heads();
            if !bases.contains(&heads) {
                bases.push(heads);
            }
            doc.merge(&mut r.doc).map_err(automerge_err)?;
        }
        let now = doc.get_heads();
        if !bases.contains(&now) {
            bases.push(now);
        }

        // 2. Disk into document.
        let local_path = self.index.path_of(id)?;
        let mut local_hash = None;
        let mut hold = false;
        match &local_path {
            Some(rel) => {
                let note = self.read_by_path(rel)?;
                let hash = content_hash(&note);
                let changed = state.as_ref().map(|s| s.hash != hash).unwrap_or(true);
                if changed && !known.contains(&hash) {
                    // Written through Graphite here: an edit on top of the
                    // file this device last reconciled. Changed by something
                    // else: possibly the sync client delivering a note ahead
                    // of its document, so wait for it to sit still, then
                    // take it as an edit on whichever known state it is
                    // closest to.
                    let dirty = state
                        .as_ref()
                        .is_some_and(|s| s.dirty.as_deref() == Some(hash.as_str()));
                    let settled = state.as_ref().is_some_and(|s| {
                        s.pending.as_deref() == Some(hash.as_str())
                            && s.pending_since
                                .is_some_and(|since| now_ms - since >= self.absorb_delay.as_millis() as i64)
                    });
                    // A Graphite write on top of a file whose previous
                    // content was unexplained (still pending) is an edit on
                    // that content: record the pending content first, as
                    // the edit on some known state it was, then this one
                    // on top of it.
                    let pending = state.as_ref().and_then(|s| {
                        let text = s.pending_text.as_deref()?;
                        Note::parse(rel, text).ok().filter(|p| content_hash(p) != hash)
                    });
                    if dirty {
                        let base = match pending {
                            Some(p) => {
                                let ph = content_hash(&p);
                                let heads = absorb(&mut doc, id, &bases, true, &p, &ph)?;
                                seen_changed |= seen.record(id, &ph);
                                known.insert(ph);
                                vec![heads]
                            }
                            None => bases[..1].to_vec(),
                        };
                        absorb(&mut doc, id, &base, false, &note, &hash)?;
                    } else if settled || first_pass {
                        absorb(&mut doc, id, &bases, true, &note, &hash)?;
                    } else {
                        if state.as_ref().and_then(|s| s.pending.as_deref()) != Some(hash.as_str()) {
                            let text = fs::read_to_string(self.root().join(rel))?;
                            self.index.set_sync_pending(id, &hash, now_ms, &text)?;
                        }
                        report.pending += 1;
                        hold = true;
                    }
                    if !hold {
                        doc.set_actor(my_actor.clone());
                        seen_changed |= seen.record(id, &hash);
                        known.insert(hash.clone());
                    }
                } else if state.as_ref().is_some_and(|s| !s.path.is_empty() && &s.path != rel) {
                    put_str(&mut doc, "path", Some(rel))?;
                }
                local_hash = Some(hash);
            }
            None => match &state {
                Some(s) if s.deleted => tombstone(&mut doc)?,
                Some(s) if !s.hash.is_empty() => match s.missing_since {
                    Some(since) if now_ms - since >= self.delete_grace.as_millis() as i64 => {
                        tombstone(&mut doc)?;
                    }
                    Some(_) => {}
                    None => self.index.set_sync_missing_since(id, Some(now_ms))?,
                },
                _ => {}
            },
        }

        // 3. Conflicted copies the sync client left behind are concurrent
        //    edits; fold them in and remove them.
        for (rel, copy) in &copies {
            let hash = content_hash(copy);
            let current = content_hash(&note_from_doc(&doc, id)?);
            if !known.contains(&hash) && hash != current {
                // Our own write, pushed aside by the sync client, or
                // someone else's concurrent one.
                let ours = state
                    .as_ref()
                    .is_some_and(|s| s.dirty.as_deref() == Some(hash.as_str()));
                let (bases, explore) = if ours { (&bases[..1], false) } else { (&bases[..], true) };
                absorb(&mut doc, id, bases, explore, copy, &hash)?;
                doc.set_actor(my_actor.clone());
                seen_changed |= seen.record(id, &hash);
                known.insert(hash);
            }
            let _ = fs::remove_file(self.root().join(rel));
            report.conflicts += 1;
        }

        // 4. Document onto disk.
        if resolve_deleted(&mut doc)? {
            if let Some(rel) = &local_path {
                fs::remove_file(self.root().join(rel))?;
                self.index.remove_by_path(rel)?;
                report.deleted.push(id.to_string());
            }
            let heads = heads_to_string(&doc.get_heads());
            self.index
                .set_sync_state(id, "", local_path.as_deref().unwrap_or(""), &heads)?;
            self.index.mark_sync_deleted(id)?;
            seen_changed |= seen.forget(id);
        } else if !hold {
            let mut merged = note_from_doc(&doc, id)?;
            let rel = self.materialise_path(id, &merged.path, local_path.as_deref())?;
            if merged.path != rel {
                merged.path = rel.clone();
                put_str(&mut doc, "path", Some(&rel))?;
            }
            // Round-trip through the file format so the hash we record is
            // the hash of what a re-read will produce.
            let text = merged.to_text();
            let merged = Note::parse(&rel, &text)?;
            let hash = content_hash(&merged);
            seen_changed |= seen.record(id, &hash);
            let heads = heads_to_string(&doc.get_heads());

            // Publish the document before touching the note so a client
            // that copies files in order delivers the history first.
            if is_new || doc.get_heads() != heads_before {
                write_atomic(&doc_path, &doc.save())?;
                report.pushed += 1;
            }
            if seen_changed {
                seen.save(&self.my_sync_dir())?;
                seen_changed = false;
            }

            let abs = self.root().join(&rel);
            let mut touched = false;
            if let Some(old) = local_path.as_deref().filter(|old| *old != rel) {
                if let Some(dir) = abs.parent() {
                    fs::create_dir_all(dir)?;
                }
                fs::rename(self.root().join(old), &abs)?;
                self.index.remove_by_path(old)?;
                touched = true;
            }
            if local_hash.as_deref() != Some(hash.as_str()) {
                if let Some(dir) = abs.parent() {
                    fs::create_dir_all(dir)?;
                }
                fs::write(&abs, &text)?;
                touched = true;
            }
            if touched {
                report.pulled.push(id.to_string());
            }
            self.index_file(&rel)?;
            self.index.set_sync_state(id, &hash, &rel, &heads)?;
            for r in &remote_docs {
                self.index.set_peer_stat(&r.device, &r.file, r.mtime, r.size)?;
            }
            return Ok(seen_changed);
        }

        if is_new || doc.get_heads() != heads_before {
            write_atomic(&doc_path, &doc.save())?;
            report.pushed += 1;
        }
        for r in &remote_docs {
            self.index.set_peer_stat(&r.device, &r.file, r.mtime, r.size)?;
        }
        Ok(seen_changed)
    }

    /// Where a merged note should live: the document's path unless that is
    /// taken by a different note, in which case the local path (or a
    /// numbered name next to the wanted one).
    fn materialise_path(&self, id: &str, wanted: &str, local: Option<&str>) -> Result<String> {
        let wanted = if wanted.is_empty() || !wanted.ends_with(".md") || self.safe_join(wanted).is_err() {
            local
                .map(str::to_string)
                .unwrap_or_else(|| format!("{DEFAULT_NOTES_DIR}/{}.md", safe_file_stem(id)))
        } else {
            to_rel_string(Path::new(wanted))
        };
        let taken_by_other = |rel: &str| -> Result<bool> {
            if local == Some(rel) {
                return Ok(false);
            }
            if self.root().join(rel).exists() {
                return Ok(true);
            }
            Ok(self.read_by_path(rel).map(|n| n.id != id).unwrap_or(false))
        };
        if !taken_by_other(&wanted)? {
            return Ok(wanted);
        }
        if let Some(local) = local {
            return Ok(local.to_string());
        }
        let stem = wanted.trim_end_matches(".md");
        for n in 2..1000 {
            let candidate = format!("{stem} ({n}).md");
            if !taken_by_other(&candidate)? {
                return Ok(candidate);
            }
        }
        Err(Error::Sync(format!("no free file name for {wanted}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::Device;
    use std::collections::BTreeMap;

    fn device(n: u8) -> Device {
        Device::new(format!("{:032x}", n as u128 + 0xa0), format!("dev{n}"))
    }

    /// A stand-in for Dropbox: a cloud copy of the shared files plus, per
    /// device, the snapshot it last agreed with the cloud on. Files changed
    /// on both sides since then become a "(conflicted copy)".
    struct Cloud {
        files: BTreeMap<String, Vec<u8>>,
        base: Vec<BTreeMap<String, Vec<u8>>>,
    }

    fn shared_files(root: &Path) -> BTreeMap<String, Vec<u8>> {
        let mut out = BTreeMap::new();
        for entry in walkdir(root) {
            let rel = to_rel_string(entry.strip_prefix(root).unwrap());
            let shared = rel.ends_with(".md") || (rel.starts_with(".graphite/sync/") && !rel.ends_with(".tmp"));
            if shared {
                out.insert(rel, fs::read(&entry).unwrap());
            }
        }
        out
    }

    fn walkdir(root: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for e in fs::read_dir(&dir).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    if p.file_name().is_some_and(|n| n == "devices") && dir.file_name().is_some_and(|n| n == STATE_DIR)
                    {
                        continue;
                    }
                    stack.push(p);
                } else {
                    out.push(p);
                }
            }
        }
        out.sort();
        out
    }

    impl Cloud {
        fn new(n: usize) -> Cloud {
            Cloud {
                files: BTreeMap::new(),
                base: vec![BTreeMap::new(); n],
            }
        }

        /// Upload device `i`'s changes, then download the cloud onto it.
        fn propagate(&mut self, i: usize, root: &Path) {
            let local = shared_files(root);
            let base = &self.base[i];
            // Upload.
            for (rel, bytes) in &local {
                if base.get(rel) == Some(bytes) {
                    continue;
                }
                let cloud_changed = self.files.get(rel) != base.get(rel);
                if cloud_changed && self.files.contains_key(rel) && rel.ends_with(".md") {
                    let stem = rel.trim_end_matches(".md");
                    let copy = format!("{stem} (dev{i}'s conflicted copy).md");
                    self.files.insert(copy, bytes.clone());
                } else if !cloud_changed || !self.files.contains_key(rel) {
                    self.files.insert(rel.clone(), bytes.clone());
                }
            }
            for rel in base.keys() {
                if !local.contains_key(rel) && self.files.get(rel) == base.get(rel) {
                    self.files.remove(rel);
                }
            }
            // Download, history first.
            let mut order: Vec<&String> = self.files.keys().collect();
            order.sort_by_key(|r| !r.starts_with(".graphite/"));
            for rel in order {
                let bytes = &self.files[rel];
                if local.get(rel) != Some(bytes) {
                    let abs = root.join(rel);
                    fs::create_dir_all(abs.parent().unwrap()).unwrap();
                    fs::write(abs, bytes).unwrap();
                }
            }
            for rel in local.keys() {
                if !self.files.contains_key(rel) && base.contains_key(rel) {
                    let _ = fs::remove_file(root.join(rel));
                }
            }
            self.base[i] = self.files.clone();
        }
    }

    fn open(dir: &Path, n: u8) -> Vault {
        let mut v = Vault::open_as(dir, device(n)).unwrap();
        v.set_delete_grace(Duration::ZERO);
        v.set_absorb_delay(Duration::ZERO);
        v
    }

    fn body(v: &Vault, id: &str) -> String {
        v.read(id).unwrap().body
    }

    #[test]
    fn two_devices_merge_concurrent_edits_to_one_note() {
        let a_dir = tempfile::tempdir().unwrap();
        let b_dir = tempfile::tempdir().unwrap();
        let mut cloud = Cloud::new(2);
        let mut a = open(a_dir.path(), 1);
        let note = a.create("Shared", None, None).unwrap();
        let mut n = note.clone();
        n.body = "line one\nline two\n".into();
        a.write(&n).unwrap();
        let r = a.sync_now().unwrap();
        assert_eq!(r.pushed, 1);
        cloud.propagate(0, a_dir.path());
        cloud.propagate(1, b_dir.path());

        let mut b = open(b_dir.path(), 2);
        let r = b.sync_now().unwrap();
        assert_eq!(r.devices, 1);
        assert_eq!(body(&b, &note.id), "line one\nline two\n");

        // Concurrent edits: A prepends, B appends.
        let mut na = a.read(&note.id).unwrap();
        na.body = "zero\nline one\nline two\n".into();
        a.write(&na).unwrap();
        let mut nb = b.read(&note.id).unwrap();
        nb.body = "line one\nline two\nline three\n".into();
        nb.tags = vec!["merged".into()];
        b.write(&nb).unwrap();
        a.sync_now().unwrap();
        b.sync_now().unwrap();
        cloud.propagate(0, a_dir.path());
        cloud.propagate(1, b_dir.path());
        cloud.propagate(0, a_dir.path());
        let ra = a.sync_now().unwrap();
        let rb = b.sync_now().unwrap();
        assert!(
            ra.conflicts + rb.conflicts >= 1,
            "the cloud should have left a conflicted copy"
        );
        cloud.propagate(0, a_dir.path());
        cloud.propagate(1, b_dir.path());
        a.sync_now().unwrap();
        b.sync_now().unwrap();
        cloud.propagate(1, b_dir.path());
        cloud.propagate(0, a_dir.path());
        a.sync_now().unwrap();

        let expected = "zero\nline one\nline two\nline three\n";
        assert_eq!(body(&a, &note.id), expected);
        assert_eq!(body(&b, &note.id), expected);
        assert_eq!(b.read(&note.id).unwrap().tags, vec!["merged".to_string()]);
        assert_eq!(a.read(&note.id).unwrap().tags, vec!["merged".to_string()]);
        assert!(shared_files(a_dir.path()).keys().all(|k| !is_conflict_copy(k)));
        assert_eq!(a.list().unwrap().len(), 1);
        assert_eq!(b.list().unwrap().len(), 1);
    }

    #[test]
    fn deletes_moves_and_new_notes_propagate() {
        let a_dir = tempfile::tempdir().unwrap();
        let b_dir = tempfile::tempdir().unwrap();
        let mut cloud = Cloud::new(2);
        let mut a = open(a_dir.path(), 1);
        let mut b = open(b_dir.path(), 2);
        let keep = a.create("Keep", None, None).unwrap();
        let gone = a.create("Gone", None, None).unwrap();
        a.sync_now().unwrap();
        cloud.propagate(0, a_dir.path());
        cloud.propagate(1, b_dir.path());
        b.sync_now().unwrap();
        assert_eq!(b.list().unwrap().len(), 2);

        // B deletes one note and moves the other; A learns both.
        b.delete(&gone.id).unwrap();
        fs::create_dir_all(b_dir.path().join("notes/Archive")).unwrap();
        fs::rename(
            b_dir.path().join(&keep.path),
            b_dir.path().join("notes/Archive/Keep.md"),
        )
        .unwrap();
        b.sync_now().unwrap();
        cloud.propagate(1, b_dir.path());
        cloud.propagate(0, a_dir.path());
        let r = a.sync_now().unwrap();
        assert_eq!(a.list().unwrap().len(), 1);
        assert_eq!(a.path_of(&keep.id).unwrap().as_deref(), Some("notes/Archive/Keep.md"));
        assert!(a.read(&gone.id).is_err());
        assert!(
            r.deleted.is_empty(),
            "the cloud already removed the file; nothing left to delete"
        );

        // A note created on B while A is offline shows up on A.
        let fresh = b.create("Fresh", Some("task"), Some("notes/Archive")).unwrap();
        b.sync_now().unwrap();
        cloud.propagate(1, b_dir.path());
        cloud.propagate(0, a_dir.path());
        a.sync_now().unwrap();
        let on_a = a.read(&fresh.id).unwrap();
        assert_eq!(on_a.note_type, "task");
        assert_eq!(on_a.path, "notes/Archive/Fresh.md");
    }

    #[test]
    fn a_note_deleted_elsewhere_but_edited_here_survives() {
        let a_dir = tempfile::tempdir().unwrap();
        let b_dir = tempfile::tempdir().unwrap();
        let mut cloud = Cloud::new(2);
        let mut a = open(a_dir.path(), 1);
        let mut b = open(b_dir.path(), 2);
        let note = a.create("Contested", None, None).unwrap();
        a.sync_now().unwrap();
        cloud.propagate(0, a_dir.path());
        cloud.propagate(1, b_dir.path());
        b.sync_now().unwrap();

        b.delete(&note.id).unwrap();
        b.sync_now().unwrap();
        let mut n = a.read(&note.id).unwrap();
        n.body = "still needed\n".into();
        a.write(&n).unwrap();
        a.sync_now().unwrap();
        for _ in 0..3 {
            cloud.propagate(1, b_dir.path());
            cloud.propagate(0, a_dir.path());
            a.sync_now().unwrap();
            b.sync_now().unwrap();
        }
        assert_eq!(body(&a, &note.id), "still needed\n");
        assert_eq!(body(&b, &note.id), "still needed\n");
    }

    #[test]
    fn conflict_copy_names_map_back_to_the_original() {
        assert_eq!(
            conflict_copy_original("notes/A (Jerred's conflicted copy 2026-10-15).md"),
            "notes/A.md"
        );
        assert_eq!(
            conflict_copy_original("notes/A (conflicted copy 2026-10-15 120000).md"),
            "notes/A.md"
        );
        assert_eq!(
            conflict_copy_original("A.sync-conflict-20261015-120000-ABCDEFG.md"),
            "A.md"
        );
        assert_eq!(conflict_copy_original("notes/A (Case Conflict 1).md"), "notes/A.md");
        assert_eq!(conflict_copy_original("notes/A.md"), "notes/A.md");
        assert!(is_conflict_copy("notes/A (Jerred's conflicted copy 2026-10-15).md"));
        assert!(is_conflict_copy("A.sync-conflict-20261015-120000-ABCDEFG.md"));
        assert!(!is_conflict_copy("notes/merge-conflict-notes.md"));
    }

    #[test]
    fn status_lists_devices() {
        let dir = tempfile::tempdir().unwrap();
        let mut v = open(dir.path(), 7);
        assert!(!v.sync_enabled());
        let s = v.sync_status().unwrap();
        assert!(!s.enabled);
        assert!(s.devices.is_empty());
        v.create("One", None, None).unwrap();
        v.sync_now().unwrap();
        assert!(v.sync_enabled());
        let s = v.sync_status().unwrap();
        assert_eq!(s.devices.len(), 1);
        assert!(s.devices[0].this_device);
        assert_eq!(s.devices[0].name, "dev7");
        assert_eq!(s.devices[0].notes, 1);
        assert!(s.last_sync.is_some());
    }

    #[test]
    fn same_file_absorbed_on_two_devices_is_one_change() {
        let a_dir = tempfile::tempdir().unwrap();
        let b_dir = tempfile::tempdir().unwrap();
        let text = "---\nid: 01HZZZZZZZZZZZZZZZZZZZZZZZ\n---\nHello from a file.\n";
        for d in [&a_dir, &b_dir] {
            fs::create_dir_all(d.path().join("notes")).unwrap();
            fs::write(d.path().join("notes/Plain.md"), text).unwrap();
        }
        let mut a = open(a_dir.path(), 1);
        let mut b = open(b_dir.path(), 2);
        a.sync_now().unwrap();
        b.sync_now().unwrap();
        // Hand B's document to A without going through the cloud.
        let from = b_dir.path().join(".graphite/sync").join(device(2).id);
        let to = a_dir.path().join(".graphite/sync").join(device(2).id);
        fs::create_dir_all(&to).unwrap();
        for e in fs::read_dir(&from).unwrap().flatten() {
            fs::copy(e.path(), to.join(e.file_name())).unwrap();
        }
        let r = a.sync_now().unwrap();
        assert!(r.pulled.is_empty(), "identical absorptions must not change the file");
        assert_eq!(body(&a, "01HZZZZZZZZZZZZZZZZZZZZZZZ"), "Hello from a file.\n");
    }

    mod convergence {
        use super::*;
        use proptest::prelude::*;

        #[derive(Debug, Clone)]
        enum Op {
            Append { dev: usize, note: usize },
            Create { dev: usize },
            Delete { dev: usize, note: usize },
            Retitle { dev: usize, note: usize },
            Sync { dev: usize },
            Propagate { dev: usize },
        }

        fn op(devs: usize) -> impl Strategy<Value = Op> {
            prop_oneof![
                4 => (0..devs, 0..6usize).prop_map(|(dev, note)| Op::Append { dev, note }),
                1 => (0..devs).prop_map(|dev| Op::Create { dev }),
                1 => (0..devs, 0..6usize).prop_map(|(dev, note)| Op::Delete { dev, note }),
                1 => (0..devs, 0..6usize).prop_map(|(dev, note)| Op::Retitle { dev, note }),
                3 => (0..devs).prop_map(|dev| Op::Sync { dev }),
                3 => (0..devs).prop_map(|dev| Op::Propagate { dev }),
            ]
        }

        /// Run `ops` on `devs` devices sharing one cloud, let everything
        /// settle, and check that the devices agree and no appended line
        /// was lost or duplicated. `trace` prints every step.
        fn run(devs: usize, ops: &[Op], trace: bool) -> std::result::Result<(), String> {
            let dirs: Vec<_> = (0..devs).map(|_| tempfile::tempdir().unwrap()).collect();
            let mut cloud = Cloud::new(devs);
            let mut vaults: Vec<Vault> = dirs
                .iter()
                .enumerate()
                .map(|(i, d)| open(d.path(), i as u8 + 1))
                .collect();
            let show = |vaults: &[Vault], ids: &[String], label: &str| {
                if !trace {
                    return;
                }
                for v in vaults {
                    let notes: Vec<String> = ids
                        .iter()
                        .map(|id| match v.read(id) {
                            Ok(n) => format!("{}={:?}/{:?}", &id[id.len() - 4..], n.title, n.body),
                            Err(_) => format!("{}=gone", &id[id.len() - 4..]),
                        })
                        .collect();
                    eprintln!("{label} [{}] {}", v.device().name, notes.join(" "));
                }
            };
            // Seed: three notes created on device 0 and spread everywhere.
            let mut ids: Vec<String> = Vec::new();
            for i in 0..3 {
                ids.push(vaults[0].create(&format!("Note {i}"), None, None).unwrap().id);
            }
            vaults[0].sync_now().unwrap();
            for i in 0..devs {
                cloud.propagate(i, dirs[i].path());
                vaults[i].sync_now().unwrap();
            }
            let mut tokens: Vec<(String, String)> = Vec::new();
            let mut deleted: HashSet<String> = HashSet::new();
            let mut counter = 0usize;
            for o in ops {
                if trace {
                    eprintln!("--- {o:?}");
                }
                match *o {
                    Op::Append { dev, note } if dev < devs => {
                        let id = ids[note % ids.len()].clone();
                        if let Ok(mut n) = vaults[dev].read(&id) {
                            counter += 1;
                            let tok = format!("tok{counter}d{dev}");
                            n.body.push_str(&format!("{tok}\n"));
                            vaults[dev].write(&n).unwrap();
                            // The app syncs right after a write.
                            let r = vaults[dev].sync_now().unwrap();
                            if trace {
                                eprintln!("    {r:?}");
                            }
                            tokens.push((id, tok));
                        }
                    }
                    Op::Create { dev } if dev < devs => {
                        counter += 1;
                        let n = vaults[dev].create(&format!("New {counter}"), None, None).unwrap();
                        ids.push(n.id);
                    }
                    Op::Delete { dev, note } if dev < devs => {
                        let id = ids[note % ids.len()].clone();
                        if vaults[dev].delete(&id).is_ok() {
                            deleted.insert(id);
                        }
                    }
                    Op::Retitle { dev, note } if dev < devs => {
                        let id = ids[note % ids.len()].clone();
                        if let Ok(mut n) = vaults[dev].read(&id) {
                            counter += 1;
                            n.title = format!("Title {counter}");
                            vaults[dev].write(&n).unwrap();
                        }
                    }
                    Op::Sync { dev } if dev < devs => {
                        let r = vaults[dev].sync_now().unwrap();
                        if trace {
                            eprintln!("    {r:?}");
                        }
                    }
                    Op::Propagate { dev } if dev < devs => {
                        cloud.propagate(dev, dirs[dev].path());
                        // The app syncs when the watcher sees files change.
                        let r = vaults[dev].sync_now().unwrap();
                        if trace {
                            eprintln!("    {r:?}");
                        }
                    }
                    _ => {}
                }
                show(&vaults, &ids, "   ");
            }
            // Let everything settle.
            for round in 0..4 {
                for i in 0..devs {
                    let r = vaults[i].sync_now().unwrap();
                    if trace {
                        eprintln!("--- settle {round} sync {i}: {r:?}");
                    }
                    show(&vaults, &ids, "   ");
                    cloud.propagate(i, dirs[i].path());
                    let r = vaults[i].sync_now().unwrap();
                    if trace {
                        eprintln!("--- settle {round} propagate {i}: {r:?}");
                    }
                    show(&vaults, &ids, "   ");
                }
            }
            for v in &mut vaults {
                v.sync_now().unwrap();
            }
            let snapshot = |v: &Vault| -> BTreeMap<String, (String, String)> {
                v.list()
                    .unwrap()
                    .into_iter()
                    .map(|s| {
                        let n = v.read(&s.id).unwrap();
                        (s.id, (s.path, content_hash(&n)))
                    })
                    .collect()
            };
            let first = snapshot(&vaults[0]);
            for v in &vaults[1..] {
                if snapshot(v) != first {
                    return Err(format!("devices disagree: {:?} vs {:?}", first, snapshot(v)));
                }
            }
            for (id, tok) in &tokens {
                if deleted.contains(id) {
                    continue;
                }
                let n = vaults[0].read(id).map_err(|e| format!("note {id} missing: {e}"))?;
                match n.body.matches(tok.as_str()).count() {
                    1 => {}
                    0 => return Err(format!("token {tok} lost from note {id}")),
                    n => return Err(format!("token {tok} appears {n} times in note {id}")),
                }
            }
            for d in &dirs {
                if shared_files(d.path()).keys().any(|k| is_conflict_copy(k)) {
                    return Err("conflicted copy left behind".into());
                }
            }
            Ok(())
        }

        /// Sequences the property test once failed on; replayed with
        /// `SYNC_TRACE=1` they print every step.
        #[test]
        fn replays() {
            use Op::*;
            let trace = std::env::var("SYNC_TRACE").is_ok();
            let cases: Vec<(usize, Vec<Op>)> = vec![
                (
                    2,
                    vec![
                        Append { dev: 0, note: 4 },
                        Propagate { dev: 0 },
                        Append { dev: 0, note: 1 },
                        Propagate { dev: 1 },
                    ],
                ),
                (
                    2,
                    vec![
                        Append { dev: 0, note: 1 },
                        Append { dev: 1, note: 1 },
                        Propagate { dev: 1 },
                        Propagate { dev: 0 },
                        Append { dev: 0, note: 4 },
                    ],
                ),
                (
                    2,
                    vec![
                        Append { dev: 0, note: 3 },
                        Create { dev: 0 },
                        Create { dev: 0 },
                        Retitle { dev: 1, note: 5 },
                        Propagate { dev: 1 },
                        Propagate { dev: 0 },
                    ],
                ),
                (
                    3,
                    vec![
                        Append { dev: 0, note: 0 },
                        Delete { dev: 0, note: 1 },
                        Create { dev: 0 },
                        Create { dev: 0 },
                        Append { dev: 2, note: 0 },
                        Append { dev: 1, note: 5 },
                        Retitle { dev: 1, note: 5 },
                        Propagate { dev: 1 },
                        Propagate { dev: 2 },
                    ],
                ),
                (
                    3,
                    vec![
                        Append { dev: 1, note: 2 },
                        Retitle { dev: 2, note: 2 },
                        Append { dev: 0, note: 0 },
                        Append { dev: 0, note: 0 },
                        Append { dev: 0, note: 0 },
                        Propagate { dev: 2 },
                        Retitle { dev: 1, note: 2 },
                        Propagate { dev: 1 },
                    ],
                ),
                (
                    3,
                    vec![
                        Append { dev: 1, note: 1 },
                        Retitle { dev: 1, note: 1 },
                        Propagate { dev: 1 },
                        Propagate { dev: 2 },
                        Append { dev: 2, note: 1 },
                    ],
                ),
                (
                    3,
                    vec![
                        Append { dev: 2, note: 1 },
                        Retitle { dev: 1, note: 1 },
                        Propagate { dev: 1 },
                        Propagate { dev: 2 },
                        Append { dev: 2, note: 1 },
                    ],
                ),
            ];
            for (i, (devs, ops)) in cases.iter().enumerate() {
                if trace {
                    eprintln!("===== case {i}");
                }
                if let Err(e) = run(*devs, ops, trace) {
                    panic!("case {i}: {e}");
                }
            }
        }

        proptest! {
            #![proptest_config(ProptestConfig { cases: 24, max_shrink_iters: 200, .. ProptestConfig::default() })]
            #[test]
            fn devices_converge_without_losing_edits(
                devs in 2..4usize,
                ops in proptest::collection::vec(op(3), 1..40),
            ) {
                if let Err(e) = run(devs, &ops, false) {
                    prop_assert!(false, "{}", e);
                }
            }
        }
    }
}
