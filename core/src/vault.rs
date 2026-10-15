//! A vault is a folder. This module reads and writes notes in it and keeps
//! the index under `.graphite/` in step with the files.

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

use chrono::{DateTime, Utc};
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};

use crate::assets::{self, Asset, DerivedState};
use crate::device::{local_device, Device};
use crate::error::{Error, Result};
use crate::index::{Backlink, Index, ItemFilter, ItemRow, SearchHit};
use crate::note::{new_id, Note, NoteSummary};
use crate::tasks::{self, Item, ItemKind, ItemStatus, Progress};

pub const STATE_DIR: &str = ".graphite";
pub const INDEX_FILE: &str = "index.sqlite";
/// Per-device state (the index) lives under `.graphite/devices/<id>/` so
/// machines sharing a vault folder never write the same database.
pub const DEVICES_DIR: &str = "devices";
pub const DEFAULT_NOTES_DIR: &str = "notes";
pub const ATTACHMENTS_DIR: &str = "attachments";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultInfo {
    pub root: String,
    pub name: String,
    pub note_count: usize,
    #[serde(default)]
    pub asset_count: usize,
}

pub struct Vault {
    root: PathBuf,
    pub(crate) index: Index,
    pub(crate) device: Device,
    pub(crate) delete_grace: std::time::Duration,
    pub(crate) absorb_delay: std::time::Duration,
    /// Relay settings for this vault on this device (see `relay.rs`).
    pub(crate) relay: Option<crate::relay::RelayConfig>,
}

impl Vault {
    /// Open (or initialise) a vault at `root`. Creates `.graphite/`,
    /// `notes/` and `attachments/` if missing, then brings the index up to
    /// date with the files on disk.
    pub fn open(root: impl AsRef<Path>) -> Result<Vault> {
        Self::open_as(root, local_device())
    }

    /// `open`, but as a given device. Tests use it to run several devices
    /// against copies of one vault in a single process.
    pub fn open_as(root: impl AsRef<Path>, device: Device) -> Result<Vault> {
        let root = root
            .as_ref()
            .canonicalize()
            .map_err(|_| Error::NotAVault(root.as_ref().to_path_buf()))?;
        if !root.is_dir() {
            return Err(Error::NotAVault(root));
        }
        let state = root.join(STATE_DIR);
        let device_dir = state.join(DEVICES_DIR).join(&device.id);
        fs::create_dir_all(&device_dir)?;
        fs::create_dir_all(root.join(DEFAULT_NOTES_DIR))?;
        fs::create_dir_all(root.join(ATTACHMENTS_DIR))?;
        // Before M4 the index sat directly in `.graphite/`; it is a cache,
        // so just drop it.
        for legacy in [INDEX_FILE, "index.sqlite-wal", "index.sqlite-shm"] {
            let _ = fs::remove_file(state.join(legacy));
        }
        let index = Index::open(&device_dir.join(INDEX_FILE))?;
        let relay = crate::relay::load_config(&root);
        let mut vault = Vault {
            root,
            index,
            device,
            delete_grace: crate::sync::DEFAULT_DELETE_GRACE,
            absorb_delay: crate::sync::DEFAULT_ABSORB_DELAY,
            relay,
        };
        vault.sync()?;
        Ok(vault)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The device this vault was opened as.
    pub fn device(&self) -> &Device {
        &self.device
    }

    pub fn info(&self) -> Result<VaultInfo> {
        Ok(VaultInfo {
            root: self.root.to_string_lossy().into_owned(),
            name: self
                .root
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Vault".into()),
            note_count: self.index.count()?,
            asset_count: self.index.asset_count()?,
        })
    }

    /// Walk the vault and (re)index every note and asset whose mtime/size
    /// changed; drop index rows for files that disappeared. Notes without an
    /// `id` get one written into their front-matter. Assets are recorded
    /// with their cheap metadata and left for `derive_assets` to finish.
    pub fn sync(&mut self) -> Result<usize> {
        self.index.begin_batch()?;
        match self.sync_inner() {
            Ok(n) => {
                self.index.end_batch()?;
                Ok(n)
            }
            Err(e) => {
                self.index.abort_batch();
                Err(e)
            }
        }
    }

    fn sync_inner(&mut self) -> Result<usize> {
        let mut seen = std::collections::HashSet::new();
        let mut seen_assets = std::collections::HashSet::new();
        let mut changed = 0usize;
        for entry in WalkBuilder::new(&self.root)
            .hidden(true)
            .git_ignore(false)
            .filter_entry(|e| e.file_name() != STATE_DIR)
            .build()
            .flatten()
        {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let rel = self.rel(path)?;
            let meta = fs::metadata(path)?;
            let (mtime, size) = stat(&meta);
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                if is_conflict_copy(&rel) {
                    continue;
                }
                if self.index.asset_stat(&rel)? != Some((mtime, size)) {
                    self.index_asset(&rel, &meta)?;
                    changed += 1;
                }
                seen_assets.insert(rel);
                continue;
            }
            if is_conflict_copy(&rel) {
                // A sync client's duplicate of a note both devices edited;
                // `sync_now` folds it into the note and removes it.
                continue;
            }
            if self.index.stat(&rel)? == Some((mtime, size)) {
                seen.insert(rel);
                continue;
            }
            if self.index_file(&rel).is_ok() {
                changed += 1;
            } else {
                tracing::warn!("skipping unreadable note {rel}");
            }
            seen.insert(rel);
        }
        for stale in self.index.all_paths()? {
            if !seen.contains(&stale) {
                self.index.remove_by_path(&stale)?;
                changed += 1;
            }
        }
        for stale in self.index.all_asset_paths()? {
            if !seen_assets.contains(&stale) {
                self.index.remove_asset(&stale)?;
                changed += 1;
            }
        }
        Ok(changed)
    }

    /// Full rebuild from disk, dropping derived previews and text too.
    pub fn rebuild(&mut self) -> Result<usize> {
        self.index.clear()?;
        let derived = self.derived_dir();
        if derived.is_dir() {
            let _ = fs::remove_dir_all(&derived);
        }
        self.sync()
    }

    /// Record (or refresh) an asset's cheap metadata and mark it pending.
    fn index_asset(&mut self, rel: &str, meta: &fs::Metadata) -> Result<()> {
        let (_, size) = stat(meta);
        let modified: DateTime<Utc> = meta.modified().map(DateTime::from).unwrap_or_else(|_| Utc::now());
        let abs = self.root.join(rel);
        let asset = assets::probe(&abs, rel, size, modified);
        self.index.upsert_asset(&asset, None)
    }

    pub(crate) fn upsert_asset(&mut self, asset: &Asset) -> Result<()> {
        let text = match (&asset.derived, &asset.text) {
            (DerivedState::Done, Some(rel)) => fs::read_to_string(self.safe_join(rel)?).ok(),
            _ => None,
        };
        self.index.upsert_asset(asset, text.as_deref())
    }

    /// Re-read one file (after an external change) and update the index.
    /// Returns the note's id, or `None` if the file is gone.
    pub fn reindex_path(&mut self, abs_or_rel: &Path) -> Result<Option<String>> {
        let rel = if abs_or_rel.is_absolute() {
            self.rel(abs_or_rel)?
        } else {
            to_rel_string(abs_or_rel)
        };
        let abs = self.root.join(&rel);
        if !abs.is_file() {
            self.index.remove_by_path(&rel)?;
            self.index.remove_asset(&rel)?;
            return Ok(None);
        }
        if is_conflict_copy(&rel) {
            return Ok(None);
        }
        if abs.extension().and_then(|e| e.to_str()) != Some("md") {
            if is_hidden(&rel) {
                return Ok(None);
            }
            let meta = fs::metadata(&abs)?;
            self.index_asset(&rel, &meta)?;
            return Ok(None);
        }
        self.index_file(&rel).map(Some)
    }

    /// Whether a path is an asset the vault would index, i.e. not a note,
    /// not hidden and not under `.graphite/`.
    pub fn is_asset_path(rel: &str) -> bool {
        !rel.to_lowercase().ends_with(".md") && !is_hidden(rel)
    }

    pub(crate) fn index_file(&mut self, rel: &str) -> Result<String> {
        let abs = self.root.join(rel);
        let mut text = fs::read_to_string(&abs)?;
        let mut note = Note::parse(rel, &text)?;
        if note.id.is_empty() {
            note.id = new_id();
            // Minting an id is the one write Graphite does to a file it did
            // not author: the id is what keeps links stable across renames.
            text = note.to_text();
            fs::write(&abs, &text)?;
        }
        let meta = fs::metadata(&abs)?;
        let (mtime, size) = stat(&meta);
        self.index.upsert(&note, mtime, size)?;
        if let Err(e) = self.record_version(&note, &text) {
            tracing::warn!("history for {rel}: {e}");
        }
        Ok(note.id)
    }

    /// Tell sync that Graphite itself wrote the note at `rel`, so the next
    /// pass takes its content as a local edit. Only matters once sync is
    /// on.
    fn note_written(&mut self, rel: &str) -> Result<()> {
        if !self.sync_enabled() {
            return Ok(());
        }
        let note = self.read_by_path(rel)?;
        self.index.mark_sync_dirty(&note.id, &crate::sync::content_hash(&note))
    }

    pub fn list(&self) -> Result<Vec<NoteSummary>> {
        self.index.list()
    }

    pub fn read(&self, id: &str) -> Result<Note> {
        let rel = self.index.path_of(id)?.ok_or_else(|| Error::NoteNotFound(id.into()))?;
        let text = fs::read_to_string(self.root.join(&rel))?;
        let mut note = Note::parse(&rel, &text)?;
        if note.id.is_empty() {
            note.id = id.to_string();
        }
        Ok(note)
    }

    pub fn read_by_path(&self, rel: &str) -> Result<Note> {
        let text = fs::read_to_string(self.safe_join(rel)?)?;
        Note::parse(rel, &text)
    }

    /// Write a note's body (and optionally its title/properties) back to
    /// disk and reindex it. `updated` is bumped.
    pub fn write(&mut self, note: &Note) -> Result<Note> {
        let rel = self
            .index
            .path_of(&note.id)?
            .ok_or_else(|| Error::NoteNotFound(note.id.clone()))?;
        let mut n = note.clone();
        n.path = rel.clone();
        n.updated = Some(Utc::now());
        if n.created.is_none() {
            n.created = n.updated;
        }
        fs::write(self.root.join(&rel), n.to_text())?;
        self.index_file(&rel)?;
        self.note_written(&rel)?;
        Ok(n)
    }

    /// Create a new note. `folder` is relative to the vault (defaults to
    /// `notes/`). The file name is derived from the title.
    pub fn create(&mut self, title: &str, note_type: Option<&str>, folder: Option<&str>) -> Result<Note> {
        let folder = folder.unwrap_or(DEFAULT_NOTES_DIR);
        let file = format!("{}.md", safe_file_stem(title));
        let rel = to_rel_string(&Path::new(folder).join(file));
        let abs = self.safe_join(&rel)?;
        if abs.exists() {
            return Err(Error::NoteExists(abs));
        }
        if let Some(parent) = abs.parent() {
            fs::create_dir_all(parent)?;
        }
        let now = Utc::now();
        let note = Note {
            id: new_id(),
            path: rel.clone(),
            title: title.trim().to_string(),
            note_type: note_type.unwrap_or(crate::note::DEFAULT_TYPE).to_string(),
            tags: Vec::new(),
            aliases: Vec::new(),
            created: Some(now),
            updated: Some(now),
            properties: Default::default(),
            body: String::new(),
        };
        fs::write(&abs, note.to_text())?;
        self.index_file(&rel)?;
        self.note_written(&rel)?;
        Ok(note)
    }

    /// Resolve a link target to a note id, creating the note if asked.
    pub fn resolve(&self, target: &str) -> Result<Option<String>> {
        self.index.resolve(target)
    }

    pub fn resolve_or_create(&mut self, target: &str) -> Result<String> {
        if let Some(id) = self.index.resolve(target)? {
            return Ok(id);
        }
        Ok(self.create(target, None, None)?.id)
    }

    pub fn delete(&mut self, id: &str) -> Result<()> {
        let rel = self.index.path_of(id)?.ok_or_else(|| Error::NoteNotFound(id.into()))?;
        fs::remove_file(self.root.join(&rel))?;
        self.index.remove_by_path(&rel)?;
        // A deliberate delete becomes a tombstone on the next sync; an
        // unexplained missing file waits out a grace period first.
        self.index.mark_sync_deleted(id)?;
        Ok(())
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
        self.index.search(query, limit)
    }

    pub fn backlinks(&self, id: &str) -> Result<Vec<Backlink>> {
        self.index.backlinks(id)
    }

    pub fn outgoing(&self, id: &str) -> Result<Vec<(String, Option<String>)>> {
        self.index.outgoing(id)
    }

    pub fn unlinked_mentions(&self, id: &str) -> Result<Vec<NoteSummary>> {
        self.index.unlinked_mentions(id)
    }

    pub fn complete_title(&self, prefix: &str, limit: usize) -> Result<Vec<NoteSummary>> {
        self.index.complete_title(prefix, limit)
    }

    pub fn summary(&self, id: &str) -> Result<Option<NoteSummary>> {
        self.index.summary(id)
    }

    /// Tasks and gaps across the vault (see `ItemFilter`).
    pub fn items(&self, filter: &ItemFilter) -> Result<Vec<ItemRow>> {
        self.index.items(filter)
    }

    /// Task roll-up for a project: every task attributed to it, in any note.
    pub fn progress(&self, project_id: &str) -> Result<Progress> {
        let rows = self.index.items(&ItemFilter {
            project: Some(project_id.to_string()),
            ..Default::default()
        })?;
        Ok(Progress::of(rows.iter().map(|r| &r.item)))
    }

    /// Change a task's checkbox (`[ ]`, `[/]`, `[x]`, `[-]`) on one line of a
    /// note. Gaps keep their `[?]`; use `answer_gap` for those.
    pub fn set_item_status(&mut self, note_id: &str, line: usize, status: ItemStatus) -> Result<Item> {
        let mut note = self.read(note_id)?;
        let current = self.item_at(&note, line)?;
        if current.kind == ItemKind::Gap {
            return Err(Error::NotAnItem {
                note: note_id.into(),
                line,
            });
        }
        note.body = tasks::set_marker(&note.body, line, status.task_marker()).ok_or_else(|| Error::NotAnItem {
            note: note_id.into(),
            line,
        })?;
        let note = self.write(&note)?;
        self.item_at(&note, line)
    }

    /// Resolve a gap by recording where the answer is: appends
    /// `answered:: [[target]]` to its line.
    pub fn answer_gap(&mut self, note_id: &str, line: usize, target: &str) -> Result<Item> {
        let mut note = self.read(note_id)?;
        let current = self.item_at(&note, line)?;
        if current.kind != ItemKind::Gap {
            return Err(Error::NotAnItem {
                note: note_id.into(),
                line,
            });
        }
        let value = format!("[[{}]]", tasks::link_target(target));
        note.body = tasks::append_field(&note.body, line, "answered", &value).ok_or_else(|| Error::NotAnItem {
            note: note_id.into(),
            line,
        })?;
        let note = self.write(&note)?;
        self.item_at(&note, line)
    }

    /// Set (or, with an empty value, remove) an inline field such as `due`
    /// or `priority` on one item line.
    pub fn set_item_field(&mut self, note_id: &str, line: usize, key: &str, value: &str) -> Result<Item> {
        let mut note = self.read(note_id)?;
        self.item_at(&note, line)?;
        note.body = tasks::set_field(&note.body, line, key, value).ok_or_else(|| Error::NotAnItem {
            note: note_id.into(),
            line,
        })?;
        let note = self.write(&note)?;
        self.item_at(&note, line)
    }

    /// Add a task or gap to a note (under its `## Tasks` / `## Open questions`
    /// heading when it has one, else at the end).
    pub fn add_item(&mut self, note_id: &str, kind: ItemKind, text: &str) -> Result<Item> {
        let text = text.trim();
        if text.is_empty() {
            return Err(Error::EmptyCapture);
        }
        let mut note = self.read(note_id)?;
        let (body, line) = tasks::append_item(&note.body, kind, text);
        note.body = body;
        let note = self.write(&note)?;
        self.item_at(&note, line)
    }

    fn item_at(&self, note: &Note, line: usize) -> Result<Item> {
        tasks::parse_items(&note.id, &note.body)
            .into_iter()
            .find(|i| i.line == line)
            .ok_or_else(|| Error::NotAnItem {
                note: note.id.clone(),
                line,
            })
    }

    /// Vault-relative path of a note by id, if indexed.
    pub fn path_of(&self, id: &str) -> Result<Option<String>> {
        self.index.path_of(id)
    }

    pub(crate) fn index_remove_path(&self, rel: &str) -> Result<Option<String>> {
        self.index.remove_by_path(rel)
    }

    /// Relative path (with `/` separators) of an absolute path inside the vault.
    pub fn rel(&self, abs: &Path) -> Result<String> {
        let rel = abs
            .strip_prefix(&self.root)
            .map_err(|_| Error::PathEscapesVault(abs.to_path_buf()))?;
        Ok(to_rel_string(rel))
    }

    /// Join a relative path onto the root, refusing `..` and absolute paths.
    pub fn safe_join(&self, rel: &str) -> Result<PathBuf> {
        let p = Path::new(rel);
        if p.is_absolute()
            || p.components()
                .any(|c| matches!(c, Component::ParentDir | Component::Prefix(_) | Component::RootDir))
        {
            return Err(Error::PathEscapesVault(p.to_path_buf()));
        }
        Ok(self.root.join(p))
    }
}

fn is_hidden(rel: &str) -> bool {
    rel.split('/').any(|seg| seg.starts_with('.'))
}

/// Whether a file name is a sync client's duplicate of a file two machines
/// changed at once (Dropbox, Syncthing, OneDrive, Google Drive, Nextcloud
/// each have their own spelling). Such files are not indexed as notes;
/// sync folds them back into the original.
pub fn is_conflict_copy(rel: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    let lower = name.to_lowercase();
    lower.contains("conflicted copy") || lower.contains(".sync-conflict-") || lower.contains("(case conflict")
}

pub(crate) fn stat(meta: &fs::Metadata) -> (i64, i64) {
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    (mtime, meta.len() as i64)
}

pub(crate) fn to_rel_string(p: &Path) -> String {
    p.components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Title → file stem: strip characters that are illegal or awkward in
/// file names on any supported OS, collapse whitespace.
pub fn safe_file_stem(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '#' | '^' | '[' | ']' => ' ',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let stem = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let stem = stem.trim_matches('.').to_string();
    if stem.is_empty() {
        "Untitled".to_string()
    } else {
        stem
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault() -> (tempfile::TempDir, Vault) {
        let dir = tempfile::tempdir().unwrap();
        let v = Vault::open(dir.path()).unwrap();
        (dir, v)
    }

    #[test]
    fn open_creates_layout_and_indexes_existing_notes() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("Projects")).unwrap();
        fs::write(dir.path().join("Projects/Robot.md"), "# Robot\n\nLinks to [[Motor]].\n").unwrap();
        fs::write(dir.path().join("Motor.md"), "A motor.\n").unwrap();
        let v = Vault::open(dir.path()).unwrap();
        assert!(dir
            .path()
            .join(".graphite/devices")
            .join(&v.device().id)
            .join("index.sqlite")
            .exists());
        assert!(dir.path().join("notes").is_dir());
        assert_eq!(v.info().unwrap().note_count, 2);
        // ids were minted into the files
        let text = fs::read_to_string(dir.path().join("Motor.md")).unwrap();
        assert!(text.starts_with("---\nid: "));
        let motor = v.resolve("Motor").unwrap().unwrap();
        let bl = v.backlinks(&motor).unwrap();
        assert_eq!(bl.len(), 1);
        assert_eq!(bl[0].from.title, "Robot");
    }

    #[test]
    fn create_write_read_delete() {
        let (dir, mut v) = vault();
        let n = v.create("Solar still: v2?", Some("idea"), None).unwrap();
        assert_eq!(n.path, "notes/Solar still v2.md");
        assert!(dir.path().join(&n.path).exists());
        let mut n2 = n.clone();
        n2.body = "Body with [[Other]]".into();
        n2.tags = vec!["water".into()];
        let written = v.write(&n2).unwrap();
        assert!(written.updated >= n.updated);
        let read = v.read(&n.id).unwrap();
        assert_eq!(read.body, "Body with [[Other]]\n");
        assert_eq!(read.tags, vec!["water"]);
        assert_eq!(read.note_type, "idea");
        assert_eq!(v.outgoing(&n.id).unwrap(), vec![("Other".to_string(), None)]);
        let other = v.resolve_or_create("Other").unwrap();
        assert_eq!(v.outgoing(&n.id).unwrap()[0].1.as_deref(), Some(other.as_str()));
        assert!(matches!(
            v.create("Solar still: v2?", None, None),
            Err(Error::NoteExists(_))
        ));
        v.delete(&n.id).unwrap();
        assert!(matches!(v.read(&n.id), Err(Error::NoteNotFound(_))));
        assert_eq!(v.list().unwrap().len(), 1);
    }

    #[test]
    fn sync_picks_up_external_edits_and_deletions() {
        let (dir, mut v) = vault();
        let n = v.create("Ext", None, None).unwrap();
        // Ensure a different mtime/size.
        fs::write(
            dir.path().join(&n.path),
            format!("{}\nmore text about turnips\n", n.to_text()),
        )
        .unwrap();
        assert_eq!(v.sync().unwrap(), 1);
        assert_eq!(v.search("turnips", 5).unwrap().len(), 1);
        fs::remove_file(dir.path().join(&n.path)).unwrap();
        assert_eq!(v.sync().unwrap(), 1);
        assert_eq!(v.list().unwrap().len(), 0);
    }

    #[test]
    fn items_are_indexed_attributed_and_editable() {
        let dir = tempfile::tempdir().unwrap();
        let mut vault = Vault::open(dir.path()).unwrap();
        let mut project = vault.create("Garden Robot", Some("project"), None).unwrap();
        project.body =
            "## Tasks\n\n- [ ] Order motors due:: 2026-11-01 #p1\n  - [?] Which motor?\n- [x] Sketch layout\n".into();
        vault.write(&project).unwrap();
        let mut daily = vault.create("2026-10-15", Some("daily"), None).unwrap();
        daily.body = "- [ ] Call the supplier project:: [[Garden Robot]]\n- [ ] Unrelated chore\n".into();
        vault.write(&daily).unwrap();
        let mut log = vault.create("Build log", None, None).unwrap();
        log.properties
            .insert("project".into(), serde_json::Value::String("[[Garden Robot]]".into()));
        log.body = "- [/] Print brackets\n".into();
        vault.write(&log).unwrap();

        let all = vault.items(&ItemFilter::default()).unwrap();
        assert_eq!(all.len(), 6);
        let mine = vault
            .items(&ItemFilter {
                project: Some(project.id.clone()),
                ..Default::default()
            })
            .unwrap();
        let texts: Vec<&str> = mine.iter().map(|r| r.item.text.as_str()).collect();
        // Ordered by note path, then line.
        assert_eq!(
            texts,
            vec![
                "Call the supplier",
                "Print brackets",
                "Order motors",
                "Which motor?",
                "Sketch layout"
            ]
        );
        assert!(mine.iter().all(|r| r.project_title.as_deref() == Some("Garden Robot")));
        let open_tasks = vault
            .items(&ItemFilter {
                project: Some(project.id.clone()),
                kind: Some(ItemKind::Task),
                open_only: true,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(open_tasks.len(), 3);

        let p = vault.progress(&project.id).unwrap();
        assert_eq!((p.tasks, p.done, p.doing, p.open, p.gaps_open), (4, 1, 1, 2, 1));
        assert_eq!(p.percent, 25);

        let done = vault.set_item_status(&project.id, 2, ItemStatus::Done).unwrap();
        assert_eq!(done.status, ItemStatus::Done);
        assert!(vault
            .read(&project.id)
            .unwrap()
            .body
            .contains("- [x] Order motors due:: 2026-11-01 #p1"));
        assert!(
            vault.set_item_status(&project.id, 3, ItemStatus::Done).is_err(),
            "gaps are answered, not ticked"
        );
        let gap = vault.answer_gap(&project.id, 3, "Motor shortlist").unwrap();
        assert_eq!(
            (gap.status, gap.answer.as_deref()),
            (ItemStatus::Answered, Some("Motor shortlist"))
        );

        let added = vault.add_item(&project.id, ItemKind::Task, "Wire it up").unwrap();
        assert_eq!(added.heading.as_deref(), Some("Tasks"));
        let body = vault.read(&project.id).unwrap().body;
        assert!(body.contains("- [x] Sketch layout\n- [ ] Wire it up\n"), "{body}");
        assert_eq!(vault.progress(&project.id).unwrap().tasks, 5);
    }

    #[test]
    fn refuses_paths_outside_vault() {
        let (_dir, v) = vault();
        assert!(matches!(v.safe_join("../x.md"), Err(Error::PathEscapesVault(_))));
        assert!(matches!(v.safe_join("/etc/passwd"), Err(Error::PathEscapesVault(_))));
    }

    #[test]
    fn file_stems() {
        assert_eq!(safe_file_stem("  a/b:c  "), "a b c");
        assert_eq!(safe_file_stem("..."), "Untitled");
        assert_eq!(crate::note::title_from_path("notes/Hi there.md"), "Hi there");
    }
}
