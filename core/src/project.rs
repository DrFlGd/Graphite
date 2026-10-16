//! Project hub data (D1), templates on the vault (A7, D10) and the query
//! entry points (G1). The desktop hub and the CLI are thin over these.

use std::fs;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::index::{ItemFilter, ItemRow};
use crate::note::{Note, NoteSummary};
use crate::query::{Query, QueryResult};
use crate::tasks::{ItemKind, Progress};
use crate::templates::{self, Template, TEMPLATES_DIR, TEMPLATE_TYPE};
use crate::vault::Vault;

/// A note linked to the project, either way round.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LinkedNote {
    #[serde(flatten)]
    pub note: NoteSummary,
    /// Typed link relation, when there is one.
    pub relation: Option<String>,
    /// `true` when the other note links here; `false` when this project
    /// links out to it.
    pub incoming: bool,
}

/// Everything the project page shows.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectHub {
    pub project: NoteSummary,
    pub progress: Progress,
    /// All tasks attributed to the project, in note order (nesting via
    /// `depth` / `parent`).
    pub tasks: Vec<ItemRow>,
    /// All gaps attributed to the project, open ones first.
    pub gaps: Vec<ItemRow>,
    pub decisions: Vec<NoteSummary>,
    pub sources: Vec<NoteSummary>,
    /// Other notes linked to or from the project (decisions and sources
    /// excluded).
    pub linked: Vec<LinkedNote>,
}

impl Vault {
    /// Run a query written in the query language.
    pub fn query(&self, text: &str) -> Result<QueryResult> {
        self.index.query(&Query::parse(text))
    }

    /// Run the query stored in a `type: query` note.
    pub fn run_saved_query(&self, id: &str) -> Result<QueryResult> {
        let text = self
            .index
            .saved_query(id)?
            .ok_or_else(|| Error::NoteNotFound(id.into()))?;
        self.query(&text)
    }

    /// Built-in templates plus the vault's own (`templates/*.md` with
    /// `type: template`), vault ones winning on name clashes.
    pub fn templates(&self) -> Result<Vec<Template>> {
        let mut out = templates::builtin_templates();
        let dir = self.root().join(TEMPLATES_DIR);
        if dir.is_dir() {
            let mut files: Vec<_> = fs::read_dir(&dir)?
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "md"))
                .collect();
            files.sort();
            for path in files {
                let rel = self.rel(&path)?;
                let note = match self.read_by_path(&rel) {
                    Ok(n) if n.note_type == TEMPLATE_TYPE => n,
                    _ => continue,
                };
                let t = Template::from_note(&note);
                out.retain(|b| b.name != t.name);
                out.push(t);
            }
        }
        Ok(out)
    }

    pub fn template(&self, name: &str) -> Result<Template> {
        self.templates()?
            .into_iter()
            .find(|t| t.name == name)
            .ok_or_else(|| Error::NoteNotFound(format!("template {name}")))
    }

    /// Create a note from a template. `folder` overrides the template's.
    pub fn create_from_template(&mut self, title: &str, template: &str, folder: Option<&str>) -> Result<Note> {
        let t = self.template(template)?;
        let folder = folder.unwrap_or(&t.folder);
        let mut note = self.create(title, Some(&t.note_type), Some(folder))?;
        note.properties = templates::render_properties(&t.properties, title);
        note.body = t.render(title, "");
        self.write(&note)
    }

    /// Turn a note (typically an idea) into a project in place: the type
    /// changes, the template's properties fill in where the note has none,
    /// and the existing text is kept inside the template body.
    pub fn promote_to_project(&mut self, id: &str, template: Option<&str>) -> Result<Note> {
        let t = self.template(template.unwrap_or("project"))?;
        let mut note = self.read(id)?;
        let old_body = strip_title_heading(&note.body, &note.title);
        note.note_type = t.note_type.clone();
        for (k, v) in templates::render_properties(&t.properties, &note.title) {
            note.properties.entry(k).or_insert(v);
        }
        note.body = t.render(&note.title, &old_body);
        self.write(&note)
    }

    /// Everything the project page shows, for a `project` note.
    pub fn project_hub(&self, id: &str) -> Result<ProjectHub> {
        let project = self.summary(id)?.ok_or_else(|| Error::NoteNotFound(id.into()))?;
        let items = self.items(&ItemFilter {
            project: Some(id.to_string()),
            ..Default::default()
        })?;
        let progress = Progress::of(items.iter().map(|r| &r.item));
        let (tasks, mut gaps): (Vec<_>, Vec<_>) = items.into_iter().partition(|r| r.item.kind == ItemKind::Task);
        gaps.sort_by_key(|g| !g.item.status.is_open());

        let mut linked: Vec<LinkedNote> = Vec::new();
        for b in self.backlinks(id)? {
            if b.from.id == id || linked.iter().any(|l| l.note.id == b.from.id) {
                continue;
            }
            linked.push(LinkedNote {
                note: b.from,
                relation: b.relation,
                incoming: true,
            });
        }
        for (_, target) in self.outgoing(id)? {
            let Some(tid) = target else { continue };
            if tid == id || linked.iter().any(|l| l.note.id == tid) {
                continue;
            }
            if let Some(note) = self.summary(&tid)? {
                linked.push(LinkedNote {
                    note,
                    relation: None,
                    incoming: false,
                });
            }
        }
        // Decisions may also point at the project through their `project`
        // property without a link in the body.
        let by_property = self.query(&format!("type:decision project:[[{}]]", project.title))?;
        for row in by_property.notes {
            if !linked.iter().any(|l| l.note.id == row.note.id) {
                linked.push(LinkedNote {
                    note: row.note,
                    relation: Some("project".into()),
                    incoming: true,
                });
            }
        }

        let mut decisions = Vec::new();
        let mut sources = Vec::new();
        let mut rest = Vec::new();
        for l in linked {
            match l.note.note_type.as_str() {
                "decision" => decisions.push(l.note),
                "source" => sources.push(l.note),
                _ => rest.push(l),
            }
        }
        decisions.sort_by_key(|d| std::cmp::Reverse(d.updated));
        sources.sort_by_key(|s| s.title.to_lowercase());
        rest.sort_by_key(|l| l.note.title.to_lowercase());

        Ok(ProjectHub {
            project,
            progress,
            tasks,
            gaps,
            decisions,
            sources,
            linked: rest,
        })
    }
}

/// Drop a leading `# Title` line so the template's own heading is not
/// doubled when a note is promoted.
fn strip_title_heading(body: &str, title: &str) -> String {
    let trimmed = body.trim_start();
    if let Some(rest) = trimmed.strip_prefix("# ") {
        if let Some((first, after)) = rest.split_once('\n') {
            if first.trim() == title.trim() {
                return after.trim_start_matches('\n').to_string();
            }
        } else if rest.trim() == title.trim() {
            return String::new();
        }
    }
    body.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::Domain;
    use crate::tasks::ItemStatus;

    #[test]
    fn templates_create_promote_and_hub() {
        let dir = tempfile::tempdir().unwrap();
        let mut vault = Vault::open(dir.path()).unwrap();

        // A vault template overrides a built-in of the same name.
        fs::create_dir_all(dir.path().join("templates")).unwrap();
        fs::write(
            dir.path().join("templates/writing.md"),
            "---\ntype: template\ndescription: My essays\nfor: project\ngenre: essay\n---\n# {{title}}\n\n{{body}}\n\n## Draft\n\n- [ ] Write it\n",
        )
        .unwrap();
        let names: Vec<String> = vault.templates().unwrap().into_iter().map(|t| t.name).collect();
        assert!(names.contains(&"research".to_string()));
        assert_eq!(names.iter().filter(|n| *n == "writing").count(), 1);
        let writing = vault.template("writing").unwrap();
        assert!(!writing.builtin);
        assert_eq!(writing.folder, "notes/Projects");

        // Create from a template.
        let essay = vault.create_from_template("Why vaults", "writing", None).unwrap();
        assert_eq!(essay.note_type, "project");
        assert_eq!(essay.path, "notes/Projects/Why vaults.md");
        assert_eq!(essay.properties.get("genre").and_then(|v| v.as_str()), Some("essay"));
        assert!(essay.body.starts_with("# Why vaults\n"));
        assert!(essay.body.contains("- [ ] Write it"));

        // Promote an idea.
        let mut idea = vault.create("Solar still", Some("idea"), Some("notes/Ideas")).unwrap();
        idea.body = "# Solar still\n\nTop up the tank from dew.\n".into();
        idea.properties
            .insert("status".into(), serde_json::Value::String("someday".into()));
        vault.write(&idea).unwrap();
        let promoted = vault.promote_to_project(&idea.id, Some("build")).unwrap();
        assert_eq!(promoted.note_type, "project");
        assert_eq!(promoted.id, idea.id);
        assert_eq!(promoted.path, "notes/Ideas/Solar still.md");
        assert_eq!(promoted.properties["status"], "someday", "existing properties win");
        assert!(promoted
            .body
            .starts_with("# Solar still\n\nTop up the tank from dew.\n\n## Design"));
        assert!(promoted.body.contains("- [ ] Order parts"));

        // A decision and a linked note.
        let mut decision = vault
            .create_from_template("Use peristaltic pump", "decision", None)
            .unwrap();
        decision
            .properties
            .insert("project".into(), serde_json::Value::String("[[Solar still]]".into()));
        vault.write(&decision).unwrap();
        let mut log = vault.create("Dew log", None, None).unwrap();
        log.body =
            "Measured 30 ml on [[Solar still]].\n- [?] Does it work in winter? project:: [[Solar still]]\n".into();
        vault.write(&log).unwrap();
        // A source that names the project only in its front-matter.
        let mut source = vault.create("Still design paper", Some("source"), None).unwrap();
        source
            .properties
            .insert("project".into(), serde_json::Value::String("[[Solar still]]".into()));
        vault.write(&source).unwrap();
        let order_parts = vault
            .items(&ItemFilter {
                note: Some(promoted.id.clone()),
                text: Some("Order parts".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(order_parts.len(), 1);
        vault
            .set_item_status(&promoted.id, order_parts[0].item.line, ItemStatus::Done)
            .unwrap();

        let hub = vault.project_hub(&promoted.id).unwrap();
        assert_eq!(hub.project.title, "Solar still");
        assert_eq!(hub.decisions.len(), 1);
        assert_eq!(hub.decisions[0].title, "Use peristaltic pump");
        assert_eq!(hub.sources.len(), 1);
        assert_eq!(hub.sources[0].title, "Still design paper");
        assert_eq!(hub.linked.len(), 1);
        assert_eq!(hub.linked[0].note.title, "Dew log");
        assert!(hub.linked[0].incoming);
        assert_eq!(hub.tasks.len(), 4);
        assert_eq!(hub.gaps.len(), 1);
        assert_eq!(hub.gaps[0].item.text, "Does it work in winter?");
        assert_eq!(
            (hub.progress.tasks, hub.progress.done, hub.progress.percent),
            (4, 1, 25)
        );
        assert_eq!(hub.progress.gaps_open, 1);

        // Queries over notes and items.
        let r = vault.query("type:project sort:title").unwrap();
        assert_eq!(r.domain, Domain::Notes);
        let titles: Vec<&str> = r.notes.iter().map(|n| n.note.title.as_str()).collect();
        assert_eq!(titles, vec!["Solar still", "Why vaults"]);
        let r = vault.query("type:gap project:[[Solar still]]").unwrap();
        assert_eq!(r.domain, Domain::Items);
        assert_eq!(r.items.len(), 1);
        let r = vault.query("type:task group:project sort:text").unwrap();
        assert_eq!(r.groups.len(), 2);
        assert_eq!(r.total, 5);

        // Saved queries.
        let mut saved = vault.create_from_template("Open gaps", "query", None).unwrap();
        saved
            .properties
            .insert("query".into(), serde_json::Value::String("type:gap status:open".into()));
        vault.write(&saved).unwrap();
        let r = vault.run_saved_query(&saved.id).unwrap();
        assert_eq!(r.items.len(), 1);
        assert!(vault.run_saved_query(&log.id).is_err());
    }

    #[test]
    fn strips_only_a_matching_title_heading() {
        assert_eq!(strip_title_heading("# A\n\nbody\n", "A"), "body\n");
        assert_eq!(strip_title_heading("# B\n\nbody\n", "A"), "# B\n\nbody\n");
        assert_eq!(strip_title_heading("# A", "A"), "");
        assert_eq!(strip_title_heading("plain\n", "A"), "plain\n");
    }
}
