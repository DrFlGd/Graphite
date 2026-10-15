//! The query language behind every view (G1).
//!
//! A query is a line of space-separated terms, for example
//! `type:task project:[[Garden Robot]] status:!done sort:due`. Terms are
//! `key:value` filters, `#tag`, or free text (full-text search on notes,
//! substring on tasks). `sort:field`, `sort:-field`, `group:field` and
//! `limit:n` shape the result. `type:task`, `type:gap` and `type:item`
//! query tasks and gaps (the `items` table); anything else queries notes.
//! The full list of keys is in `docs/spec/vault-format.md`.

use std::collections::HashMap;

use chrono::{Duration, Local, NaiveDate};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::index::{Index, ItemFilter, ItemRow};
use crate::links::normalise_target;
use crate::note::NoteSummary;
use crate::tasks::{link_target, ItemKind, ItemStatus};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Domain {
    Notes,
    Items,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Op {
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
    /// `key:*`: the field is set.
    Exists,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Term {
    pub key: String,
    pub op: Op,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Query {
    pub domain: Domain,
    pub terms: Vec<Term>,
    /// Free-text words.
    pub text: Vec<String>,
    /// `(field, ascending)`.
    pub sort: Vec<(String, bool)>,
    pub group: Option<String>,
    pub limit: Option<usize>,
}

/// A note as a query result row: summary plus its properties, for tables.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NoteRow {
    #[serde(flatten)]
    pub note: NoteSummary,
    pub created: Option<String>,
    pub properties: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Group {
    /// The group's value; empty for rows that have no value for the field.
    pub key: String,
    pub count: usize,
}

/// Rows come sorted; when the query groups, they are ordered group by group
/// and `groups` gives each group's key and size in that order.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueryResult {
    pub domain: Domain,
    pub notes: Vec<NoteRow>,
    pub items: Vec<ItemRow>,
    pub groups: Vec<Group>,
    pub total: usize,
}

/// A filter applied in Rust after the SQL part of a query.
type Post<T> = Box<dyn Fn(&T) -> bool>;
/// One `notes` row as read from SQLite: id, path, title, type, created, updated.
type RawNote = (String, String, String, String, Option<String>, Option<String>);

/// Split on whitespace, keeping `"quoted strings"` and `[[links]]` whole.
fn tokenize(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = s.chars().peekable();
    let mut in_quotes = false;
    let mut bracket_depth = 0usize;
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
            }
            '[' if !in_quotes && chars.peek() == Some(&'[') => {
                chars.next();
                bracket_depth += 1;
                cur.push_str("[[");
            }
            ']' if !in_quotes && bracket_depth > 0 && chars.peek() == Some(&']') => {
                chars.next();
                bracket_depth -= 1;
                cur.push_str("]]");
            }
            c if c.is_whitespace() && !in_quotes && bracket_depth == 0 => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn unwrap_value(v: &str) -> String {
    let v = v.trim().trim_matches('"');
    if v.starts_with("[[") {
        link_target(v)
    } else {
        v.to_string()
    }
}

impl Query {
    pub fn parse(text: &str) -> Query {
        let mut q = Query {
            domain: Domain::Notes,
            terms: Vec::new(),
            text: Vec::new(),
            sort: Vec::new(),
            group: None,
            limit: None,
        };
        for tok in tokenize(text) {
            if let Some(tag) = tok.strip_prefix('#') {
                if !tag.is_empty() {
                    q.terms.push(Term {
                        key: "tag".into(),
                        op: Op::Eq,
                        value: tag.to_string(),
                    });
                }
                continue;
            }
            let Some((key, rest)) = split_key(&tok) else {
                q.text.push(tok.trim_matches('"').to_string());
                continue;
            };
            let key = key.to_lowercase();
            match key.as_str() {
                "sort" => {
                    for part in rest.split(',') {
                        let part = part.trim();
                        if part.is_empty() {
                            continue;
                        }
                        match part.strip_prefix('-') {
                            Some(f) => q.sort.push((f.to_lowercase(), false)),
                            None => q.sort.push((part.trim_start_matches('+').to_lowercase(), true)),
                        }
                    }
                }
                "group" => q.group = Some(rest.trim().to_lowercase()).filter(|s| !s.is_empty()),
                "limit" => q.limit = rest.trim().parse().ok(),
                _ => {
                    let (op, value) = if rest == "*" {
                        (Op::Exists, String::new())
                    } else if let Some(v) = rest.strip_prefix("!=").or_else(|| rest.strip_prefix('!')) {
                        (Op::Ne, unwrap_value(v))
                    } else if let Some(v) = rest.strip_prefix(">=") {
                        (Op::Ge, unwrap_value(v))
                    } else if let Some(v) = rest.strip_prefix("<=") {
                        (Op::Le, unwrap_value(v))
                    } else if let Some(v) = rest.strip_prefix('>') {
                        (Op::Gt, unwrap_value(v))
                    } else if let Some(v) = rest.strip_prefix('<') {
                        (Op::Lt, unwrap_value(v))
                    } else {
                        (Op::Eq, unwrap_value(rest))
                    };
                    if key == "type" && op == Op::Eq && matches!(value.as_str(), "task" | "gap" | "item" | "items") {
                        q.domain = Domain::Items;
                    }
                    q.terms.push(Term { key, op, value });
                }
            }
        }
        q
    }
}

/// `key:rest` where the colon is the first one outside `[[ ]]` and quotes,
/// and the key is a plain word.
fn split_key(tok: &str) -> Option<(&str, &str)> {
    let idx = tok.find(':')?;
    let key = &tok[..idx];
    if key.is_empty() || !key.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
        return None;
    }
    // `http://…` and `[[a:b]]` are not terms.
    if tok.starts_with("[[") || tok[idx..].starts_with("://") {
        return None;
    }
    Some((key, &tok[idx + 1..]))
}

/// `today`, `tomorrow`, `yesterday`, `+3d`, `-1w`, `2026-11-01` → `YYYY-MM-DD`.
/// Anything else is returned unchanged.
pub fn resolve_date(v: &str) -> String {
    let today = Local::now().date_naive();
    let shift = |d: NaiveDate| d.format("%Y-%m-%d").to_string();
    match v.trim().to_lowercase().as_str() {
        "today" | "now" => return shift(today),
        "tomorrow" => return shift(today + Duration::days(1)),
        "yesterday" => return shift(today - Duration::days(1)),
        _ => {}
    }
    let s = v.trim();
    if let Some(body) = s.strip_prefix('+').or_else(|| s.strip_prefix('-')) {
        let sign: i64 = if s.starts_with('-') { -1 } else { 1 };
        let (num, unit) = body.split_at(body.len().saturating_sub(1));
        if let Ok(n) = num.parse::<i64>() {
            let days = match unit {
                "d" => Some(n),
                "w" => Some(n * 7),
                "m" => Some(n * 30),
                "y" => Some(n * 365),
                _ => None,
            };
            if let Some(days) = days {
                return shift(today + Duration::days(sign * days));
            }
        }
    }
    s.to_string()
}

fn is_date_key(key: &str) -> bool {
    matches!(
        key,
        "due" | "created" | "updated" | "date" | "decided" | "start" | "end"
    )
}

fn cmp_op(op: Op, ord: std::cmp::Ordering) -> bool {
    use std::cmp::Ordering::*;
    match op {
        Op::Eq => ord == Equal,
        Op::Ne => ord != Equal,
        Op::Gt => ord == Greater,
        Op::Lt => ord == Less,
        Op::Ge => ord != Less,
        Op::Le => ord != Greater,
        Op::Exists => true,
    }
}

/// Compare two values the way a person would: numbers numerically, else
/// case-insensitive text.
fn compare_values(a: &str, b: &str) -> std::cmp::Ordering {
    match (a.trim().parse::<f64>(), b.trim().parse::<f64>()) {
        (Ok(x), Ok(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
        _ => a.to_lowercase().cmp(&b.to_lowercase()),
    }
}

fn matches(op: Op, actual: Option<&str>, wanted: &str) -> bool {
    match (op, actual) {
        (Op::Exists, a) => a.map(|s| !s.is_empty()).unwrap_or(false),
        (Op::Ne, None) => true,
        (_, None) => false,
        (op, Some(a)) => cmp_op(op, compare_values(a, wanted)),
    }
}

fn json_text(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::Null => None,
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Array(a) => Some(a.iter().filter_map(json_text).collect::<Vec<_>>().join(", ")),
        other => Some(other.to_string()),
    }
}

impl Index {
    /// Run a query. See the module docs for the language.
    pub fn query(&self, q: &Query) -> Result<QueryResult> {
        match q.domain {
            Domain::Notes => self.query_notes(q),
            Domain::Items => self.query_items(q),
        }
    }

    /// Normalised keys (title, aliases, path) a link to this note can use.
    fn link_keys(&self, id: &str) -> Result<Vec<String>> {
        let Some(me) = self.summary(id)? else {
            return Ok(Vec::new());
        };
        let mut keys = vec![normalise_target(&me.title), normalise_target(&me.path)];
        let mut stmt = self
            .conn
            .prepare_cached("SELECT alias FROM aliases WHERE note_id = ?1")?;
        for a in stmt.query_map(params![id], |r| r.get::<_, String>(0))? {
            keys.push(normalise_target(&a?));
        }
        keys.sort();
        keys.dedup();
        Ok(keys)
    }

    fn query_notes(&self, q: &Query) -> Result<QueryResult> {
        let mut sql = String::from("SELECT n.id, n.path, n.title, n.type, n.created, n.updated FROM notes n WHERE 1=1");
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        // Filters that need a lookup first are applied in Rust afterwards.
        let mut post: Vec<Post<NoteRow>> = Vec::new();
        let mut project_filter: Option<(Op, Option<String>)> = None;

        for t in &q.terms {
            match t.key.as_str() {
                "type" => {
                    sql.push_str(if t.op == Op::Ne {
                        " AND n.type != ?"
                    } else {
                        " AND n.type = ?"
                    });
                    args.push(Box::new(t.value.to_lowercase()));
                }
                "tag" | "tags" => {
                    let not = if t.op == Op::Ne { "NOT " } else { "" };
                    sql.push_str(&format!(
                        " AND {not}EXISTS (SELECT 1 FROM tags t WHERE t.note_id = n.id AND (t.tag = ? OR t.tag LIKE ?))"
                    ));
                    args.push(Box::new(t.value.clone()));
                    args.push(Box::new(format!("{}/%", t.value)));
                }
                "in" | "folder" | "path" => {
                    let not = if t.op == Op::Ne { "NOT " } else { "" };
                    sql.push_str(&format!(" AND n.path {not}LIKE ?"));
                    let v = t.value.trim_matches('/');
                    args.push(Box::new(format!("{v}/%")));
                }
                "title" => {
                    let not = if t.op == Op::Ne { "NOT " } else { "" };
                    sql.push_str(&format!(" AND lower(n.title) {not}LIKE ?"));
                    args.push(Box::new(format!("%{}%", t.value.to_lowercase())));
                }
                "links" | "to" | "links-to" => {
                    let keys = match self.resolve(&t.value)? {
                        Some(id) => self.link_keys(&id)?,
                        None => vec![normalise_target(&t.value)],
                    };
                    let not = if t.op == Op::Ne { "NOT " } else { "" };
                    let ph = keys.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                    sql.push_str(&format!(
                        " AND {not}EXISTS (SELECT 1 FROM links l WHERE l.from_id = n.id AND l.target_norm IN ({ph}))"
                    ));
                    for k in keys {
                        args.push(Box::new(k));
                    }
                }
                "from" | "linked-from" | "backlinks" => {
                    let mut ids: Vec<String> = Vec::new();
                    if let Some(src) = self.resolve(&t.value)? {
                        for (_, resolved) in self.outgoing(&src)? {
                            if let Some(r) = resolved {
                                ids.push(r);
                            }
                        }
                    }
                    let not = if t.op == Op::Ne { "NOT " } else { "" };
                    if ids.is_empty() {
                        sql.push_str(if t.op == Op::Ne { " AND 1=1" } else { " AND 1=0" });
                    } else {
                        let ph = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                        sql.push_str(&format!(" AND n.id {not}IN ({ph})"));
                        for i in ids {
                            args.push(Box::new(i));
                        }
                    }
                }
                "project" => {
                    project_filter = Some((t.op, self.resolve(&t.value)?));
                }
                "created" | "updated" => {
                    let col = if t.key == "created" { "n.created" } else { "n.updated" };
                    let op = match t.op {
                        Op::Eq => "LIKE",
                        Op::Ne => "NOT LIKE",
                        Op::Gt => ">",
                        Op::Lt => "<",
                        Op::Ge => ">=",
                        Op::Le => "<=",
                        Op::Exists => "IS NOT NULL OR ? IS NOT",
                    };
                    let v = resolve_date(&t.value);
                    match t.op {
                        Op::Exists => sql.push_str(&format!(" AND {col} IS NOT NULL")),
                        Op::Eq | Op::Ne => {
                            sql.push_str(&format!(" AND {col} {op} ?"));
                            args.push(Box::new(format!("{v}%")));
                        }
                        Op::Lt | Op::Le => {
                            // `< 2026-11-01` means before that day; compare on the date part.
                            sql.push_str(&format!(" AND substr({col}, 1, 10) {op} ?"));
                            args.push(Box::new(v));
                        }
                        _ => {
                            sql.push_str(&format!(" AND substr({col}, 1, 10) {op} ?"));
                            args.push(Box::new(v));
                        }
                    }
                }
                "id" => {
                    sql.push_str(if t.op == Op::Ne {
                        " AND n.id != ?"
                    } else {
                        " AND n.id = ?"
                    });
                    args.push(Box::new(t.value.clone()));
                }
                key => {
                    // Any other key is a front-matter property.
                    let key = key.to_string();
                    let wanted = if is_date_key(&key) {
                        resolve_date(&t.value)
                    } else {
                        t.value.clone()
                    };
                    let op = t.op;
                    post.push(Box::new(move |row: &NoteRow| {
                        let actual = row.properties.get(&key).and_then(json_text);
                        matches(op, actual.as_deref(), &wanted)
                    }));
                }
            }
        }
        if !q.text.is_empty() {
            let fts: Vec<String> = q
                .text
                .iter()
                .map(|t| format!("\"{}\"*", t.replace('"', "\"\"")))
                .collect();
            sql.push_str(" AND n.id IN (SELECT id FROM notes_fts WHERE notes_fts MATCH ?)");
            args.push(Box::new(fts.join(" ")));
        }
        sql.push_str(" ORDER BY n.updated DESC, n.title COLLATE NOCASE");

        let mut stmt = self.conn.prepare(&sql)?;
        let params_vec: Vec<&dyn rusqlite::ToSql> = args.iter().map(|a| a.as_ref()).collect();
        let raw: Vec<RawNote> = stmt
            .query_map(params_vec.as_slice(), |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
            })?
            .collect::<std::result::Result<_, _>>()?;

        let mut rows: Vec<NoteRow> = Vec::with_capacity(raw.len());
        let mut props_stmt = self
            .conn
            .prepare_cached("SELECT key, value FROM properties WHERE note_id = ?1")?;
        let mut tags_stmt = self
            .conn
            .prepare_cached("SELECT tag FROM tags WHERE note_id = ?1 ORDER BY tag")?;
        let mut resolved: HashMap<String, Option<String>> = HashMap::new();
        for (id, path, title, note_type, created, updated) in raw {
            let mut properties = serde_json::Map::new();
            for p in props_stmt.query_map(params![id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
                let (k, v) = p?;
                let value = serde_json::from_str(&v).unwrap_or(serde_json::Value::String(v));
                properties.insert(k, value);
            }
            let tags: Vec<String> = tags_stmt
                .query_map(params![id], |r| r.get(0))?
                .collect::<std::result::Result<_, _>>()?;
            let row = NoteRow {
                note: NoteSummary {
                    id,
                    path,
                    title,
                    note_type,
                    tags,
                    updated: updated.and_then(|u| u.parse().ok()),
                },
                created,
                properties,
            };
            if let Some((op, want)) = &project_filter {
                let target = row
                    .properties
                    .get("project")
                    .and_then(json_text)
                    .map(|v| link_target(&v));
                let mine = match &target {
                    Some(t) => {
                        let key = normalise_target(t);
                        match resolved.get(&key) {
                            Some(v) => v.clone(),
                            None => {
                                let v = self.resolve(t)?;
                                resolved.insert(key, v.clone());
                                v
                            }
                        }
                    }
                    None => None,
                };
                let is_match = match op {
                    Op::Exists => mine.is_some(),
                    Op::Ne => mine != *want || want.is_none(),
                    _ => want.is_some() && mine == *want,
                };
                if !is_match {
                    continue;
                }
            }
            if post.iter().all(|f| f(&row)) {
                rows.push(row);
            }
        }

        // Sort.
        if !q.sort.is_empty() {
            rows.sort_by(|a, b| {
                for (field, asc) in &q.sort {
                    let va = note_sort_value(a, field);
                    let vb = note_sort_value(b, field);
                    let ord = match (&va, &vb) {
                        (None, None) => std::cmp::Ordering::Equal,
                        (None, Some(_)) => std::cmp::Ordering::Greater, // missing values last
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (Some(x), Some(y)) => {
                            let o = compare_values(x, y);
                            if *asc {
                                o
                            } else {
                                o.reverse()
                            }
                        }
                    };
                    if ord != std::cmp::Ordering::Equal {
                        return ord;
                    }
                }
                std::cmp::Ordering::Equal
            });
        }
        let total = rows.len();
        if let Some(l) = q.limit {
            rows.truncate(l);
        }
        let groups = match &q.group {
            Some(field) => group_rows(&mut rows, |r| note_group_value(r, field)),
            None => Vec::new(),
        };
        Ok(QueryResult {
            domain: Domain::Notes,
            notes: rows,
            items: Vec::new(),
            groups,
            total,
        })
    }

    fn query_items(&self, q: &Query) -> Result<QueryResult> {
        let mut filter = ItemFilter::default();
        let mut post: Vec<Post<ItemRow>> = Vec::new();
        for t in &q.terms {
            match t.key.as_str() {
                "type" => {
                    filter.kind = match t.value.as_str() {
                        "task" => Some(ItemKind::Task),
                        "gap" => Some(ItemKind::Gap),
                        _ => None,
                    };
                }
                "project" => match self.resolve(&t.value)? {
                    Some(id) if t.op != Op::Ne => filter.project = Some(id),
                    Some(id) => post.push(Box::new(move |r| r.project_id.as_deref() != Some(id.as_str()))),
                    None if t.op == Op::Exists => post.push(Box::new(|r| r.project_id.is_some())),
                    None if t.op == Op::Ne => {}
                    None => post.push(Box::new(|_| false)),
                },
                "note" | "in" | "path" | "folder" => {
                    if t.key == "note" {
                        match self.resolve(&t.value)? {
                            Some(id) if t.op != Op::Ne => filter.note = Some(id),
                            Some(id) => post.push(Box::new(move |r| r.item.note_id != id)),
                            None => post.push(Box::new(|_| false)),
                        }
                    } else {
                        let prefix = format!("{}/", t.value.trim_matches('/'));
                        let ne = t.op == Op::Ne;
                        post.push(Box::new(move |r| r.note_path.starts_with(&prefix) != ne));
                    }
                }
                "status" => {
                    let (op, v) = (t.op, t.value.to_lowercase());
                    post.push(Box::new(move |r| {
                        let s = r.item.status.as_str();
                        match (op, v.as_str()) {
                            // `status:open` covers doing too; `status:done` is done only.
                            (Op::Eq, "open") => r.item.status.is_open(),
                            (Op::Ne, "open") => !r.item.status.is_open(),
                            (Op::Eq, "closed") => !r.item.status.is_open(),
                            (Op::Ne, "closed") => r.item.status.is_open(),
                            (Op::Eq, w) => s == w,
                            (Op::Ne, w) => s != w,
                            _ => true,
                        }
                    }));
                }
                "due" => {
                    let (op, v) = (t.op, resolve_date(&t.value));
                    post.push(Box::new(move |r| matches(op, r.item.due.as_deref(), &v)));
                }
                "priority" => {
                    let (op, v) = (t.op, t.value.trim_start_matches('p').to_string());
                    post.push(Box::new(move |r| {
                        let p = r.item.priority.map(|p| p.to_string());
                        matches(op, p.as_deref(), &v)
                    }));
                }
                "tag" | "tags" => {
                    let (op, v) = (t.op, t.value.to_lowercase());
                    post.push(Box::new(move |r| {
                        let has = r
                            .item
                            .tags
                            .iter()
                            .any(|x| x.to_lowercase() == v || x.to_lowercase().starts_with(&format!("{v}/")));
                        has != (op == Op::Ne)
                    }));
                }
                "heading" => {
                    let (op, v) = (t.op, t.value.to_lowercase());
                    post.push(Box::new(move |r| {
                        let has = r
                            .item
                            .heading
                            .as_deref()
                            .map(|h| h.to_lowercase().contains(&v))
                            .unwrap_or(false);
                        has != (op == Op::Ne)
                    }));
                }
                "text" | "title" => {
                    let (op, v) = (t.op, t.value.to_lowercase());
                    post.push(Box::new(move |r| {
                        r.item.text.to_lowercase().contains(&v) != (op == Op::Ne)
                    }));
                }
                "depth" => {
                    let (op, v) = (t.op, t.value.clone());
                    post.push(Box::new(move |r| matches(op, Some(&r.item.depth.to_string()), &v)));
                }
                key => {
                    let key = key.to_string();
                    let wanted = if is_date_key(&key) {
                        resolve_date(&t.value)
                    } else {
                        t.value.clone()
                    };
                    let op = t.op;
                    post.push(Box::new(move |r| {
                        matches(op, r.item.fields.get(&key).map(String::as_str), &wanted)
                    }));
                }
            }
        }
        let text: Vec<String> = q.text.iter().map(|t| t.to_lowercase()).collect();
        let mut rows: Vec<ItemRow> = self
            .items(&filter)?
            .into_iter()
            .filter(|r| post.iter().all(|f| f(r)))
            .filter(|r| {
                let hay = format!("{} {}", r.item.text.to_lowercase(), r.note_title.to_lowercase());
                text.iter().all(|t| hay.contains(t))
            })
            .collect();

        if !q.sort.is_empty() {
            rows.sort_by(|a, b| {
                for (field, asc) in &q.sort {
                    let va = item_sort_value(a, field);
                    let vb = item_sort_value(b, field);
                    let ord = match (&va, &vb) {
                        (None, None) => std::cmp::Ordering::Equal,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (Some(x), Some(y)) => {
                            let o = compare_values(x, y);
                            if *asc {
                                o
                            } else {
                                o.reverse()
                            }
                        }
                    };
                    if ord != std::cmp::Ordering::Equal {
                        return ord;
                    }
                }
                std::cmp::Ordering::Equal
            });
        }
        let total = rows.len();
        if let Some(l) = q.limit {
            rows.truncate(l);
        }
        let groups = match &q.group {
            Some(field) => group_rows(&mut rows, |r| item_group_value(r, field)),
            None => Vec::new(),
        };
        Ok(QueryResult {
            domain: Domain::Items,
            notes: Vec::new(),
            items: rows,
            groups,
            total,
        })
    }

    /// Does `id` name a saved query note? Returns its query text.
    pub fn saved_query(&self, id: &str) -> Result<Option<String>> {
        let t: Option<String> = self
            .conn
            .query_row("SELECT type FROM notes WHERE id = ?1", params![id], |r| r.get(0))
            .optional()?;
        if t.as_deref() != Some("query") {
            return Ok(None);
        }
        self.property(id, "query")
    }
}

fn note_sort_value(r: &NoteRow, field: &str) -> Option<String> {
    match field {
        "title" | "name" => Some(r.note.title.to_lowercase()),
        "path" | "file" => Some(r.note.path.to_lowercase()),
        "type" => Some(r.note.note_type.clone()),
        "updated" | "modified" => r.note.updated.map(|u| u.to_rfc3339()),
        "created" => r.created.clone(),
        "tags" | "tag" => r.note.tags.first().cloned(),
        other => r.properties.get(other).and_then(json_text),
    }
}

fn note_group_value(r: &NoteRow, field: &str) -> String {
    match field {
        "type" => r.note.note_type.clone(),
        "folder" | "path" | "in" => r
            .note
            .path
            .rsplit_once('/')
            .map(|(f, _)| f.to_string())
            .unwrap_or_default(),
        "tag" | "tags" => r.note.tags.first().cloned().unwrap_or_default(),
        "updated" | "created" => note_sort_value(r, field)
            .map(|d| d[..10.min(d.len())].to_string())
            .unwrap_or_default(),
        other => r
            .properties
            .get(other)
            .and_then(json_text)
            .map(|v| link_target(&v))
            .unwrap_or_default(),
    }
}

fn item_sort_value(r: &ItemRow, field: &str) -> Option<String> {
    match field {
        "due" => r.item.due.clone(),
        "priority" => r.item.priority.map(|p| p.to_string()),
        "status" => Some(
            match r.item.status {
                ItemStatus::Doing => 0,
                ItemStatus::Open => 1,
                ItemStatus::Answered => 2,
                ItemStatus::Done => 3,
                ItemStatus::Cancelled => 4,
            }
            .to_string(),
        ),
        "text" | "title" => Some(r.item.text.to_lowercase()),
        "note" | "path" | "file" => Some(format!("{}:{:06}", r.note_path.to_lowercase(), r.item.line)),
        "line" => Some(format!("{:06}", r.item.line)),
        "project" => r.project_title.clone(),
        "heading" => r.item.heading.clone(),
        other => r.item.fields.get(other).cloned(),
    }
}

fn item_group_value(r: &ItemRow, field: &str) -> String {
    match field {
        "status" => r.item.status.as_str().to_string(),
        "kind" | "type" => r.item.kind.as_str().to_string(),
        "project" => r.project_title.clone().unwrap_or_default(),
        "note" | "path" | "file" => r.note_title.clone(),
        "heading" => r.item.heading.clone().unwrap_or_default(),
        "priority" => r.item.priority.map(|p| format!("p{p}")).unwrap_or_default(),
        "due" => r.item.due.clone().unwrap_or_default(),
        "tag" | "tags" => r.item.tags.first().cloned().unwrap_or_default(),
        other => r.item.fields.get(other).cloned().unwrap_or_default(),
    }
}

/// Stable-sort rows by their group value (empty last) and describe the groups.
fn group_rows<T>(rows: &mut [T], key: impl Fn(&T) -> String) -> Vec<Group> {
    rows.sort_by_cached_key(|r| {
        let k = key(r);
        (k.is_empty(), k.to_lowercase())
    });
    let mut groups: Vec<Group> = Vec::new();
    for r in rows.iter() {
        let k = key(r);
        match groups.last_mut() {
            Some(g) if g.key.eq_ignore_ascii_case(&k) => g.count += 1,
            _ => groups.push(Group { key: k, count: 1 }),
        }
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_terms_text_sort_group_and_domain() {
        let q = Query::parse(
            r#"type:task project:[[Garden Robot]] status:!done due:<today #p1 "exact words" motors sort:-due,priority group:project limit:20"#,
        );
        assert_eq!(q.domain, Domain::Items);
        assert_eq!(
            q.terms[0],
            Term {
                key: "type".into(),
                op: Op::Eq,
                value: "task".into()
            }
        );
        assert_eq!(
            q.terms[1],
            Term {
                key: "project".into(),
                op: Op::Eq,
                value: "Garden Robot".into()
            }
        );
        assert_eq!(
            q.terms[2],
            Term {
                key: "status".into(),
                op: Op::Ne,
                value: "done".into()
            }
        );
        assert_eq!(
            q.terms[3],
            Term {
                key: "due".into(),
                op: Op::Lt,
                value: "today".into()
            }
        );
        assert_eq!(
            q.terms[4],
            Term {
                key: "tag".into(),
                op: Op::Eq,
                value: "p1".into()
            }
        );
        assert_eq!(q.text, vec!["exact words", "motors"]);
        assert_eq!(q.sort, vec![("due".to_string(), false), ("priority".to_string(), true)]);
        assert_eq!(q.group.as_deref(), Some("project"));
        assert_eq!(q.limit, Some(20));

        let q = Query::parse("type:project status:* https://example.com/a:b");
        assert_eq!(q.domain, Domain::Notes);
        assert_eq!(q.terms[1].op, Op::Exists);
        assert_eq!(q.text, vec!["https://example.com/a:b"]);
    }

    #[test]
    fn dates_resolve_relative_to_today() {
        let today = Local::now().date_naive();
        assert_eq!(resolve_date("today"), today.format("%Y-%m-%d").to_string());
        assert_eq!(
            resolve_date("+1w"),
            (today + Duration::days(7)).format("%Y-%m-%d").to_string()
        );
        assert_eq!(
            resolve_date("-2d"),
            (today - Duration::days(2)).format("%Y-%m-%d").to_string()
        );
        assert_eq!(resolve_date("2026-11-01"), "2026-11-01");
    }

    #[test]
    fn values_compare_like_a_person_would() {
        assert!(matches(Op::Lt, Some("2026-10-01"), "2026-11-01"));
        assert!(matches(Op::Gt, Some("10"), "9"));
        assert!(matches(Op::Eq, Some("Active"), "active"));
        assert!(matches(Op::Ne, None, "x"));
        assert!(!matches(Op::Eq, None, "x"));
        assert!(matches(Op::Exists, Some("x"), ""));
        assert!(!matches(Op::Exists, Some(""), ""));
    }
}
