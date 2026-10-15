//! Inline tasks and gaps.
//!
//! A task is a GFM task-list item anywhere in a note: `- [ ] text`. A gap
//! (an information need) is `- [?] I need to know …`. Both are parsed from
//! the body when a note is indexed and stored in the `items` table so views
//! can query them; editing one rewrites just its line in the file (the file
//! stays the source of truth, ADR-002). See `docs/spec/vault-format.md`.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum ItemKind {
    Task,
    Gap,
}

impl ItemKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ItemKind::Task => "task",
            ItemKind::Gap => "gap",
        }
    }

    pub fn parse(s: &str) -> Option<ItemKind> {
        match s {
            "task" => Some(ItemKind::Task),
            "gap" => Some(ItemKind::Gap),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum ItemStatus {
    /// `[ ]`, or a gap `[?]` with no answer yet.
    Open,
    /// `[/]` in progress.
    Doing,
    /// `[x]`.
    Done,
    /// `[-]`.
    Cancelled,
    /// A gap with an `answered:: [[Note]]` field.
    Answered,
}

impl ItemStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ItemStatus::Open => "open",
            ItemStatus::Doing => "doing",
            ItemStatus::Done => "done",
            ItemStatus::Cancelled => "cancelled",
            ItemStatus::Answered => "answered",
        }
    }

    pub fn parse(s: &str) -> Option<ItemStatus> {
        match s {
            "open" => Some(ItemStatus::Open),
            "doing" => Some(ItemStatus::Doing),
            "done" => Some(ItemStatus::Done),
            "cancelled" => Some(ItemStatus::Cancelled),
            "answered" => Some(ItemStatus::Answered),
            _ => None,
        }
    }

    /// Still needs doing (or answering).
    pub fn is_open(self) -> bool {
        matches!(self, ItemStatus::Open | ItemStatus::Doing)
    }

    /// The checkbox character for a task in this status.
    pub fn task_marker(self) -> char {
        match self {
            ItemStatus::Open | ItemStatus::Answered => ' ',
            ItemStatus::Doing => '/',
            ItemStatus::Done => 'x',
            ItemStatus::Cancelled => '-',
        }
    }
}

/// One task or gap line.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Item {
    pub note_id: String,
    /// 0-based line number in the note body (not counting front-matter).
    pub line: usize,
    pub kind: ItemKind,
    pub status: ItemStatus,
    /// The item text without the checkbox, inline fields and tags.
    pub text: String,
    /// Everything after the checkbox, verbatim.
    pub raw: String,
    /// Nesting depth, 0 for a top-level item.
    pub depth: usize,
    /// Line of the enclosing task/gap, if nested under one.
    pub parent: Option<usize>,
    /// Nearest heading above the item.
    pub heading: Option<String>,
    /// `due:: 2026-11-01`
    pub due: Option<String>,
    /// 1 (highest) to 4, from `priority:: p1` or a `#p1` tag.
    pub priority: Option<u8>,
    pub tags: Vec<String>,
    /// Raw link target of `project:: [[X]]` on this item or an ancestor.
    pub project: Option<String>,
    /// Raw link target of `answered:: [[X]]` (gaps).
    pub answer: Option<String>,
    /// Every inline field on the line, including the ones above.
    pub fields: BTreeMap<String, String>,
}

fn item_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^(?P<indent>[ \t]*)(?:[-*+]|\d+[.)])[ \t]+\[(?P<m>[ xX/?-])\][ \t]+(?P<rest>.*?)[ \t]*$").unwrap()
    })
}

fn bullet_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(?P<indent>[ \t]*)(?:[-*+]|\d+[.)])[ \t]+").unwrap())
}

fn field_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?:^|\s)(?P<key>[A-Za-z][\w-]*)::[ \t]*").unwrap())
}

fn tag_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?:^|\s)#(?P<tag>[\w/-]+)").unwrap())
}

fn heading_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^#{1,6}[ \t]+(?P<text>.+?)[ \t]*#*[ \t]*$").unwrap())
}

fn indent_width(s: &str) -> usize {
    s.chars().map(|c| if c == '\t' { 4 } else { 1 }).sum()
}

/// Strip `[[` `]]` (and a `|display` or `#heading` part) from a link target.
pub fn link_target(value: &str) -> String {
    let v = value.trim();
    let inner = v.strip_prefix("[[").and_then(|s| s.strip_suffix("]]")).unwrap_or(v);
    let inner = inner.split('|').next().unwrap_or(inner);
    let inner = inner.split('#').next().unwrap_or(inner);
    inner.trim().to_string()
}

/// Split an item's raw text into `(text, fields)`: `Order motors due:: 2026-11-01 #p1`
/// gives `("Order motors #p1", {due: "2026-11-01"})`.
pub fn split_fields(raw: &str) -> (String, BTreeMap<String, String>) {
    let mut fields = BTreeMap::new();
    let marks: Vec<(usize, usize, String)> = field_re()
        .captures_iter(raw)
        .map(|c| {
            let key = c.name("key").unwrap();
            (key.start(), c.get(0).unwrap().end(), key.as_str().to_lowercase())
        })
        .collect();
    if marks.is_empty() {
        return (raw.trim().to_string(), fields);
    }
    let text = raw[..marks[0].0].trim().to_string();
    for (i, (_, value_start, key)) in marks.iter().enumerate() {
        let end = marks.get(i + 1).map(|m| m.0).unwrap_or(raw.len());
        let value = raw[*value_start..end].trim();
        // A value stops at the first `#tag`, so `due:: 2026-11-01 #p1` works;
        // a `#` inside a link (`[[Note#Heading]]`) is not preceded by a space.
        let value = match tag_re().find(value) {
            Some(m) => value[..m.start()].trim(),
            None => value,
        };
        fields.insert(key.clone(), value.to_string());
    }
    (text, fields)
}

fn parse_priority(fields: &BTreeMap<String, String>, tags: &[String]) -> Option<u8> {
    let from_field = fields.get("priority").and_then(|v| {
        let v = v.trim().to_lowercase();
        match v.as_str() {
            "p1" | "1" | "high" | "highest" | "urgent" => Some(1),
            "p2" | "2" | "medium" | "normal" => Some(2),
            "p3" | "3" | "low" => Some(3),
            "p4" | "4" | "lowest" | "someday" => Some(4),
            _ => None,
        }
    });
    from_field.or_else(|| {
        tags.iter().find_map(|t| match t.as_str() {
            "p1" => Some(1),
            "p2" => Some(2),
            "p3" => Some(3),
            "p4" => Some(4),
            _ => None,
        })
    })
}

/// Parse every task and gap in a note body.
pub fn parse_items(note_id: &str, body: &str) -> Vec<Item> {
    let mut out: Vec<Item> = Vec::new();
    // (indent width, index into `out` when the entry is a task/gap)
    let mut stack: Vec<(usize, Option<usize>)> = Vec::new();
    let mut heading: Option<String> = None;
    let mut in_fence = false;

    for (line_no, line) in body.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            stack.clear();
            continue;
        }
        if in_fence {
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        if let Some(h) = heading_re().captures(line) {
            heading = Some(h.name("text").unwrap().as_str().to_string());
            stack.clear();
            continue;
        }
        let Some(bullet) = bullet_re().captures(line) else {
            // A paragraph (or anything that is not a list item) ends the list,
            // unless it is indented continuation text under an item.
            let width = indent_width(line) - indent_width(line.trim_start());
            if stack.last().map(|(w, _)| width <= *w).unwrap_or(true) {
                stack.clear();
            }
            continue;
        };
        let indent = indent_width(bullet.name("indent").unwrap().as_str());
        while stack.last().map(|(w, _)| *w >= indent).unwrap_or(false) {
            stack.pop();
        }
        let parent_idx = stack.iter().rev().find_map(|(_, i)| *i);
        let depth = stack.len();

        let Some(cap) = item_re().captures(line) else {
            stack.push((indent, None));
            continue;
        };
        let marker = cap.name("m").unwrap().as_str().chars().next().unwrap_or(' ');
        let raw = cap.name("rest").unwrap().as_str().to_string();
        let (kind, mut status) = match marker {
            'x' | 'X' => (ItemKind::Task, ItemStatus::Done),
            '/' => (ItemKind::Task, ItemStatus::Doing),
            '-' => (ItemKind::Task, ItemStatus::Cancelled),
            '?' => (ItemKind::Gap, ItemStatus::Open),
            _ => (ItemKind::Task, ItemStatus::Open),
        };
        let (text_with_tags, fields) = split_fields(&raw);
        // Tags count wherever they are on the line, also after a field value.
        let tags: Vec<String> = tag_re()
            .captures_iter(&raw)
            .map(|c| c.name("tag").unwrap().as_str().to_string())
            .collect();
        let text = tag_re().replace_all(&text_with_tags, "").trim().to_string();
        let answer = fields.get("answered").map(|v| link_target(v)).filter(|s| !s.is_empty());
        if kind == ItemKind::Gap && answer.is_some() {
            status = ItemStatus::Answered;
        }
        let project = fields
            .get("project")
            .map(|v| link_target(v))
            .filter(|s| !s.is_empty())
            .or_else(|| parent_idx.and_then(|i| out[i].project.clone()));
        let priority = parse_priority(&fields, &tags);
        let due = fields
            .get("due")
            .map(|d| d.trim().to_string())
            .filter(|d| !d.is_empty());
        out.push(Item {
            note_id: note_id.to_string(),
            line: line_no,
            kind,
            status,
            text,
            raw,
            depth,
            parent: parent_idx.map(|i| out[i].line),
            heading: heading.clone(),
            due,
            priority,
            tags,
            project,
            answer,
            fields,
        });
        stack.push((indent, Some(out.len() - 1)));
    }
    out
}

/// Rewrite the checkbox of the item on `line` of `body`. Returns the new
/// body, or `None` if that line is not a task/gap.
pub fn set_marker(body: &str, line: usize, marker: char) -> Option<String> {
    let mut lines: Vec<String> = body.split_inclusive('\n').map(str::to_string).collect();
    let target = lines.get_mut(line)?;
    let cap = item_re().captures(target.trim_end_matches(['\r', '\n']))?;
    let m = cap.name("m").unwrap();
    target.replace_range(m.start()..m.end(), &marker.to_string());
    Some(lines.concat())
}

/// Append an inline field to the item on `line`.
pub fn append_field(body: &str, line: usize, key: &str, value: &str) -> Option<String> {
    let mut lines: Vec<String> = body.split_inclusive('\n').map(str::to_string).collect();
    let target = lines.get_mut(line)?;
    item_re().captures(target.trim_end_matches(['\r', '\n']))?;
    let ending: String = target
        .chars()
        .rev()
        .take_while(|c| *c == '\n' || *c == '\r')
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let content = target.trim_end_matches(['\r', '\n']).trim_end().to_string();
    *target = format!("{content} {key}:: {value}{ending}");
    Some(lines.concat())
}

/// Add a task or gap line to `body`: under the `## Tasks` heading (or
/// `## Open questions` for gaps) when the note has one, else at the end.
/// Returns the new body and the line the item landed on.
pub fn append_item(body: &str, kind: ItemKind, text: &str) -> (String, usize) {
    let marker = match kind {
        ItemKind::Task => ' ',
        ItemKind::Gap => '?',
    };
    let new_line = format!("- [{marker}] {}", text.trim());
    let wanted = match kind {
        ItemKind::Task => "tasks",
        ItemKind::Gap => "open questions",
    };
    let lines: Vec<&str> = body.lines().collect();
    let section = lines.iter().position(|l| {
        heading_re()
            .captures(l)
            .map(|c| c.name("text").unwrap().as_str().trim().to_lowercase() == wanted)
            .unwrap_or(false)
    });
    if let Some(start) = section {
        // Insert after the last item of the section's first list.
        let mut insert_at = start + 1;
        let mut i = start + 1;
        while i < lines.len() {
            let l = lines[i];
            if l.trim().is_empty() {
                // blank lines inside a list are fine; the next non-blank decides
            } else if bullet_re().is_match(l) {
                insert_at = i + 1;
            } else if insert_at > start + 1 && l.starts_with(' ') {
                // indented continuation of the previous item
            } else {
                break;
            }
            i += 1;
        }
        let mut out: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        out.insert(insert_at, new_line);
        let mut text = out.join("\n");
        if body.ends_with('\n') || !text.ends_with('\n') {
            text.push('\n');
        }
        return (text, insert_at);
    }
    let mut text = body.trim_end_matches(['\r', '\n']).to_string();
    let line = if text.is_empty() {
        0
    } else {
        let n = text.lines().count();
        text.push('\n');
        // A blank line before the item when the body does not end in a list.
        if !bullet_re().is_match(body.lines().last().unwrap_or("")) {
            text.push('\n');
            n + 1
        } else {
            n
        }
    };
    text.push_str(&new_line);
    text.push('\n');
    (text, line)
}

/// Roll-up of a set of items.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Progress {
    pub tasks: usize,
    pub done: usize,
    pub doing: usize,
    pub open: usize,
    pub cancelled: usize,
    pub gaps: usize,
    pub gaps_open: usize,
    /// 0-100 share of tasks that are done (cancelled ones are left out).
    pub percent: u8,
}

impl Progress {
    pub fn of<'a>(items: impl IntoIterator<Item = &'a Item>) -> Progress {
        let mut p = Progress::default();
        for it in items {
            match it.kind {
                ItemKind::Task => {
                    p.tasks += 1;
                    match it.status {
                        ItemStatus::Done => p.done += 1,
                        ItemStatus::Doing => p.doing += 1,
                        ItemStatus::Cancelled => p.cancelled += 1,
                        _ => p.open += 1,
                    }
                }
                ItemKind::Gap => {
                    p.gaps += 1;
                    if it.status.is_open() {
                        p.gaps_open += 1;
                    }
                }
            }
        }
        let counted = p.tasks.saturating_sub(p.cancelled);
        p.percent = (p.done * 100).checked_div(counted).unwrap_or(0) as u8;
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &str = "# Garden Robot\n\n## Tasks\n\n- [ ] Order motors due:: 2026-11-01 #p1 #hardware\n  - [x] Compare drivers\n  - [?] Which voltage does the pump need? blocks:: [[Pump]]\n- [/] Build chassis project:: [[Chassis v3]]\n    - [ ] Print brackets\n- [-] Old idea\n\nSome text.\n\n```\n- [ ] not a task, in code\n```\n\n## Open questions\n\n- [?] Is a 12 V supply enough? answered:: [[Power budget]]\n";

    #[test]
    fn parses_tasks_gaps_nesting_and_fields() {
        let items = parse_items("N", BODY);
        assert_eq!(items.len(), 7);
        let t = &items[0];
        assert_eq!((t.kind, t.status), (ItemKind::Task, ItemStatus::Open));
        assert_eq!(t.text, "Order motors");
        assert_eq!(t.due.as_deref(), Some("2026-11-01"));
        assert_eq!(t.priority, Some(1));
        assert_eq!(t.tags, vec!["p1", "hardware"]);
        assert_eq!(t.heading.as_deref(), Some("Tasks"));
        assert_eq!(t.depth, 0);
        assert_eq!(t.line, 4);

        let sub = &items[1];
        assert_eq!(sub.status, ItemStatus::Done);
        assert_eq!(sub.depth, 1);
        assert_eq!(sub.parent, Some(4));

        let gap = &items[2];
        assert_eq!((gap.kind, gap.status), (ItemKind::Gap, ItemStatus::Open));
        assert_eq!(gap.text, "Which voltage does the pump need?");
        assert_eq!(gap.fields.get("blocks").map(String::as_str), Some("[[Pump]]"));

        let chassis = &items[3];
        assert_eq!(chassis.status, ItemStatus::Doing);
        assert_eq!(chassis.project.as_deref(), Some("Chassis v3"));
        assert_eq!(items[4].project.as_deref(), Some("Chassis v3"), "project is inherited");
        assert_eq!(items[4].parent, Some(7));
        assert_eq!(items[5].status, ItemStatus::Cancelled);

        let answered = &items[6];
        assert_eq!(answered.status, ItemStatus::Answered);
        assert_eq!(answered.answer.as_deref(), Some("Power budget"));
        assert_eq!(answered.heading.as_deref(), Some("Open questions"));
    }

    #[test]
    fn progress_rolls_up() {
        let items = parse_items("N", BODY);
        let p = Progress::of(&items);
        assert_eq!((p.tasks, p.done, p.doing, p.open, p.cancelled), (5, 1, 1, 2, 1));
        assert_eq!((p.gaps, p.gaps_open), (2, 1));
        assert_eq!(p.percent, 25);
    }

    #[test]
    fn rewrites_markers_and_fields_in_place() {
        let body = set_marker(BODY, 4, 'x').unwrap();
        assert!(body.lines().nth(4).unwrap().starts_with("- [x] Order motors"));
        assert_eq!(body.lines().count(), BODY.lines().count());
        assert!(set_marker(BODY, 0, 'x').is_none(), "a heading is not an item");

        let body = append_field(BODY, 6, "answered", "[[Pump spec]]").unwrap();
        let items = parse_items("N", &body);
        assert_eq!(items[2].status, ItemStatus::Answered);
        assert_eq!(items[2].answer.as_deref(), Some("Pump spec"));
    }

    #[test]
    fn appends_under_the_matching_heading_or_at_the_end() {
        let (body, line) = append_item(BODY, ItemKind::Task, "Wire it up");
        assert_eq!(line, 10);
        assert_eq!(body.lines().nth(10).unwrap(), "- [ ] Wire it up");
        assert!(body.lines().nth(11).unwrap().is_empty());

        let (body, line) = append_item("Just prose.", ItemKind::Gap, "How big?");
        assert_eq!(body, "Just prose.\n\n- [?] How big?\n");
        assert_eq!(line, 2);
        let items = parse_items("N", &body);
        assert_eq!(items[0].line, line);

        let (body, line) = append_item("", ItemKind::Task, "First");
        assert_eq!((body.as_str(), line), ("- [ ] First\n", 0));
    }

    #[test]
    fn link_targets_and_field_splitting() {
        assert_eq!(link_target("[[Garden Robot|GR]]"), "Garden Robot");
        assert_eq!(link_target("[[Note#Heading]]"), "Note");
        assert_eq!(link_target("plain"), "plain");
        let (text, fields) = split_fields("Do a thing due:: 2026-01-02 priority:: p2");
        assert_eq!(text, "Do a thing");
        assert_eq!(fields.get("due").map(String::as_str), Some("2026-01-02"));
        assert_eq!(fields.get("priority").map(String::as_str), Some("p2"));
        assert_eq!(split_fields("no fields here").0, "no fields here");
    }
}
