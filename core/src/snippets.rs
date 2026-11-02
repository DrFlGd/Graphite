//! Snippets (C13): reusable bits of Markdown kept in the vault as notes
//! with `type: snippet`, by default under `snippets/`. The editor lists them
//! in its `/` menu and inserts the body with placeholders filled in:
//! `{{date}}` (today, `YYYY-MM-DD`), `{{time}}` (`HH:MM`), `{{datetime}}`
//! (RFC 3339) and `{{title}}` (the title of the note it goes into).

use chrono::{Local, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::note::Note;
use crate::vault::Vault;

pub const SNIPPETS_DIR: &str = "snippets";
pub const SNIPPET_TYPE: &str = "snippet";

/// A snippet as pickers list it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Snippet {
    /// The id of the snippet's note.
    pub id: String,
    /// The note's title.
    pub name: String,
    /// Front-matter `description`, or the start of the body.
    pub description: String,
    pub path: String,
}

/// Fill a snippet's placeholders in.
pub fn render_snippet(body: &str, title: &str) -> String {
    let now = Local::now();
    body.replace("{{title}}", title.trim())
        .replace("{{date}}", &now.format("%Y-%m-%d").to_string())
        .replace("{{time}}", &now.format("%H:%M").to_string())
        .replace("{{datetime}}", &Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true))
        .trim_matches('\n')
        .to_string()
}

fn is_snippet(note_type: &str, path: &str) -> bool {
    note_type == SNIPPET_TYPE || path.starts_with(&format!("{SNIPPETS_DIR}/"))
}

/// One line describing a snippet: its `description`, else its first line of text.
fn describe(note: &Note) -> String {
    if let Some(d) = note.properties.get("description").and_then(|v| v.as_str()) {
        return d.trim().to_string();
    }
    let line = note.body.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    let mut out: String = line.chars().take(80).collect();
    if line.chars().count() > 80 {
        out.push('…');
    }
    out
}

impl Vault {
    /// Every snippet in the vault, by name.
    pub fn snippets(&self) -> Result<Vec<Snippet>> {
        let mut out = Vec::new();
        for s in self.list()? {
            if !is_snippet(&s.note_type, &s.path) {
                continue;
            }
            let Ok(note) = self.read(&s.id) else { continue };
            out.push(Snippet {
                description: describe(&note),
                id: s.id,
                name: s.title,
                path: s.path,
            });
        }
        out.sort_by_key(|s| s.name.to_lowercase());
        Ok(out)
    }

    /// The snippet's Markdown, ready to insert into the note titled `title`.
    pub fn render_snippet(&self, id: &str, title: &str) -> Result<String> {
        Ok(render_snippet(&self.read(id)?.body, title))
    }

    /// Keep `body` as a new snippet called `name` (under `snippets/`).
    pub fn save_snippet(&mut self, name: &str, body: &str) -> Result<Note> {
        let mut note = self.create(name, Some(SNIPPET_TYPE), Some(SNIPPETS_DIR))?;
        note.body = format!("{}\n", body.trim_matches('\n'));
        self.write(&note)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn snippets_are_listed_rendered_and_saved() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("snippets")).unwrap();
        fs::create_dir_all(dir.path().join("notes")).unwrap();
        fs::write(
            dir.path().join("snippets/Meeting header.md"),
            "---\ndescription: Date and attendees\n---\n**{{date}}**, notes for {{title}}\n\nAttendees:\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("notes/Sign-off.md"),
            "---\ntype: snippet\n---\nThanks,\nJerred\n",
        )
        .unwrap();
        fs::write(dir.path().join("notes/Plain.md"), "Not a snippet").unwrap();
        let mut vault = Vault::open(dir.path()).unwrap();

        let list = vault.snippets().unwrap();
        let names: Vec<_> = list.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["Meeting header", "Sign-off"]);
        assert_eq!(list[0].description, "Date and attendees");
        assert_eq!(list[1].description, "Thanks,");

        let text = vault.render_snippet(&list[0].id, "Weekly sync").unwrap();
        let today = Local::now().format("%Y-%m-%d").to_string();
        assert_eq!(text, format!("**{today}**, notes for Weekly sync\n\nAttendees:"));

        let saved = vault.save_snippet("Bug report", "\nSteps:\n1. \n").unwrap();
        assert_eq!(saved.path, "snippets/Bug report.md");
        assert_eq!(saved.note_type, "snippet");
        assert_eq!(vault.snippets().unwrap().len(), 3);
    }
}
