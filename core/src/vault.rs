//! A vault is a folder. This module reads and writes notes in it and keeps
//! the index under `.graphite/` in step with the files.

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

use chrono::Utc;
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::index::{Backlink, Index, SearchHit};
use crate::note::{new_id, Note, NoteSummary};

pub const STATE_DIR: &str = ".graphite";
pub const INDEX_FILE: &str = "index.sqlite";
pub const DEFAULT_NOTES_DIR: &str = "notes";
pub const ATTACHMENTS_DIR: &str = "attachments";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultInfo {
    pub root: String,
    pub name: String,
    pub note_count: usize,
}

pub struct Vault {
    root: PathBuf,
    index: Index,
}

impl Vault {
    /// Open (or initialise) a vault at `root`. Creates `.graphite/`,
    /// `notes/` and `attachments/` if missing, then brings the index up to
    /// date with the files on disk.
    pub fn open(root: impl AsRef<Path>) -> Result<Vault> {
        let root = root
            .as_ref()
            .canonicalize()
            .map_err(|_| Error::NotAVault(root.as_ref().to_path_buf()))?;
        if !root.is_dir() {
            return Err(Error::NotAVault(root));
        }
        fs::create_dir_all(root.join(STATE_DIR))?;
        fs::create_dir_all(root.join(DEFAULT_NOTES_DIR))?;
        fs::create_dir_all(root.join(ATTACHMENTS_DIR))?;
        let index = Index::open(&root.join(STATE_DIR).join(INDEX_FILE))?;
        let mut vault = Vault { root, index };
        vault.sync()?;
        Ok(vault)
    }

    pub fn root(&self) -> &Path {
        &self.root
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
        })
    }

    /// Walk the vault and (re)index every note whose mtime/size changed;
    /// drop index rows for files that disappeared. Notes without an `id`
    /// get one written into their front-matter.
    pub fn sync(&mut self) -> Result<usize> {
        let mut seen = Vec::new();
        let mut changed = 0usize;
        for entry in WalkBuilder::new(&self.root)
            .hidden(true)
            .git_ignore(false)
            .filter_entry(|e| e.file_name() != STATE_DIR)
            .build()
            .flatten()
        {
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let rel = self.rel(path)?;
            seen.push(rel.clone());
            let meta = fs::metadata(path)?;
            let (mtime, size) = stat(&meta);
            if self.index.stat(&rel)? == Some((mtime, size)) {
                continue;
            }
            if self.index_file(&rel).is_ok() {
                changed += 1;
            } else {
                tracing::warn!("skipping unreadable note {rel}");
            }
        }
        for stale in self.index.all_paths()? {
            if !seen.contains(&stale) {
                self.index.remove_by_path(&stale)?;
                changed += 1;
            }
        }
        Ok(changed)
    }

    /// Full rebuild from disk.
    pub fn rebuild(&mut self) -> Result<usize> {
        self.index.clear()?;
        self.sync()
    }

    /// Re-read one file (after an external change) and update the index.
    /// Returns the note's id, or `None` if the file is gone.
    pub fn reindex_path(&mut self, abs_or_rel: &Path) -> Result<Option<String>> {
        let rel = if abs_or_rel.is_absolute() {
            self.rel(abs_or_rel)?
        } else {
            to_rel_string(abs_or_rel)
        };
        if !self.root.join(&rel).is_file() {
            self.index.remove_by_path(&rel)?;
            return Ok(None);
        }
        self.index_file(&rel).map(Some)
    }

    pub(crate) fn index_file(&mut self, rel: &str) -> Result<String> {
        let abs = self.root.join(rel);
        let text = fs::read_to_string(&abs)?;
        let mut note = Note::parse(rel, &text)?;
        if note.id.is_empty() {
            note.id = new_id();
            // Minting an id is the one write Graphite does to a file it did
            // not author: the id is what keeps links stable across renames.
            fs::write(&abs, note.to_text())?;
        }
        let meta = fs::metadata(&abs)?;
        let (mtime, size) = stat(&meta);
        self.index.upsert(&note, mtime, size)?;
        Ok(note.id)
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

fn stat(meta: &fs::Metadata) -> (i64, i64) {
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
        assert!(dir.path().join(".graphite/index.sqlite").exists());
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
