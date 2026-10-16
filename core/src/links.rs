//! Link extraction from Markdown bodies.
//!
//! Syntax (see the vault spec):
//! - `[[Target]]`, `[[Target#Heading]]`, `[[Target|Display text]]`
//! - `![[file.png]]` embeds (an embed is a link with `kind = Embed`)
//! - `key:: [[Target]]` typed links (inline field on the same line)
//! - `[text](path/to/file.pdf)` and `![alt](attachments/x.png)` file links:
//!   a relative path with no scheme is a link to an asset (`kind = File`);
//!   a relative `.md` path is a link to that note.
//!
//! Fenced and inline code are skipped.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LinkKind {
    Wiki,
    Embed,
    /// `[text](relative/path.ext)` or `![alt](relative/path.ext)` to a file in the vault.
    File,
}

impl LinkKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            LinkKind::Wiki => "wiki",
            LinkKind::Embed => "embed",
            LinkKind::File => "file",
        }
    }

    pub fn parse(s: &str) -> LinkKind {
        match s {
            "embed" => LinkKind::Embed,
            "file" => LinkKind::File,
            _ => LinkKind::Wiki,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Link {
    /// The note title, alias or path the link points at (before `#` / `|`).
    pub target: String,
    pub heading: Option<String>,
    pub display: Option<String>,
    /// Relation name when written as `relation:: [[Target]]`.
    pub relation: Option<String>,
    pub kind: LinkKind,
    /// Byte offset of the link in the body (for ordering and UI).
    pub position: usize,
}

fn link_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(!?)\[\[([^\[\]\n]+?)\]\]").unwrap())
}

fn md_link_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // `[text](target)` / `![alt](target)` with an optional "title"; targets with spaces are allowed
    // (the editor writes them unescaped) but not nested parentheses.
    RE.get_or_init(|| Regex::new(r#"(!?)\[([^\[\]\n]*)\]\(([^()\n]+?)(?:\s+"[^"\n]*")?\)"#).unwrap())
}

/// A Markdown link target that points inside the vault: no scheme, no
/// anchor-only target, no absolute path.
fn vault_path_target(raw: &str) -> Option<String> {
    let t = raw.trim().trim_matches('<').trim_matches('>');
    if t.is_empty() || t.starts_with('#') || t.starts_with('/') || t.starts_with("//") {
        return None;
    }
    // A scheme (`https:`, `mailto:`, `file:`) or a Windows drive makes it external.
    if let Some(i) = t.find(':') {
        let head = &t[..i];
        if !head.is_empty()
            && head
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '.' || c == '-')
        {
            return None;
        }
    }
    let t = t.split('#').next().unwrap_or(t);
    let decoded = percent_decode(t);
    let decoded = decoded.trim_start_matches("./").to_string();
    if decoded.is_empty() || decoded.split('/').any(|seg| seg == "..") {
        return None;
    }
    Some(decoded)
}

fn percent_decode(s: &str) -> String {
    if !s.contains('%') {
        return s.to_string();
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn field_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // `relation:: ` at the start of a line or list item, e.g. `- depends-on:: [[X]]`
    RE.get_or_init(|| Regex::new(r"^\s*(?:[-*+]\s+)?([A-Za-z][\w-]*)::\s").unwrap())
}

/// Wiki links written in front-matter property values, such as
/// `project: "[[Garden Robot]]"` or `related: ["[[A]]", "[[B]]"]`. Each is a
/// typed link whose relation is the property's key. They sort before the
/// body's links (position 0).
pub fn extract_property_links(properties: &serde_json::Map<String, serde_json::Value>) -> Vec<Link> {
    fn strings<'a>(v: &'a serde_json::Value, out: &mut Vec<&'a str>) {
        match v {
            serde_json::Value::String(s) => out.push(s),
            serde_json::Value::Array(a) => a.iter().for_each(|x| strings(x, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for (key, value) in properties {
        let mut texts = Vec::new();
        strings(value, &mut texts);
        for text in texts {
            for cap in link_re().captures_iter(text) {
                let inner = cap.get(2).unwrap().as_str();
                let target_part = inner.split_once('|').map_or(inner, |(t, _)| t);
                let (target, heading) = match target_part.split_once('#') {
                    Some((t, h)) => (t.trim(), Some(h.trim().to_string())),
                    None => (target_part.trim(), None),
                };
                if target.is_empty() {
                    continue;
                }
                out.push(Link {
                    target: target.to_string(),
                    heading,
                    display: None,
                    relation: Some(key.clone()),
                    kind: LinkKind::Wiki,
                    position: 0,
                });
            }
        }
    }
    out
}

/// Extract every link in `body`, in document order.
pub fn extract_links(body: &str) -> Vec<Link> {
    let mut out = Vec::new();
    let mut in_fence = false;
    let mut offset = 0usize;

    for line in body.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            offset += line.len();
            continue;
        }
        if !in_fence {
            let relation = field_re()
                .captures(line)
                .map(|c| c.get(1).unwrap().as_str().to_string());
            for (start, end, text) in strip_inline_code(line) {
                for cap in link_re().captures_iter(text) {
                    let m = cap.get(0).unwrap();
                    let inner = cap.get(2).unwrap().as_str();
                    let kind = if cap.get(1).unwrap().as_str() == "!" {
                        LinkKind::Embed
                    } else {
                        LinkKind::Wiki
                    };
                    let (target_part, display) = match inner.split_once('|') {
                        Some((t, d)) => (t, Some(d.trim().to_string())),
                        None => (inner, None),
                    };
                    let (target, heading) = match target_part.split_once('#') {
                        Some((t, h)) => (t, Some(h.trim().to_string())),
                        None => (target_part, None),
                    };
                    let target = target.trim();
                    if target.is_empty() && heading.is_none() {
                        continue;
                    }
                    let _ = end;
                    out.push(Link {
                        target: target.to_string(),
                        heading,
                        display,
                        relation: relation.clone(),
                        kind,
                        position: offset + start + m.start(),
                    });
                }
                for cap in md_link_re().captures_iter(text) {
                    let m = cap.get(0).unwrap();
                    let Some(path) = vault_path_target(cap.get(3).unwrap().as_str()) else {
                        continue;
                    };
                    let display = cap
                        .get(2)
                        .map(|d| d.as_str().trim().to_string())
                        .filter(|d| !d.is_empty());
                    if path.to_lowercase().ends_with(".md") {
                        out.push(Link {
                            target: path.trim_end_matches(".md").trim_end_matches(".MD").to_string(),
                            heading: None,
                            display,
                            relation: relation.clone(),
                            kind: LinkKind::Wiki,
                            position: offset + start + m.start(),
                        });
                    } else {
                        out.push(Link {
                            target: path,
                            heading: None,
                            display,
                            relation: relation.clone(),
                            kind: LinkKind::File,
                            position: offset + start + m.start(),
                        });
                    }
                }
            }
        }
        offset += line.len();
    }
    out.sort_by_key(|l| l.position);
    out
}

/// Yield the segments of `line` that are outside inline code spans, as
/// `(start_offset, end_offset, text)`.
fn strip_inline_code(line: &str) -> Vec<(usize, usize, &str)> {
    let mut segs = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    let bytes = line.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'`' {
            // find closing backtick
            if let Some(rel) = line[i + 1..].find('`') {
                if start < i {
                    segs.push((start, i, &line[start..i]));
                }
                i = i + 1 + rel + 1;
                start = i;
                continue;
            }
        }
        i += 1;
    }
    if start < line.len() {
        segs.push((start, line.len(), &line[start..]));
    }
    segs
}

/// Normalise a link target for lookup: case-insensitive, `.md` stripped,
/// folders stripped (links resolve by title/alias first, then by path).
pub fn normalise_target(target: &str) -> String {
    let t = target.trim().trim_end_matches(".md");
    let t = t.rsplit('/').next().unwrap_or(t);
    t.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_file_links_and_markdown_note_links() {
        let body = "See ![plan](attachments/Garden/plan.png) and [the spec](docs/spec%20v2.pdf \"title\").\nAlso [web](https://x.y/a.png), [anchor](#top), [note](notes/Other.md) and `[code](a.png)`.\n- source:: [data](attachments/data.csv)";
        let links = extract_links(body);
        let files: Vec<(&str, LinkKind, Option<&str>)> = links
            .iter()
            .map(|l| (l.target.as_str(), l.kind, l.relation.as_deref()))
            .collect();
        assert_eq!(
            files,
            vec![
                ("attachments/Garden/plan.png", LinkKind::File, None),
                ("docs/spec v2.pdf", LinkKind::File, None),
                ("notes/Other", LinkKind::Wiki, None),
                ("attachments/data.csv", LinkKind::File, Some("source")),
            ]
        );
        assert_eq!(links[0].display.as_deref(), Some("plan"));
        assert!(links.windows(2).all(|w| w[0].position <= w[1].position));
    }

    #[test]
    fn finds_wiki_links_with_heading_display_and_embed() {
        let body = "See [[Garden Robot]] and [[Garden Robot#Power|the power section]].\n![[chassis.png]]\n";
        let links = extract_links(body);
        assert_eq!(links.len(), 3);
        assert_eq!(links[0].target, "Garden Robot");
        assert_eq!(links[0].kind, LinkKind::Wiki);
        assert_eq!(links[1].heading.as_deref(), Some("Power"));
        assert_eq!(links[1].display.as_deref(), Some("the power section"));
        assert_eq!(links[2].kind, LinkKind::Embed);
        assert_eq!(links[2].target, "chassis.png");
    }

    #[test]
    fn skips_code() {
        let body = "`[[not a link]]`\n\n```\n[[also not]]\n```\n[[real]]\n";
        let links = extract_links(body);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "real");
    }

    #[test]
    fn typed_links_via_inline_fields() {
        let body = "- depends-on:: [[Motor driver]]\nsource:: [[Datasheet]] and [[Other]]\n[[plain]]\n";
        let links = extract_links(body);
        assert_eq!(links[0].relation.as_deref(), Some("depends-on"));
        assert_eq!(links[1].relation.as_deref(), Some("source"));
        assert_eq!(links[2].relation.as_deref(), Some("source"));
        assert_eq!(links[3].relation, None);
    }

    #[test]
    fn positions_are_byte_offsets() {
        let body = "ab [[x]]\ncd [[y]]";
        let links = extract_links(body);
        assert_eq!(&body[links[0].position..links[0].position + 5], "[[x]]");
        assert_eq!(&body[links[1].position..links[1].position + 5], "[[y]]");
    }

    #[test]
    fn normalises_targets() {
        assert_eq!(normalise_target("Projects/Garden Robot.md"), "garden robot");
        assert_eq!(normalise_target(" GR "), "gr");
    }

    #[test]
    fn property_links_are_typed_by_their_key() {
        let props: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
            r#"{"project": "[[Garden Robot]]", "related": ["[[A#Part]]", "plain", "[[B|shown]]"], "count": 3, "url": "https://x.y"}"#,
        )
        .unwrap();
        let links = extract_property_links(&props);
        let got: Vec<_> = links
            .iter()
            .map(|l| (l.target.as_str(), l.relation.as_deref(), l.heading.as_deref()))
            .collect();
        assert_eq!(
            got,
            vec![
                ("Garden Robot", Some("project"), None),
                ("A", Some("related"), Some("Part")),
                ("B", Some("related"), None),
            ]
        );
    }
}
