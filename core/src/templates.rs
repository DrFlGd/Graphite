//! Templates for notes and whole projects (A7), and idea → project
//! promotion (D10).
//!
//! A template is a Markdown body with front-matter defaults. Built-in ones
//! cover the common project shapes; a vault adds its own by putting notes
//! with `type: template` under `templates/`. Placeholders: `{{title}}`,
//! `{{date}}` (today, `YYYY-MM-DD`), `{{datetime}}` (RFC 3339) and
//! `{{body}}` (the existing text when a note is promoted; empty otherwise).

use chrono::{Local, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

use crate::note::Note;

pub const TEMPLATES_DIR: &str = "templates";
pub const TEMPLATE_TYPE: &str = "template";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Template {
    /// Short name used to pick it (`research`, `decision`, …).
    pub name: String,
    /// One line shown in pickers.
    pub description: String,
    /// `type` of the note it creates.
    pub note_type: String,
    /// Folder new notes go in (relative to the vault).
    pub folder: String,
    /// Front-matter properties the new note starts with.
    pub properties: serde_json::Map<String, serde_json::Value>,
    /// Body with placeholders.
    pub body: String,
    /// `true` for the ones shipped with Graphite; a vault template of the
    /// same name replaces it.
    pub builtin: bool,
}

impl Template {
    /// Fill the placeholders in.
    pub fn render(&self, title: &str, body: &str) -> String {
        let now = Utc::now();
        let out = self
            .body
            .replace("{{title}}", title.trim())
            .replace("{{date}}", &Local::now().format("%Y-%m-%d").to_string())
            .replace("{{datetime}}", &now.to_rfc3339_opts(SecondsFormat::Secs, true));
        if out.contains("{{body}}") {
            out.replace("{{body}}", body.trim())
        } else if body.trim().is_empty() {
            out
        } else {
            let mut s = out.trim_end().to_string();
            s.push_str("\n\n");
            s.push_str(body.trim());
            s.push('\n');
            s
        }
    }

    /// Build a template from a vault note (`type: template`). The file stem
    /// is the name; `description`, `note_type` and `folder` are properties;
    /// every other property is a default for the created note.
    pub fn from_note(note: &Note) -> Template {
        let name = crate::note::title_from_path(&note.path)
            .to_lowercase()
            .replace(' ', "-");
        let text = |k: &str| note.properties.get(k).and_then(|v| v.as_str()).map(str::to_string);
        let note_type = text("note_type")
            .or_else(|| text("for"))
            .unwrap_or_else(|| "note".into());
        let folder = text("folder").unwrap_or_else(|| default_folder(&note_type).to_string());
        let mut properties = serde_json::Map::new();
        for (k, v) in &note.properties {
            if !matches!(k.as_str(), "description" | "note_type" | "for" | "folder") {
                properties.insert(k.clone(), v.clone());
            }
        }
        Template {
            name,
            description: text("description").unwrap_or_default(),
            note_type,
            folder,
            properties,
            body: note.body.clone(),
            builtin: false,
        }
    }
}

/// Where notes of a type live unless the template says otherwise.
pub fn default_folder(note_type: &str) -> &'static str {
    match note_type {
        "project" => "notes/Projects",
        "decision" => "notes/Decisions",
        "idea" => "notes/Ideas",
        "query" => "notes/Queries",
        _ => crate::vault::DEFAULT_NOTES_DIR,
    }
}

fn status(v: &str) -> serde_json::Map<String, serde_json::Value> {
    let mut m = serde_json::Map::new();
    m.insert("status".into(), serde_json::Value::String(v.into()));
    m
}

/// The templates shipped with Graphite.
pub fn builtin_templates() -> Vec<Template> {
    let project = |name: &str, description: &str, body: &str| Template {
        name: name.into(),
        description: description.into(),
        note_type: "project".into(),
        folder: default_folder("project").into(),
        properties: status("active"),
        body: body.into(),
        builtin: true,
    };
    vec![
        project(
            "project",
            "A plain project: goal, tasks, open questions, notes.",
            "# {{title}}\n\n{{body}}\n\n## Goal\n\n\n## Tasks\n\n\n## Open questions\n\n\n## Notes\n\n",
        ),
        project(
            "research",
            "Investigate a question: sources, findings, open questions.",
            "# {{title}}\n\n{{body}}\n\n## Question\n\n\n## Sources\n\n\n## Findings\n\n\n## Open questions\n\n\n## Tasks\n\n- [ ] Collect sources\n- [ ] Write up findings\n",
        ),
        project(
            "build",
            "Make a physical thing: parts, steps, measurements.",
            "# {{title}}\n\n{{body}}\n\n## Design\n\n\n## Parts\n\n| Part | Source | Status |\n| --- | --- | --- |\n|  |  |  |\n\n## Tasks\n\n- [ ] Finish the design\n- [ ] Order parts\n- [ ] Assemble\n- [ ] Test\n\n## Open questions\n\n\n## Build log\n\n",
        ),
        project(
            "software",
            "A software project: scope, architecture, backlog, decisions.",
            "# {{title}}\n\n{{body}}\n\n## Scope\n\n\n## Architecture\n\n\n## Tasks\n\n- [ ] Set up the repository\n- [ ] First working slice\n\n## Open questions\n\n\n## Decisions\n\n",
        ),
        project(
            "writing",
            "A piece of writing: outline, drafts, sources.",
            "# {{title}}\n\n{{body}}\n\n## Audience and point\n\n\n## Outline\n\n1. \n\n## Tasks\n\n- [ ] Outline\n- [ ] First draft\n- [ ] Revise\n\n## Open questions\n\n\n## Sources\n\n",
        ),
        Template {
            name: "decision".into(),
            description: "Record a decision: context, options, outcome.".into(),
            note_type: "decision".into(),
            folder: default_folder("decision").into(),
            properties: {
                let mut m = status("proposed");
                m.insert("date".into(), serde_json::Value::String("{{date}}".into()));
                m.insert("project".into(), serde_json::Value::String(String::new()));
                m
            },
            body: "# {{title}}\n\n{{body}}\n\n## Context\n\n\n## Options\n\n1. \n2. \n\n## Outcome\n\n\n## Consequences\n\n".into(),
            builtin: true,
        },
        Template {
            name: "query".into(),
            description: "A saved query shown as a list, board or task tree.".into(),
            note_type: "query".into(),
            folder: default_folder("query").into(),
            properties: {
                let mut m = serde_json::Map::new();
                m.insert("query".into(), serde_json::Value::String("type:task status:open".into()));
                m.insert("view".into(), serde_json::Value::String("list".into()));
                m
            },
            body: "{{body}}".into(),
            builtin: true,
        },
    ]
}

/// Fill `{{date}}` and friends in property values too.
pub fn render_properties(
    props: &serde_json::Map<String, serde_json::Value>,
    title: &str,
) -> serde_json::Map<String, serde_json::Value> {
    let today = Local::now().format("%Y-%m-%d").to_string();
    props
        .iter()
        .map(|(k, v)| {
            let v = match v {
                serde_json::Value::String(s) => {
                    serde_json::Value::String(s.replace("{{date}}", &today).replace("{{title}}", title))
                }
                other => other.clone(),
            };
            (k.clone(), v)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_placeholders_and_keeps_existing_text() {
        let t = &builtin_templates()[0];
        let out = t.render("Solar still", "An idea.");
        assert!(out.starts_with("# Solar still\n\nAn idea.\n"));
        assert!(out.contains("## Tasks"));
        let fresh = t.render("X", "");
        assert!(fresh.starts_with("# X\n\n\n\n## Goal"));
        let no_slot = Template {
            body: "# {{title}} on {{date}}".into(),
            ..t.clone()
        };
        let out = no_slot.render("Y", "kept");
        assert!(out.starts_with("# Y on 20"));
        assert!(out.ends_with("\n\nkept\n"));
    }

    #[test]
    fn vault_template_notes_become_templates() {
        let text = "---\ntype: template\ndescription: Weekly review\nfor: review\nmood: ok\n---\n# Week of {{date}}\n";
        let note = Note::parse("templates/Weekly review.md", text).unwrap();
        let t = Template::from_note(&note);
        assert_eq!(t.name, "weekly-review");
        assert_eq!(t.note_type, "review");
        assert_eq!(t.folder, "notes");
        assert_eq!(t.description, "Weekly review");
        assert_eq!(t.properties.get("mood").and_then(|v| v.as_str()), Some("ok"));
        assert!(!t.builtin);
    }

    #[test]
    fn builtin_names_are_unique() {
        let mut names: Vec<String> = builtin_templates().into_iter().map(|t| t.name).collect();
        let n = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), n);
    }
}
