//! Link extraction from Markdown bodies.
//!
//! Syntax (see the vault spec):
//! - `[[Target]]`, `[[Target#Heading]]`, `[[Target|Display text]]`
//! - `![[file.png]]` embeds (an embed is a link with `kind = Embed`)
//! - `key:: [[Target]]` typed links (inline field on the same line)
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

fn field_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // `relation:: ` at the start of a line or list item, e.g. `- depends-on:: [[X]]`
    RE.get_or_init(|| Regex::new(r"^\s*(?:[-*+]\s+)?([A-Za-z][\w-]*)::\s").unwrap())
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
            }
        }
        offset += line.len();
    }
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
}
