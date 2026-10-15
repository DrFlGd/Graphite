//! Per-note history (M4, J2): every time a note's content changes, the
//! whole file is copied to `.graphite/history/<note-id>/<ms>-<device>.md`.
//! Plain Markdown, one file per version, so a version can be read, diffed
//! or restored by anything, and the folder can be deleted without losing a
//! note. Edits made within ten minutes of each other on one device fold
//! into one version, so autosave does not produce a version per keystroke.

use std::fs;
use std::path::PathBuf;

use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::note::Note;
use crate::sync::content_hash;
use crate::vault::{Vault, STATE_DIR};

pub const HISTORY_DIR: &str = "history";
/// Edits closer together than this, on one device, are one version.
const COALESCE_MS: i64 = 10 * 60 * 1000;
/// Versions kept per note; the oldest go first.
const MAX_VERSIONS: usize = 200;

/// One saved version of a note.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Version {
    /// File stem under the note's history folder: `<ms>-<device>`.
    pub id: String,
    pub note_id: String,
    pub time: DateTime<Utc>,
    /// First eight characters of the device id that saved it.
    pub device: String,
    pub size: u64,
    pub title: String,
}

fn parse_version_name(name: &str) -> Option<(i64, String)> {
    let stem = name.strip_suffix(".md")?;
    let (ms, device) = stem.split_once('-')?;
    let ms: i64 = ms.parse().ok()?;
    if device.is_empty() || !device.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some((ms, device.to_string()))
}

impl Vault {
    fn history_dir(&self, note_id: &str) -> PathBuf {
        self.root().join(STATE_DIR).join(HISTORY_DIR).join(note_id)
    }

    fn device_short(&self) -> String {
        self.device.id.chars().take(8).collect()
    }

    /// Save a version of `note` (whose file text is `text`) if its content
    /// differs from the last version this device knows about. Called from
    /// indexing, so every change, local or external, is kept.
    pub(crate) fn record_version(&mut self, note: &Note, text: &str) -> Result<()> {
        if note.id.is_empty() {
            return Ok(());
        }
        let hash = content_hash(note);
        let latest = match self.index.history_latest(&note.id)? {
            Some(row) => Some(row),
            None => {
                // Not in the index (fresh index): look at the folder.
                let versions = self.versions(&note.id)?;
                match versions.first() {
                    Some(v) => {
                        let old = self.version_note(&note.id, &v.id)?;
                        let h = content_hash(&old);
                        self.index.set_history_latest(&note.id, &h, &v.id)?;
                        Some((h, v.id.clone()))
                    }
                    None => None,
                }
            }
        };
        if latest.as_ref().is_some_and(|(h, _)| *h == hash) {
            return Ok(());
        }
        let dir = self.history_dir(&note.id);
        fs::create_dir_all(&dir)?;
        let now = Utc::now().timestamp_millis();
        let device = self.device_short();
        // Fold into the previous version if it is ours and recent.
        if let Some((_, prev_id)) = &latest {
            if let Some((ms, dev)) = parse_version_name(&format!("{prev_id}.md")) {
                if dev == device && now - ms < COALESCE_MS {
                    let _ = fs::remove_file(dir.join(format!("{prev_id}.md")));
                }
            }
        }
        let id = format!("{now}-{device}");
        fs::write(dir.join(format!("{id}.md")), text)?;
        self.index.set_history_latest(&note.id, &hash, &id)?;
        self.prune_versions(&note.id)?;
        Ok(())
    }

    fn prune_versions(&self, note_id: &str) -> Result<()> {
        let versions = self.versions(note_id)?;
        for v in versions.iter().skip(MAX_VERSIONS) {
            let _ = fs::remove_file(self.history_dir(note_id).join(format!("{}.md", v.id)));
        }
        Ok(())
    }

    /// Saved versions of a note, newest first (from every device).
    pub fn versions(&self, note_id: &str) -> Result<Vec<Version>> {
        let dir = self.history_dir(note_id);
        let mut out = Vec::new();
        let Ok(entries) = fs::read_dir(&dir) else {
            return Ok(out);
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some((ms, device)) = parse_version_name(&name) else {
                continue;
            };
            let meta = entry.metadata()?;
            let text = fs::read_to_string(entry.path()).unwrap_or_default();
            let title = Note::parse("x.md", &text)
                .map(|n| if n.title == "x" { String::new() } else { n.title })
                .unwrap_or_default();
            out.push(Version {
                id: name.trim_end_matches(".md").to_string(),
                note_id: note_id.to_string(),
                time: Utc.timestamp_millis_opt(ms).single().unwrap_or_else(Utc::now),
                device,
                size: meta.len(),
                title,
            });
        }
        out.sort_by(|a, b| b.time.cmp(&a.time).then_with(|| b.id.cmp(&a.id)));
        Ok(out)
    }

    /// A version's file text.
    pub fn version_text(&self, note_id: &str, version_id: &str) -> Result<String> {
        if version_id.contains('/') || version_id.contains('\\') || version_id.contains("..") {
            return Err(Error::NoteNotFound(version_id.into()));
        }
        let path = self.history_dir(note_id).join(format!("{version_id}.md"));
        fs::read_to_string(&path).map_err(|_| Error::NoteNotFound(format!("{note_id}@{version_id}")))
    }

    /// A version parsed as the note it was, at the note's current path.
    pub fn version_note(&self, note_id: &str, version_id: &str) -> Result<Note> {
        let text = self.version_text(note_id, version_id)?;
        let rel = self.index.path_of(note_id)?.unwrap_or_else(|| format!("{note_id}.md"));
        let mut note = Note::parse(&rel, &text)?;
        note.id = note_id.to_string();
        Ok(note)
    }

    /// Make a version the note's current content. The current content
    /// becomes a version of its own first, so a restore can be undone.
    pub fn restore_version(&mut self, note_id: &str, version_id: &str) -> Result<Note> {
        let old = self.version_note(note_id, version_id)?;
        let mut current = self.read(note_id)?;
        current.title = old.title;
        current.note_type = old.note_type;
        current.tags = old.tags;
        current.aliases = old.aliases;
        current.properties = old.properties;
        current.body = old.body;
        if let Some(c) = old.created {
            current.created = Some(c);
        }
        self.write(&current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_make_versions_and_restore_brings_one_back() {
        let dir = tempfile::tempdir().unwrap();
        let mut v = Vault::open(dir.path()).unwrap();
        let note = v.create("Plan", None, None).unwrap();
        assert_eq!(v.versions(&note.id).unwrap().len(), 1, "creation is the first version");
        let mut n = note.clone();
        n.body = "first draft\n".into();
        v.write(&n).unwrap();
        // Within the coalescing window, the second edit folds into the version.
        let versions = v.versions(&note.id).unwrap();
        assert_eq!(versions.len(), 1);
        assert_eq!(v.version_note(&note.id, &versions[0].id).unwrap().body, "first draft\n");

        // An older version on another device sits in the folder too.
        let hist = dir.path().join(".graphite/history").join(&note.id);
        fs::write(
            hist.join("1000-deadbeef.md"),
            "---\nid: x\ntitle: Old plan\n---\nvery old\n",
        )
        .unwrap();
        let versions = v.versions(&note.id).unwrap();
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[1].device, "deadbeef");
        assert_eq!(versions[1].title, "Old plan");

        let restored = v.restore_version(&note.id, "1000-deadbeef").unwrap();
        assert_eq!(restored.body, "very old\n");
        assert_eq!(restored.title, "Old plan");
        assert_eq!(restored.id, note.id);
        assert_eq!(v.read(&note.id).unwrap().body, "very old\n");
        // Unchanged content does not add a version.
        let before = v.versions(&note.id).unwrap().len();
        v.sync().unwrap();
        assert_eq!(v.versions(&note.id).unwrap().len(), before);
    }

    #[test]
    fn version_ids_are_validated() {
        let dir = tempfile::tempdir().unwrap();
        let v = Vault::open(dir.path()).unwrap();
        assert!(v.version_text("01X", "../../etc/passwd").is_err());
        assert!(v.version_text("01X", "nope").is_err());
        assert_eq!(
            parse_version_name("1700000000000-abcd1234.md"),
            Some((1700000000000, "abcd1234".into()))
        );
        assert_eq!(parse_version_name("notes.md"), None);
    }
}
