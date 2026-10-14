//! A note is a Markdown file with optional YAML front-matter.
//!
//! Reserved front-matter keys (see the vault spec): `id`, `type`, `title`,
//! `tags`, `aliases`, `created`, `updated`. Everything else is a user
//! property and is preserved verbatim.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_yaml::{Mapping, Value};

use crate::error::{Error, Result};

pub const DEFAULT_TYPE: &str = "note";
const RESERVED: &[&str] = &["id", "type", "title", "tags", "aliases", "created", "updated"];

/// The part of a note that is cheap to list.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NoteSummary {
    pub id: String,
    /// Path relative to the vault root, using `/` separators.
    pub path: String,
    pub title: String,
    #[serde(rename = "type")]
    pub note_type: String,
    pub tags: Vec<String>,
    pub updated: Option<DateTime<Utc>>,
}

/// A fully loaded note.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Note {
    pub id: String,
    pub path: String,
    pub title: String,
    #[serde(rename = "type")]
    pub note_type: String,
    pub tags: Vec<String>,
    pub aliases: Vec<String>,
    pub created: Option<DateTime<Utc>>,
    pub updated: Option<DateTime<Utc>>,
    /// User-defined front-matter keys, in file order.
    pub properties: serde_json::Map<String, serde_json::Value>,
    /// Markdown body without the front-matter block.
    pub body: String,
}

impl Note {
    pub fn summary(&self) -> NoteSummary {
        NoteSummary {
            id: self.id.clone(),
            path: self.path.clone(),
            title: self.title.clone(),
            note_type: self.note_type.clone(),
            tags: self.tags.clone(),
            updated: self.updated,
        }
    }

    /// Parse a note from its file text. `rel_path` is used for the default
    /// title and for error messages. A missing `id` is left empty; callers
    /// decide whether to mint one (the vault does, on first index).
    pub fn parse(rel_path: &str, text: &str) -> Result<Note> {
        let (front, body) = split_front_matter(text);
        let mapping: Mapping = match front {
            Some(yaml) if !yaml.trim().is_empty() => match serde_yaml::from_str::<Value>(yaml) {
                Ok(Value::Mapping(m)) => m,
                Ok(Value::Null) => Mapping::new(),
                Ok(_) => {
                    return Err(Error::FrontMatter {
                        path: PathBuf::from(rel_path),
                        reason: "front-matter must be a mapping".into(),
                    })
                }
                Err(e) => {
                    return Err(Error::FrontMatter {
                        path: PathBuf::from(rel_path),
                        reason: e.to_string(),
                    })
                }
            },
            _ => Mapping::new(),
        };

        let get_str = |k: &str| mapping.get(k).and_then(yaml_to_string);
        let get_list = |k: &str| -> Vec<String> {
            match mapping.get(k) {
                Some(Value::Sequence(seq)) => seq.iter().filter_map(yaml_to_string).collect(),
                Some(Value::String(s)) => s
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect(),
                _ => Vec::new(),
            }
        };
        let get_time = |k: &str| {
            get_str(k).and_then(|s| {
                DateTime::parse_from_rfc3339(&s)
                    .map(|d| d.with_timezone(&Utc))
                    .ok()
                    .or_else(|| {
                        chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d")
                            .ok()
                            .and_then(|d| d.and_hms_opt(0, 0, 0))
                            .map(|d| d.and_utc())
                    })
            })
        };

        let mut properties = serde_json::Map::new();
        for (k, v) in &mapping {
            if let Some(key) = k.as_str() {
                if !RESERVED.contains(&key) {
                    properties.insert(key.to_string(), yaml_to_json(v));
                }
            }
        }

        Ok(Note {
            id: get_str("id").unwrap_or_default(),
            path: rel_path.to_string(),
            title: get_str("title").unwrap_or_else(|| title_from_path(rel_path)),
            note_type: get_str("type").unwrap_or_else(|| DEFAULT_TYPE.to_string()),
            tags: get_list("tags"),
            aliases: get_list("aliases"),
            created: get_time("created"),
            updated: get_time("updated"),
            properties,
            body: body.to_string(),
        })
    }

    /// Serialise back to file text. Front-matter is emitted only if there is
    /// something to put in it, so a note that was plain Markdown with no id
    /// stays plain until it gets one.
    pub fn to_text(&self) -> String {
        let mut m = Mapping::new();
        if !self.id.is_empty() {
            m.insert("id".into(), self.id.clone().into());
        }
        if self.note_type != DEFAULT_TYPE {
            m.insert("type".into(), self.note_type.clone().into());
        }
        if self.title != title_from_path(&self.path) {
            m.insert("title".into(), self.title.clone().into());
        }
        if !self.tags.is_empty() {
            m.insert(
                "tags".into(),
                Value::Sequence(self.tags.iter().map(|t| t.clone().into()).collect()),
            );
        }
        if !self.aliases.is_empty() {
            m.insert(
                "aliases".into(),
                Value::Sequence(self.aliases.iter().map(|t| t.clone().into()).collect()),
            );
        }
        if let Some(c) = self.created {
            m.insert(
                "created".into(),
                c.to_rfc3339_opts(chrono::SecondsFormat::Secs, true).into(),
            );
        }
        if let Some(u) = self.updated {
            m.insert(
                "updated".into(),
                u.to_rfc3339_opts(chrono::SecondsFormat::Secs, true).into(),
            );
        }
        for (k, v) in &self.properties {
            m.insert(k.clone().into(), json_to_yaml(v));
        }

        let mut out = String::new();
        if !m.is_empty() {
            let yaml = serde_yaml::to_string(&Value::Mapping(m)).unwrap_or_default();
            out.push_str("---\n");
            out.push_str(&yaml);
            if !yaml.ends_with('\n') {
                out.push('\n');
            }
            out.push_str("---\n");
        }
        out.push_str(&self.body);
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out
    }
}

/// Split `---\n…\n---\n` front-matter off the top of a file.
/// Returns `(Some(yaml), body)` or `(None, whole_text)`.
pub fn split_front_matter(text: &str) -> (Option<&str>, &str) {
    let rest = text.strip_prefix("\u{feff}").unwrap_or(text);
    let after_open = match rest.strip_prefix("---\n").or_else(|| rest.strip_prefix("---\r\n")) {
        Some(a) => a,
        None => return (None, text),
    };
    // Closing fence is a line that is exactly `---` (or `...`).
    let mut offset = 0;
    for line in after_open.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed == "---" || trimmed == "..." {
            let yaml = &after_open[..offset];
            let body = &after_open[offset + line.len()..];
            return (Some(yaml), body);
        }
        offset += line.len();
    }
    (None, text)
}

pub fn title_from_path(rel_path: &str) -> String {
    Path::new(rel_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(rel_path)
        .to_string()
}

pub fn new_id() -> String {
    ulid::Ulid::generate().to_string()
}

fn yaml_to_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn yaml_to_json(v: &Value) -> serde_json::Value {
    match v {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Number(n) => n
            .as_i64()
            .map(serde_json::Value::from)
            .or_else(|| n.as_f64().map(serde_json::Value::from))
            .unwrap_or(serde_json::Value::Null),
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::Sequence(seq) => serde_json::Value::Array(seq.iter().map(yaml_to_json).collect()),
        Value::Mapping(m) => serde_json::Value::Object(
            m.iter()
                .filter_map(|(k, v)| yaml_to_string(k).map(|k| (k, yaml_to_json(v))))
                .collect(),
        ),
        Value::Tagged(t) => yaml_to_json(&t.value),
    }
}

fn json_to_yaml(v: &serde_json::Value) -> Value {
    match v {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => n
            .as_i64()
            .map(Value::from)
            .or_else(|| n.as_f64().map(Value::from))
            .unwrap_or(Value::Null),
        serde_json::Value::String(s) => Value::String(s.clone()),
        serde_json::Value::Array(a) => Value::Sequence(a.iter().map(json_to_yaml).collect()),
        serde_json::Value::Object(o) => Value::Mapping(
            o.iter()
                .map(|(k, v)| (Value::String(k.clone()), json_to_yaml(v)))
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_markdown_without_front_matter() {
        let n = Note::parse("Ideas/Solar still.md", "# Hello\n\nBody.\n").unwrap();
        assert_eq!(n.id, "");
        assert_eq!(n.title, "Solar still");
        assert_eq!(n.note_type, "note");
        assert_eq!(n.body, "# Hello\n\nBody.\n");
    }

    #[test]
    fn parses_front_matter_and_preserves_user_properties() {
        let text = "---\nid: 01ARZ3NDEKTSV4RRFFQ69G5FAV\ntype: project\ntags: [robot, garden]\naliases:\n  - GR\nstatus: active\nbudget: 250\n---\n# Garden Robot\n";
        let n = Note::parse("Projects/Garden Robot.md", text).unwrap();
        assert_eq!(n.id, "01ARZ3NDEKTSV4RRFFQ69G5FAV");
        assert_eq!(n.note_type, "project");
        assert_eq!(n.tags, vec!["robot", "garden"]);
        assert_eq!(n.aliases, vec!["GR"]);
        assert_eq!(n.properties["status"], "active");
        assert_eq!(n.properties["budget"], 250);
        assert_eq!(n.body, "# Garden Robot\n");
    }

    #[test]
    fn round_trips() {
        let text = "---\nid: 01ARZ3NDEKTSV4RRFFQ69G5FAV\ntype: project\ntags:\n- robot\nstatus: active\n---\n# Garden Robot\n\nText.\n";
        let n = Note::parse("Projects/Garden Robot.md", text).unwrap();
        let again = Note::parse("Projects/Garden Robot.md", &n.to_text()).unwrap();
        assert_eq!(n, again);
    }

    #[test]
    fn bad_front_matter_is_an_error_not_a_panic() {
        let err = Note::parse("x.md", "---\n- just\n- a list\n---\nbody").unwrap_err();
        assert!(matches!(err, Error::FrontMatter { .. }));
    }

    #[test]
    fn unterminated_front_matter_is_body() {
        let n = Note::parse("x.md", "---\nid: nope\nno closing fence").unwrap();
        assert_eq!(n.id, "");
        assert!(n.body.starts_with("---"));
    }
}
