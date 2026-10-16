//! Quick capture, daily notes and moving notes between folders.
//!
//! Capture never asks a question: whatever comes in lands as a `capture`
//! note in the inbox with a timestamped title, to be filed later.

use chrono::{Local, NaiveDate};

use crate::error::{Error, Result};
use crate::note::Note;
use crate::vault::{to_rel_string, Vault};

pub const INBOX_DIR: &str = "notes/Inbox";
pub const DAILY_DIR: &str = "notes/Daily";
pub const CAPTURE_TYPE: &str = "capture";
pub const DAILY_TYPE: &str = "daily";

/// A daily note's title always carries its date, so a renamed day stays
/// findable: `Garden day` for `notes/Daily/2026-10-15.md` becomes
/// `Garden day (2026-10-15)`. Titles that already hold the date, and notes
/// whose file name is not a date, are left alone.
pub fn dated_daily_title(title: &str, rel: &str) -> String {
    let stem = rel.rsplit('/').next().unwrap_or(rel).trim_end_matches(".md");
    let title = title.trim();
    if NaiveDate::parse_from_str(stem, "%Y-%m-%d").is_err() || title.contains(stem) {
        return title.to_string();
    }
    if title.is_empty() {
        stem.to_string()
    } else {
        format!("{title} ({stem})")
    }
}

const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "avif"];

impl Vault {
    /// Create an inbox note from free text plus already-imported attachment
    /// paths (vault-relative, see `import_attachment`). Images are embedded,
    /// other files linked. Returns the new note.
    pub fn capture(&mut self, text: &str, attachments: &[String]) -> Result<Note> {
        let text = text.trim();
        if text.is_empty() && attachments.is_empty() {
            return Err(Error::EmptyCapture);
        }
        let title = capture_title(text, attachments);
        let mut note = self.create(&title, Some(CAPTURE_TYPE), Some(INBOX_DIR))?;
        let mut body = String::new();
        if !text.is_empty() {
            body.push_str(text);
            body.push('\n');
        }
        if is_url(text) {
            note.properties
                .insert("source".into(), serde_json::Value::String(text.to_string()));
        }
        for rel in attachments {
            let name = rel.rsplit('/').next().unwrap_or(rel);
            let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
            if !body.is_empty() && !body.ends_with("\n\n") {
                body.push('\n');
            }
            if IMAGE_EXTS.contains(&ext.as_str()) {
                body.push_str(&format!("![{name}]({rel})\n"));
            } else {
                body.push_str(&format!("[{name}]({rel})\n"));
            }
        }
        note.body = body;
        self.write(&note)
    }

    /// Get or create the daily note for `date` (`notes/Daily/YYYY-MM-DD.md`).
    pub fn daily_note(&mut self, date: NaiveDate) -> Result<Note> {
        let title = date.format("%Y-%m-%d").to_string();
        let rel = format!("{DAILY_DIR}/{title}.md");
        if self.safe_join(&rel)?.is_file() {
            // Make sure it is indexed (it may have been created externally).
            let id = self.index_file(&rel)?;
            return self.read(&id);
        }
        self.create(&title, Some(DAILY_TYPE), Some(DAILY_DIR))
    }

    /// Move a note into another folder (vault-relative), keeping its file
    /// name and id. Links keep resolving because they resolve by title.
    pub fn move_note(&mut self, id: &str, folder: &str) -> Result<Note> {
        let rel = self.path_of(id)?.ok_or_else(|| Error::NoteNotFound(id.into()))?;
        // Validate the folder before `to_rel_string` quietly drops `..` components.
        self.safe_join(folder)?;
        let file = rel.rsplit('/').next().unwrap_or(&rel).to_string();
        let new_rel = to_rel_string(std::path::Path::new(folder.trim_matches('/')).join(&file).as_path());
        if new_rel == rel {
            return self.read(id);
        }
        let from = self.safe_join(&rel)?;
        let to = self.safe_join(&new_rel)?;
        if to.exists() {
            return Err(Error::NoteExists(to));
        }
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(&from, &to)?;
        self.index_remove_path(&rel)?;
        let id = self.index_file(&new_rel)?;
        self.read(&id)
    }

    /// Change a note's type without touching its body.
    pub fn set_type(&mut self, id: &str, note_type: &str) -> Result<Note> {
        let mut note = self.read(id)?;
        note.note_type = note_type.trim().to_string();
        if note.note_type.is_empty() {
            note.note_type = crate::note::DEFAULT_TYPE.into();
        }
        self.write(&note)
    }

    /// Folders that currently hold notes, plus the standard ones, sorted.
    pub fn folders(&self) -> Result<Vec<String>> {
        let mut out: Vec<String> = vec![
            crate::vault::DEFAULT_NOTES_DIR.into(),
            INBOX_DIR.into(),
            DAILY_DIR.into(),
        ];
        for n in self.list()? {
            if let Some((folder, _)) = n.path.rsplit_once('/') {
                out.push(folder.to_string());
            }
        }
        out.sort();
        out.dedup();
        Ok(out)
    }
}

/// `YYYY-MM-DD HHMM <first line…>` so captures never collide and sort by time.
pub fn capture_title(text: &str, attachments: &[String]) -> String {
    let stamp = Local::now().format("%Y-%m-%d %H%M").to_string();
    let first = text.lines().next().unwrap_or("").trim().trim_start_matches('#').trim();
    let first = if is_url(first) {
        url_label(first)
    } else {
        first.to_string()
    };
    let short: String = first.chars().take(48).collect::<String>().trim().to_string();
    if !short.is_empty() {
        format!("{stamp} {short}")
    } else if let Some(a) = attachments.first() {
        format!("{stamp} {}", a.rsplit('/').next().unwrap_or(a))
    } else {
        stamp
    }
}

pub fn is_url(s: &str) -> bool {
    let s = s.trim();
    (s.starts_with("http://") || s.starts_with("https://")) && !s.contains(char::is_whitespace)
}

/// `https://www.example.com/a/b?x` → `example.com/a/b`
fn url_label(url: &str) -> String {
    let rest = url.trim().split("://").nth(1).unwrap_or(url);
    let rest = rest.split(['?', '#']).next().unwrap_or(rest);
    let rest = rest.strip_prefix("www.").unwrap_or(rest);
    rest.trim_end_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_lands_in_inbox_with_stamped_title() {
        let dir = tempfile::tempdir().unwrap();
        let mut v = Vault::open(dir.path()).unwrap();
        let n = v.capture("# Solar still\nfor the garden", &[]).unwrap();
        assert!(n.path.starts_with("notes/Inbox/"));
        assert!(n.title.ends_with(" Solar still"), "{}", n.title);
        assert_eq!(n.note_type, "capture");
        assert_eq!(n.body, "# Solar still\nfor the garden\n");
        assert!(matches!(v.capture("   ", &[]), Err(Error::EmptyCapture)));
    }

    #[test]
    fn capture_of_url_and_attachments() {
        let dir = tempfile::tempdir().unwrap();
        let mut v = Vault::open(dir.path()).unwrap();
        let n = v
            .capture(
                "https://www.example.com/pumps?x=1",
                &["attachments/a.png".into(), "attachments/b.pdf".into()],
            )
            .unwrap();
        assert!(n.title.ends_with(" example.com/pumps"), "{}", n.title);
        assert_eq!(n.properties["source"], "https://www.example.com/pumps?x=1");
        assert!(n.body.contains("![a.png](attachments/a.png)"));
        assert!(n.body.contains("[b.pdf](attachments/b.pdf)"));
        let only_files = v.capture("", &["attachments/c.stl".into()]).unwrap();
        assert!(only_files.title.ends_with(" c.stl"));
    }

    #[test]
    fn daily_note_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let mut v = Vault::open(dir.path()).unwrap();
        let d = NaiveDate::from_ymd_opt(2026, 10, 14).unwrap();
        let a = v.daily_note(d).unwrap();
        let b = v.daily_note(d).unwrap();
        assert_eq!(a.id, b.id);
        assert_eq!(a.path, "notes/Daily/2026-10-14.md");
        assert_eq!(a.note_type, "daily");
    }

    #[test]
    fn renamed_daily_notes_keep_their_date() {
        let dir = tempfile::tempdir().unwrap();
        let mut v = Vault::open(dir.path()).unwrap();
        let mut day = v.daily_note(NaiveDate::from_ymd_opt(2026, 10, 14).unwrap()).unwrap();
        day.title = "Garden day".into();
        assert_eq!(v.write(&day).unwrap().title, "Garden day (2026-10-14)");
        assert_eq!(v.read(&day.id).unwrap().title, "Garden day (2026-10-14)");
        // Still today's note for the Today button.
        assert_eq!(
            v.daily_note(NaiveDate::from_ymd_opt(2026, 10, 14).unwrap()).unwrap().id,
            day.id
        );
        assert_eq!(dated_daily_title("", "notes/Daily/2026-10-14.md"), "2026-10-14");
        assert_eq!(
            dated_daily_title("Trip 2026-10-14", "notes/Daily/2026-10-14.md"),
            "Trip 2026-10-14"
        );
        assert_eq!(dated_daily_title("Log", "notes/Daily/log.md"), "Log");
        // Retyped to an ordinary note, the title is the user's again.
        day.note_type = "note".into();
        day.title = "Garden day".into();
        assert_eq!(v.write(&day).unwrap().title, "Garden day");
    }

    #[test]
    fn move_and_retype_keep_id_and_links() {
        let dir = tempfile::tempdir().unwrap();
        let mut v = Vault::open(dir.path()).unwrap();
        let cap = v.capture("Pump notes", &[]).unwrap();
        let other = v.create("Garden Robot", Some("project"), None).unwrap();
        let mut other = other;
        other.body = format!("see [[{}]]", cap.title);
        v.write(&other).unwrap();
        let moved = v.move_note(&cap.id, "notes/Projects/Garden Robot").unwrap();
        assert_eq!(moved.id, cap.id);
        assert_eq!(moved.path, format!("notes/Projects/Garden Robot/{}.md", cap.title));
        assert!(!dir.path().join(&cap.path).exists());
        assert_eq!(v.backlinks(&cap.id).unwrap().len(), 1);
        let retyped = v.set_type(&cap.id, "note").unwrap();
        assert_eq!(retyped.note_type, "note");
        assert_eq!(v.read(&cap.id).unwrap().body, "Pump notes\n");
        assert!(v
            .folders()
            .unwrap()
            .contains(&"notes/Projects/Garden Robot".to_string()));
        assert!(matches!(v.move_note(&cap.id, "../x"), Err(Error::PathEscapesVault(_))));
    }
}
