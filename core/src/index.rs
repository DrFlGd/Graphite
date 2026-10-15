//! SQLite index over the vault. Fully rebuildable from the files on disk.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::assets::{Asset, AssetHit, AssetKind, DerivedState};
use crate::error::Result;
use crate::links::{extract_links, normalise_target, LinkKind};
use crate::note::{Note, NoteSummary};
use crate::tasks::{parse_items, Item, ItemKind, ItemStatus};
use std::collections::HashMap;

const SCHEMA_VERSION: i64 = 5;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);

CREATE TABLE IF NOT EXISTS notes (
  id        TEXT PRIMARY KEY,
  path      TEXT NOT NULL UNIQUE,
  title     TEXT NOT NULL,
  type      TEXT NOT NULL,
  created   TEXT,
  updated   TEXT,
  mtime_ms  INTEGER NOT NULL,
  size      INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS notes_title ON notes(title COLLATE NOCASE);
CREATE INDEX IF NOT EXISTS notes_type ON notes(type);

CREATE TABLE IF NOT EXISTS tags (
  note_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
  tag     TEXT NOT NULL,
  PRIMARY KEY (note_id, tag)
);
CREATE INDEX IF NOT EXISTS tags_tag ON tags(tag);

CREATE TABLE IF NOT EXISTS aliases (
  note_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
  alias   TEXT NOT NULL,
  PRIMARY KEY (note_id, alias)
);
CREATE INDEX IF NOT EXISTS aliases_alias ON aliases(alias COLLATE NOCASE);

CREATE TABLE IF NOT EXISTS properties (
  note_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
  key     TEXT NOT NULL,
  value   TEXT NOT NULL,
  PRIMARY KEY (note_id, key)
);
CREATE INDEX IF NOT EXISTS properties_key ON properties(key);

CREATE TABLE IF NOT EXISTS links (
  from_id   TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
  target    TEXT NOT NULL,
  target_norm TEXT NOT NULL,
  heading   TEXT,
  relation  TEXT,
  kind      TEXT NOT NULL,
  position  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS links_from ON links(from_id);
CREATE INDEX IF NOT EXISTS links_target ON links(target_norm);

CREATE TABLE IF NOT EXISTS items (
  note_id   TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
  line      INTEGER NOT NULL,
  kind      TEXT NOT NULL,
  status    TEXT NOT NULL,
  text      TEXT NOT NULL,
  raw       TEXT NOT NULL,
  depth     INTEGER NOT NULL,
  parent    INTEGER,
  heading   TEXT,
  due       TEXT,
  priority  INTEGER,
  tags      TEXT NOT NULL,
  project   TEXT,
  project_norm TEXT,
  answer    TEXT,
  fields    TEXT NOT NULL,
  PRIMARY KEY (note_id, line)
);
CREATE INDEX IF NOT EXISTS items_kind_status ON items(kind, status);
CREATE INDEX IF NOT EXISTS items_project ON items(project_norm);

CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(
  id UNINDEXED, title, body, tags, tokenize = 'unicode61 remove_diacritics 2'
);

CREATE TABLE IF NOT EXISTS assets (
  path      TEXT PRIMARY KEY,
  name      TEXT NOT NULL,
  ext       TEXT NOT NULL,
  kind      TEXT NOT NULL,
  size      INTEGER NOT NULL,
  mtime_ms  INTEGER NOT NULL,
  modified  TEXT NOT NULL,
  hash      TEXT,
  width     INTEGER,
  height    INTEGER,
  duration  REAL,
  pages     INTEGER,
  meta      TEXT NOT NULL,
  text      TEXT,
  thumbnail TEXT,
  derived   TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS assets_kind ON assets(kind);
CREATE INDEX IF NOT EXISTS assets_derived ON assets(derived);
CREATE INDEX IF NOT EXISTS assets_name ON assets(name COLLATE NOCASE);

CREATE VIRTUAL TABLE IF NOT EXISTS assets_fts USING fts5(
  path UNINDEXED, name, body, tokenize = 'unicode61 remove_diacritics 2'
);

-- Sync state of this device (see sync.rs): the content hash and path each
-- note had when this device last reconciled it with its document.
CREATE TABLE IF NOT EXISTS sync_notes (
  note_id       TEXT PRIMARY KEY,
  hash          TEXT NOT NULL,
  path          TEXT NOT NULL,
  deleted       INTEGER NOT NULL DEFAULT 0,
  missing_since INTEGER,
  heads         TEXT NOT NULL DEFAULT '',
  dirty         TEXT,
  pending       TEXT,
  pending_since INTEGER,
  pending_text  TEXT
);
-- Which versions of other devices' documents this device has merged.
CREATE TABLE IF NOT EXISTS sync_peers (
  device   TEXT NOT NULL,
  file     TEXT NOT NULL,
  mtime_ms INTEGER NOT NULL,
  size     INTEGER NOT NULL,
  PRIMARY KEY (device, file)
);
"#;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchHit {
    pub id: String,
    pub path: String,
    pub title: String,
    /// Snippet with `[` `]` around matched terms.
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Backlink {
    pub from: NoteSummary,
    pub relation: Option<String>,
    pub kind: LinkKind,
    pub position: usize,
}

/// Which tasks/gaps to list.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ItemFilter {
    pub kind: Option<ItemKind>,
    /// Only `open` and `doing` items.
    #[serde(default)]
    pub open_only: bool,
    /// Items in one note (id).
    pub note: Option<String>,
    /// Items belonging to one project (note id), however they were attributed.
    pub project: Option<String>,
    /// Items due on or before this `YYYY-MM-DD`.
    pub due_before: Option<String>,
    /// Case-insensitive substring of the item text.
    pub text: Option<String>,
}

/// A task or gap with the note and project it belongs to.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ItemRow {
    #[serde(flatten)]
    pub item: Item,
    pub note_title: String,
    pub note_path: String,
    pub project_id: Option<String>,
    pub project_title: Option<String>,
}

/// What this device last reconciled for one note (see `sync.rs`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncState {
    pub hash: String,
    pub path: String,
    pub deleted: bool,
    pub missing_since: Option<i64>,
    /// Document heads (comma-separated) the file was last reconciled at.
    pub heads: String,
    /// Content hash of what Graphite itself last wrote to the file, if
    /// that was after the last pass (so a file with this hash is a local
    /// edit, not one the sync client delivered).
    pub dirty: Option<String>,
    /// An unexplained content hash first seen at `pending_since` (ms);
    /// absorbed once it has sat still long enough.
    pub pending: Option<String>,
    pub pending_since: Option<i64>,
    /// The file as it was when `pending` was recorded.
    pub pending_text: Option<String>,
}

pub struct Index {
    pub(crate) conn: Connection,
}

impl Index {
    pub fn open(path: &Path) -> Result<Index> {
        let conn = Connection::open(path)?;
        // The index is a rebuildable cache, so WAL + NORMAL (no fsync per
        // commit) is the right trade: a power cut can only lose the last few
        // commits, which the next sync redoes.
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA foreign_keys = ON;")?;
        let idx = Index { conn };
        idx.migrate()?;
        Ok(idx)
    }

    pub fn open_in_memory() -> Result<Index> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        let idx = Index { conn };
        idx.migrate()?;
        Ok(idx)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(SCHEMA)?;
        let version: Option<i64> = self
            .conn
            .query_row("SELECT value FROM meta WHERE key = 'schema_version'", [], |r| {
                r.get::<_, String>(0)
            })
            .optional()?
            .and_then(|s| s.parse().ok());
        if version != Some(SCHEMA_VERSION) {
            // Any schema change is a rebuild: the index is a cache.
            self.clear()?;
            self.conn.execute(
                "INSERT OR REPLACE INTO meta(key, value) VALUES ('schema_version', ?1)",
                params![SCHEMA_VERSION.to_string()],
            )?;
        }
        Ok(())
    }

    pub fn clear(&self) -> Result<()> {
        self.conn.execute_batch(
            "DELETE FROM items; DELETE FROM links; DELETE FROM properties; DELETE FROM aliases; DELETE FROM tags; DELETE FROM notes; DELETE FROM notes_fts; DELETE FROM assets; DELETE FROM assets_fts; DELETE FROM sync_notes; DELETE FROM sync_peers;",
        )?;
        Ok(())
    }

    /// Insert or replace a note. `mtime_ms`/`size` let the vault skip
    /// unchanged files on rescan.
    /// Group many `upsert`/`remove_by_path` calls into one transaction
    /// (a full sync of thousands of notes is ~20x faster that way). Pair with
    /// `end_batch`; an error in between should go through `abort_batch`.
    pub fn begin_batch(&mut self) -> Result<()> {
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        Ok(())
    }

    pub fn end_batch(&mut self) -> Result<()> {
        self.conn.execute_batch("COMMIT")?;
        Ok(())
    }

    pub fn abort_batch(&mut self) {
        let _ = self.conn.execute_batch("ROLLBACK");
    }

    pub fn upsert(&mut self, note: &Note, mtime_ms: i64, size: i64) -> Result<()> {
        // A savepoint works on its own and inside a batch transaction alike.
        let tx = self.conn.savepoint()?;
        // FTS rows share the note's rowid: deleting by rowid is indexed, while
        // deleting by the UNINDEXED id column would scan the whole table.
        tx.prepare_cached("DELETE FROM notes_fts WHERE rowid IN (SELECT rowid FROM notes WHERE id = ?1 OR path = ?2)")?
            .execute(params![note.id, note.path])?;
        // A note moved on disk keeps its id; a file whose id collides with another path wins by path.
        tx.prepare_cached("DELETE FROM notes WHERE id = ?1 OR path = ?2")?
            .execute(params![note.id, note.path])?;
        tx.prepare_cached(
            "INSERT INTO notes(id, path, title, type, created, updated, mtime_ms, size) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        )?
        .execute(params![
            note.id,
            note.path,
            note.title,
            note.note_type,
            note.created.map(|d| d.to_rfc3339()),
            note.updated.map(|d| d.to_rfc3339()),
            mtime_ms,
            size
        ])?;
        let rowid = tx.last_insert_rowid();
        for t in &note.tags {
            tx.prepare_cached("INSERT OR IGNORE INTO tags(note_id, tag) VALUES (?1, ?2)")?
                .execute(params![note.id, t])?;
        }
        for a in &note.aliases {
            tx.prepare_cached("INSERT OR IGNORE INTO aliases(note_id, alias) VALUES (?1, ?2)")?
                .execute(params![note.id, a])?;
        }
        for (k, v) in &note.properties {
            tx.prepare_cached("INSERT OR REPLACE INTO properties(note_id, key, value) VALUES (?1, ?2, ?3)")?
                .execute(params![note.id, k, v.to_string()])?;
        }
        for l in extract_links(&note.body) {
            tx.prepare_cached(
                "INSERT INTO links(from_id, target, target_norm, heading, relation, kind, position) VALUES (?1,?2,?3,?4,?5,?6,?7)",
            )?
            .execute(params![
                note.id,
                l.target,
                match l.kind {
                    // File links match by whole path; note links by title/stem.
                    LinkKind::File => l.target.to_lowercase(),
                    _ => normalise_target(&l.target),
                },
                l.heading,
                l.relation,
                l.kind.as_str(),
                l.position as i64
            ])?;
        }
        for it in parse_items(&note.id, &note.body) {
            tx.prepare_cached(
                "INSERT INTO items(note_id, line, kind, status, text, raw, depth, parent, heading, due, priority, tags, project, project_norm, answer, fields)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
            )?
            .execute(params![
                it.note_id,
                it.line as i64,
                it.kind.as_str(),
                it.status.as_str(),
                it.text,
                it.raw,
                it.depth as i64,
                it.parent.map(|p| p as i64),
                it.heading,
                it.due,
                it.priority.map(|p| p as i64),
                it.tags.join(" "),
                it.project,
                it.project.as_deref().map(normalise_target),
                it.answer,
                serde_json::to_string(&it.fields).unwrap_or_else(|_| "{}".into()),
            ])?;
        }
        tx.prepare_cached("INSERT INTO notes_fts(rowid, id, title, body, tags) VALUES (?1, ?2, ?3, ?4, ?5)")?
            .execute(params![rowid, note.id, note.title, note.body, note.tags.join(" ")])?;
        tx.commit()?;
        Ok(())
    }

    pub fn remove_by_path(&self, path: &str) -> Result<Option<String>> {
        let id: Option<String> = self
            .conn
            .query_row("SELECT id FROM notes WHERE path = ?1", params![path], |r| r.get(0))
            .optional()?;
        if let Some(id) = &id {
            self.conn.execute(
                "DELETE FROM notes_fts WHERE rowid IN (SELECT rowid FROM notes WHERE id = ?1)",
                params![id],
            )?;
            self.conn.execute("DELETE FROM notes WHERE id = ?1", params![id])?;
        }
        Ok(id)
    }

    /// `(mtime_ms, size)` recorded for a path, if indexed.
    pub fn stat(&self, path: &str) -> Result<Option<(i64, i64)>> {
        Ok(self
            .conn
            .query_row("SELECT mtime_ms, size FROM notes WHERE path = ?1", params![path], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()?)
    }

    pub fn all_paths(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT path FROM notes")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn list(&self) -> Result<Vec<NoteSummary>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, path, title, type, updated FROM notes ORDER BY updated DESC, title COLLATE NOCASE")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<String>>(4)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, path, title, note_type, updated) = row?;
            let tags = self.tags_of(&id)?;
            out.push(NoteSummary {
                id,
                path,
                title,
                note_type,
                tags,
                updated: updated.and_then(|u| u.parse().ok()),
            });
        }
        Ok(out)
    }

    fn tags_of(&self, id: &str) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT tag FROM tags WHERE note_id = ?1 ORDER BY tag")?;
        let rows = stmt.query_map(params![id], |r| r.get(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn summary(&self, id: &str) -> Result<Option<NoteSummary>> {
        let row = self
            .conn
            .query_row(
                "SELECT id, path, title, type, updated FROM notes WHERE id = ?1",
                params![id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, Option<String>>(4)?,
                    ))
                },
            )
            .optional()?;
        match row {
            Some((id, path, title, note_type, updated)) => {
                let tags = self.tags_of(&id)?;
                Ok(Some(NoteSummary {
                    id,
                    path,
                    title,
                    note_type,
                    tags,
                    updated: updated.and_then(|u| u.parse().ok()),
                }))
            }
            None => Ok(None),
        }
    }

    pub fn path_of(&self, id: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT path FROM notes WHERE id = ?1", params![id], |r| r.get(0))
            .optional()?)
    }

    /// Resolve a link target to a note id: exact title, then alias, then
    /// file stem of the path (all case-insensitive).
    pub fn resolve(&self, target: &str) -> Result<Option<String>> {
        let norm = normalise_target(target);
        let full = target.trim().trim_end_matches(".md").to_lowercase();
        let by_title: Option<String> = self
            .conn
            .query_row(
                "SELECT id FROM notes WHERE lower(title) = ?1 ORDER BY path LIMIT 1",
                params![norm],
                |r| r.get(0),
            )
            .optional()?;
        if by_title.is_some() {
            return Ok(by_title);
        }
        let by_alias: Option<String> = self
            .conn
            .query_row(
                "SELECT note_id FROM aliases WHERE lower(alias) = ?1 ORDER BY note_id LIMIT 1",
                params![norm],
                |r| r.get(0),
            )
            .optional()?;
        if by_alias.is_some() {
            return Ok(by_alias);
        }
        let by_path: Option<String> = self
            .conn
            .query_row(
                "SELECT id FROM notes WHERE lower(path) = ?1 || '.md' OR lower(path) LIKE '%/' || ?2 || '.md' OR lower(path) = ?2 || '.md' ORDER BY path LIMIT 1",
                params![full, norm],
                |r| r.get(0),
            )
            .optional()?;
        Ok(by_path)
    }

    /// Notes that link to `id` (by any of its title, aliases or path).
    pub fn backlinks(&self, id: &str) -> Result<Vec<Backlink>> {
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

        let placeholders = keys.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT from_id, relation, kind, position FROM links WHERE kind != 'file' AND target_norm IN ({placeholders}) AND from_id != ? ORDER BY from_id, position"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let mut params_vec: Vec<&dyn rusqlite::ToSql> = keys.iter().map(|k| k as &dyn rusqlite::ToSql).collect();
        params_vec.push(&id);
        let rows = stmt.query_map(params_vec.as_slice(), |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (from_id, relation, kind, position) = row?;
            if let Some(from) = self.summary(&from_id)? {
                out.push(Backlink {
                    from,
                    relation,
                    kind: LinkKind::parse(&kind),
                    position: position as usize,
                });
            }
        }
        Ok(out)
    }

    /// Outgoing links of a note, resolved where possible: `(target, resolved id)`.
    pub fn outgoing(&self, id: &str) -> Result<Vec<(String, Option<String>)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT target FROM links WHERE from_id = ?1 AND kind != 'file' ORDER BY position")?;
        let targets: Vec<String> = stmt
            .query_map(params![id], |r| r.get(0))?
            .collect::<std::result::Result<_, _>>()?;
        let mut out = Vec::new();
        for t in targets {
            let resolved = self.resolve(&t)?;
            out.push((t, resolved));
        }
        Ok(out)
    }

    /// Notes whose text mentions this note's title but do not link to it.
    pub fn unlinked_mentions(&self, id: &str) -> Result<Vec<NoteSummary>> {
        let Some(me) = self.summary(id)? else {
            return Ok(Vec::new());
        };
        if me.title.trim().is_empty() {
            return Ok(Vec::new());
        }
        let phrase = format!("\"{}\"", me.title.replace('"', "\"\""));
        let linked: Vec<String> = self.backlinks(id)?.into_iter().map(|b| b.from.id).collect();
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM notes_fts WHERE notes_fts MATCH ?1 AND id != ?2")?;
        let ids: Vec<String> = stmt
            .query_map(params![phrase, id], |r| r.get(0))?
            .collect::<std::result::Result<_, _>>()?;
        let mut out = Vec::new();
        for cand in ids {
            if linked.contains(&cand) {
                continue;
            }
            if let Some(s) = self.summary(&cand)? {
                out.push(s);
            }
        }
        Ok(out)
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
        let q = query.trim();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        // Turn free text into a prefix-match FTS query; quote each term so
        // user punctuation can't break the FTS grammar.
        let fts: Vec<String> = q
            .split_whitespace()
            .map(|t| format!("\"{}\"*", t.replace('"', "\"\"")))
            .collect();
        let fts = fts.join(" ");
        let mut stmt = self.conn.prepare(
            "SELECT n.id, n.path, n.title, snippet(notes_fts, 2, '[', ']', '…', 12)
             FROM notes_fts JOIN notes n ON n.id = notes_fts.id
             WHERE notes_fts MATCH ?1
             ORDER BY bm25(notes_fts, 0.0, 5.0, 1.0, 2.0) LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![fts, limit as i64], |r| {
            Ok(SearchHit {
                id: r.get(0)?,
                path: r.get(1)?,
                title: r.get(2)?,
                snippet: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Titles and aliases matching a prefix, for `[[` autocomplete.
    pub fn complete_title(&self, prefix: &str, limit: usize) -> Result<Vec<NoteSummary>> {
        let pat = format!("{}%", prefix.trim().to_lowercase().replace('%', ""));
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT n.id FROM notes n LEFT JOIN aliases a ON a.note_id = n.id
             WHERE lower(n.title) LIKE ?1 OR lower(a.alias) LIKE ?1
             ORDER BY n.title COLLATE NOCASE LIMIT ?2",
        )?;
        let ids: Vec<String> = stmt
            .query_map(params![pat, limit as i64], |r| r.get(0))?
            .collect::<std::result::Result<_, _>>()?;
        let mut out = Vec::new();
        for id in ids {
            if let Some(s) = self.summary(&id)? {
                out.push(s);
            }
        }
        Ok(out)
    }

    /// One front-matter property of a note as plain text: a string value
    /// without its JSON quotes, anything else as its JSON form.
    pub fn property(&self, note_id: &str, key: &str) -> Result<Option<String>> {
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM properties WHERE note_id = ?1 AND key = ?2",
                params![note_id, key],
                |r| r.get(0),
            )
            .optional()?;
        Ok(raw.map(|v| match serde_json::from_str::<serde_json::Value>(&v) {
            Ok(serde_json::Value::String(s)) => s,
            _ => v,
        }))
    }

    /// Tasks and gaps matching `f`, with the project each one belongs to
    /// resolved: the item's own `project::` field, else the note's `project`
    /// property, else the note itself when it is a project.
    pub fn items(&self, f: &ItemFilter) -> Result<Vec<ItemRow>> {
        let mut sql = String::from(
            "SELECT i.note_id, i.line, i.kind, i.status, i.text, i.raw, i.depth, i.parent, i.heading, i.due, i.priority, i.tags, i.project, i.answer, i.fields, n.title, n.path, n.type
             FROM items i JOIN notes n ON n.id = i.note_id WHERE 1=1",
        );
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if let Some(k) = f.kind {
            sql.push_str(" AND i.kind = ?");
            args.push(Box::new(k.as_str().to_string()));
        }
        if f.open_only {
            sql.push_str(" AND i.status IN ('open', 'doing')");
        }
        if let Some(n) = &f.note {
            sql.push_str(" AND i.note_id = ?");
            args.push(Box::new(n.clone()));
        }
        if let Some(d) = &f.due_before {
            sql.push_str(" AND i.due IS NOT NULL AND i.due <= ?");
            args.push(Box::new(d.clone()));
        }
        if let Some(t) = &f.text {
            sql.push_str(" AND lower(i.text) LIKE ?");
            args.push(Box::new(format!("%{}%", t.to_lowercase().replace('%', ""))));
        }
        sql.push_str(" ORDER BY n.path, i.line");
        let mut stmt = self.conn.prepare(&sql)?;
        let params_vec: Vec<&dyn rusqlite::ToSql> = args.iter().map(|a| a.as_ref()).collect();
        let rows = stmt.query_map(params_vec.as_slice(), |r| {
            Ok((
                Item {
                    note_id: r.get(0)?,
                    line: r.get::<_, i64>(1)? as usize,
                    kind: ItemKind::parse(&r.get::<_, String>(2)?).unwrap_or(ItemKind::Task),
                    status: ItemStatus::parse(&r.get::<_, String>(3)?).unwrap_or(ItemStatus::Open),
                    text: r.get(4)?,
                    raw: r.get(5)?,
                    depth: r.get::<_, i64>(6)? as usize,
                    parent: r.get::<_, Option<i64>>(7)?.map(|p| p as usize),
                    heading: r.get(8)?,
                    due: r.get(9)?,
                    priority: r.get::<_, Option<i64>>(10)?.map(|p| p as u8),
                    tags: r.get::<_, String>(11)?.split_whitespace().map(str::to_string).collect(),
                    project: r.get(12)?,
                    answer: r.get(13)?,
                    fields: serde_json::from_str(&r.get::<_, String>(14)?).unwrap_or_default(),
                },
                r.get::<_, String>(15)?,
                r.get::<_, String>(16)?,
                r.get::<_, String>(17)?,
            ))
        })?;
        let mut out = Vec::new();
        let mut resolved: std::collections::HashMap<String, Option<String>> = Default::default();
        let mut note_projects: std::collections::HashMap<String, Option<String>> = Default::default();
        for row in rows {
            let (item, note_title, note_path, note_type) = row?;
            let project_id = match &item.project {
                Some(target) => {
                    let key = normalise_target(target);
                    match resolved.get(&key) {
                        Some(v) => v.clone(),
                        None => {
                            let v = self.resolve(target)?;
                            resolved.insert(key, v.clone());
                            v
                        }
                    }
                }
                None => match note_projects.get(&item.note_id) {
                    Some(v) => v.clone(),
                    None => {
                        let v = if note_type == "project" {
                            Some(item.note_id.clone())
                        } else {
                            match self.property(&item.note_id, "project")? {
                                Some(p) => self.resolve(&crate::tasks::link_target(&p))?,
                                None => None,
                            }
                        };
                        note_projects.insert(item.note_id.clone(), v.clone());
                        v
                    }
                },
            };
            if let Some(want) = &f.project {
                if project_id.as_deref() != Some(want.as_str()) {
                    continue;
                }
            }
            let project_title = match &project_id {
                Some(pid) => self
                    .conn
                    .query_row("SELECT title FROM notes WHERE id = ?1", params![pid], |r| r.get(0))
                    .optional()?,
                None => None,
            };
            out.push(ItemRow {
                item,
                note_title,
                note_path,
                project_id,
                project_title,
            });
        }
        Ok(out)
    }

    // ----- assets -----

    /// Insert or replace an asset row. `text` is the extracted text to
    /// make searchable (the vault reads the derived file before calling).
    pub fn upsert_asset(&mut self, a: &Asset, text: Option<&str>) -> Result<()> {
        let tx = self.conn.savepoint()?;
        tx.prepare_cached("DELETE FROM assets_fts WHERE rowid IN (SELECT rowid FROM assets WHERE path = ?1)")?
            .execute(params![a.path])?;
        tx.prepare_cached("DELETE FROM assets WHERE path = ?1")?
            .execute(params![a.path])?;
        tx.prepare_cached(
            "INSERT INTO assets(path, name, ext, kind, size, mtime_ms, modified, hash, width, height, duration, pages, meta, text, thumbnail, derived)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
        )?
        .execute(params![
            a.path,
            a.name,
            a.ext,
            a.kind.as_str(),
            a.size,
            a.modified.timestamp_millis(),
            a.modified.to_rfc3339(),
            a.hash,
            a.width,
            a.height,
            a.duration,
            a.pages,
            serde_json::Value::Object(a.meta.clone()).to_string(),
            a.text,
            a.thumbnail,
            match a.derived {
                DerivedState::Pending => "pending",
                DerivedState::Done => "done",
            },
        ])?;
        let rowid = tx.last_insert_rowid();
        let body = text.unwrap_or_default();
        tx.prepare_cached("INSERT INTO assets_fts(rowid, path, name, body) VALUES (?1, ?2, ?3, ?4)")?
            .execute(params![rowid, a.path, a.name.replace(['_', '-', '.'], " "), body])?;
        tx.commit()?;
        Ok(())
    }

    pub fn remove_asset(&self, path: &str) -> Result<bool> {
        self.conn.execute(
            "DELETE FROM assets_fts WHERE rowid IN (SELECT rowid FROM assets WHERE path = ?1)",
            params![path],
        )?;
        Ok(self.conn.execute("DELETE FROM assets WHERE path = ?1", params![path])? > 0)
    }

    /// `(mtime_ms, size)` recorded for an asset path, if indexed.
    pub fn asset_stat(&self, path: &str) -> Result<Option<(i64, i64)>> {
        Ok(self
            .conn
            .query_row(
                "SELECT mtime_ms, size FROM assets WHERE path = ?1",
                params![path],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
    }

    pub fn all_asset_paths(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT path FROM assets")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    const ASSET_COLS: &'static str =
        "path, name, ext, kind, size, modified, hash, width, height, duration, pages, meta, text, thumbnail, derived";

    fn row_asset(r: &rusqlite::Row<'_>) -> rusqlite::Result<Asset> {
        let meta: String = r.get(11)?;
        let derived: String = r.get(14)?;
        let modified: String = r.get(5)?;
        Ok(Asset {
            path: r.get(0)?,
            name: r.get(1)?,
            ext: r.get(2)?,
            kind: AssetKind::parse(&r.get::<_, String>(3)?),
            size: r.get(4)?,
            modified: chrono::DateTime::parse_from_rfc3339(&modified)
                .map(|d| d.with_timezone(&chrono::Utc))
                .unwrap_or_default(),
            hash: r.get(6)?,
            width: r.get(7)?,
            height: r.get(8)?,
            duration: r.get(9)?,
            pages: r.get(10)?,
            meta: serde_json::from_str::<serde_json::Value>(&meta)
                .ok()
                .and_then(|v| v.as_object().cloned())
                .unwrap_or_default(),
            text: r.get(12)?,
            thumbnail: r.get(13)?,
            derived: if derived == "done" {
                DerivedState::Done
            } else {
                DerivedState::Pending
            },
        })
    }

    pub fn asset(&self, path: &str) -> Result<Option<Asset>> {
        let sql = format!("SELECT {} FROM assets WHERE path = ?1", Self::ASSET_COLS);
        Ok(self.conn.query_row(&sql, params![path], Self::row_asset).optional()?)
    }

    /// Assets newest first, optionally by kind and name/path substring.
    pub fn assets(&self, kind: Option<AssetKind>, filter: Option<&str>) -> Result<Vec<Asset>> {
        let mut sql = format!("SELECT {} FROM assets WHERE 1=1", Self::ASSET_COLS);
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if let Some(k) = kind {
            sql.push_str(" AND kind = ?");
            args.push(Box::new(k.as_str().to_string()));
        }
        if let Some(f) = filter.map(str::trim).filter(|f| !f.is_empty()) {
            sql.push_str(" AND path LIKE ? ESCAPE '\\'");
            let esc = f.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
            args.push(Box::new(format!("%{esc}%")));
        }
        sql.push_str(" ORDER BY mtime_ms DESC, path");
        let mut stmt = self.conn.prepare(&sql)?;
        let refs: Vec<&dyn rusqlite::ToSql> = args.iter().map(|a| a.as_ref()).collect();
        let rows = stmt.query_map(refs.as_slice(), Self::row_asset)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn pending_assets(&self, limit: usize) -> Result<Vec<Asset>> {
        let sql = format!(
            "SELECT {} FROM assets WHERE derived = 'pending' ORDER BY size ASC LIMIT ?1",
            Self::ASSET_COLS
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![limit as i64], Self::row_asset)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn asset_counts(&self) -> Result<HashMap<AssetKind, usize>> {
        let mut stmt = self.conn.prepare("SELECT kind, COUNT(*) FROM assets GROUP BY kind")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        let mut out = HashMap::new();
        for row in rows {
            let (k, n) = row?;
            out.insert(AssetKind::parse(&k), n as usize);
        }
        Ok(out)
    }

    /// Notes linking to a file: `[text](path)`, `![alt](path)` or `![[name]]`.
    pub fn asset_references(&self, path: &str) -> Result<Vec<Backlink>> {
        let path_norm = path.trim().to_lowercase();
        let name_norm = normalise_target(path);
        let mut stmt = self.conn.prepare(
            "SELECT from_id, relation, kind, position FROM links
             WHERE (kind = 'file' AND target_norm = ?1) OR (kind = 'embed' AND target_norm = ?2)
             ORDER BY from_id, position",
        )?;
        let rows = stmt.query_map(params![path_norm, name_norm], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (from_id, relation, kind, position) = row?;
            if let Some(from) = self.summary(&from_id)? {
                out.push(Backlink {
                    from,
                    relation,
                    kind: LinkKind::parse(&kind),
                    position: position as usize,
                });
            }
        }
        Ok(out)
    }

    pub fn search_assets(&self, query: &str, limit: usize) -> Result<Vec<AssetHit>> {
        let q = query.trim();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let fts: Vec<String> = q
            .split_whitespace()
            .map(|t| format!("\"{}\"*", t.replace('"', "\"\"")))
            .collect();
        let fts = fts.join(" ");
        let mut stmt = self.conn.prepare(
            "SELECT a.path, a.name, a.kind, snippet(assets_fts, 2, '[', ']', '…', 12)
             FROM assets_fts JOIN assets a ON a.path = assets_fts.path
             WHERE assets_fts MATCH ?1
             ORDER BY bm25(assets_fts, 0.0, 5.0, 1.0) LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![fts, limit as i64], |r| {
            Ok(AssetHit {
                path: r.get(0)?,
                name: r.get(1)?,
                kind: AssetKind::parse(&r.get::<_, String>(2)?),
                snippet: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn asset_count(&self) -> Result<usize> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM assets", [], |r| r.get::<_, i64>(0))? as usize)
    }

    // ----- sync state -------------------------------------------------

    pub fn sync_state(&self, note_id: &str) -> Result<Option<SyncState>> {
        Ok(self
            .conn
            .query_row(
                "SELECT hash, path, deleted, missing_since, heads, dirty, pending, pending_since, pending_text FROM sync_notes WHERE note_id = ?1",
                params![note_id],
                |r| {
                    Ok(SyncState {
                        hash: r.get(0)?,
                        path: r.get(1)?,
                        deleted: r.get::<_, i64>(2)? != 0,
                        missing_since: r.get(3)?,
                        heads: r.get(4)?,
                        dirty: r.get(5)?,
                        pending: r.get(6)?,
                        pending_since: r.get(7)?,
                        pending_text: r.get(8)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn sync_states(&self) -> Result<Vec<(String, SyncState)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT note_id, hash, path, deleted, missing_since, heads, dirty, pending, pending_since, pending_text FROM sync_notes ORDER BY note_id")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get(0)?,
                SyncState {
                    hash: r.get(1)?,
                    path: r.get(2)?,
                    deleted: r.get::<_, i64>(3)? != 0,
                    missing_since: r.get(4)?,
                    heads: r.get(5)?,
                    dirty: r.get(6)?,
                    pending: r.get(7)?,
                    pending_since: r.get(8)?,
                    pending_text: r.get(9)?,
                },
            ))
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Record that `note_id` on disk at `path` has content hash `hash` and
    /// matches this device's document. Clears any deletion mark.
    pub fn set_sync_state(&self, note_id: &str, hash: &str, path: &str, heads: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO sync_notes(note_id, hash, path, deleted, missing_since, heads, dirty, pending) VALUES (?1, ?2, ?3, 0, NULL, ?4, NULL, NULL)
             ON CONFLICT(note_id) DO UPDATE SET hash = excluded.hash, path = excluded.path, deleted = 0, missing_since = NULL, heads = excluded.heads, dirty = NULL, pending = NULL, pending_since = NULL, pending_text = NULL",
            params![note_id, hash, path, heads],
        )?;
        Ok(())
    }

    /// Mark a note as deliberately deleted on this device, so the next
    /// sync publishes a tombstone instead of waiting out the grace period.
    pub fn mark_sync_deleted(&self, note_id: &str) -> Result<()> {
        self.conn
            .execute("UPDATE sync_notes SET deleted = 1 WHERE note_id = ?1", params![note_id])?;
        Ok(())
    }

    /// Note that Graphite itself just wrote `note_id` with content `hash`.
    pub fn mark_sync_dirty(&self, note_id: &str, hash: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO sync_notes(note_id, hash, path, dirty) VALUES (?1, '', '', ?2)
             ON CONFLICT(note_id) DO UPDATE SET dirty = excluded.dirty",
            params![note_id, hash],
        )?;
        Ok(())
    }

    /// Note an unexplained content hash (creating the row if needed).
    pub fn set_sync_pending(&self, note_id: &str, pending: &str, since: i64, text: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO sync_notes(note_id, hash, path, pending, pending_since, pending_text) VALUES (?1, '', '', ?2, ?3, ?4)
             ON CONFLICT(note_id) DO UPDATE SET pending = excluded.pending, pending_since = excluded.pending_since, pending_text = excluded.pending_text",
            params![note_id, pending, since, text],
        )?;
        Ok(())
    }

    pub fn set_sync_missing_since(&self, note_id: &str, since: Option<i64>) -> Result<()> {
        self.conn.execute(
            "UPDATE sync_notes SET missing_since = ?2 WHERE note_id = ?1",
            params![note_id, since],
        )?;
        Ok(())
    }

    pub fn remove_sync_state(&self, note_id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM sync_notes WHERE note_id = ?1", params![note_id])?;
        Ok(())
    }

    pub fn peer_stat(&self, device: &str, file: &str) -> Result<Option<(i64, i64)>> {
        Ok(self
            .conn
            .query_row(
                "SELECT mtime_ms, size FROM sync_peers WHERE device = ?1 AND file = ?2",
                params![device, file],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
    }

    pub fn set_peer_stat(&self, device: &str, file: &str, mtime_ms: i64, size: i64) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO sync_peers(device, file, mtime_ms, size) VALUES (?1, ?2, ?3, ?4)",
            params![device, file, mtime_ms, size],
        )?;
        Ok(())
    }

    pub fn count(&self) -> Result<usize> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM notes", [], |r| r.get::<_, i64>(0))? as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(id: &str, path: &str, body: &str) -> Note {
        let mut n = Note::parse(path, body).unwrap();
        n.id = id.into();
        n
    }

    #[test]
    fn links_backlinks_and_resolution() {
        let mut idx = Index::open_in_memory().unwrap();
        idx.upsert(
            &note("A", "Projects/Garden Robot.md", "---\naliases: [GR]\n---\nhub"),
            0,
            0,
        )
        .unwrap();
        idx.upsert(
            &note("B", "Notes/Motors.md", "depends-on:: [[Garden Robot]]\nsee [[gr]] too"),
            0,
            0,
        )
        .unwrap();
        idx.upsert(
            &note("C", "Notes/Other.md", "mentions Garden Robot without a link"),
            0,
            0,
        )
        .unwrap();

        assert_eq!(idx.resolve("garden robot").unwrap().as_deref(), Some("A"));
        assert_eq!(idx.resolve("GR").unwrap().as_deref(), Some("A"));
        assert_eq!(idx.resolve("Projects/Garden Robot").unwrap().as_deref(), Some("A"));
        assert_eq!(idx.resolve("nope").unwrap(), None);

        let bl = idx.backlinks("A").unwrap();
        assert_eq!(bl.len(), 2);
        assert_eq!(bl[0].from.id, "B");
        assert_eq!(bl[0].relation.as_deref(), Some("depends-on"));

        let um = idx.unlinked_mentions("A").unwrap();
        assert_eq!(um.len(), 1);
        assert_eq!(um[0].id, "C");
    }

    #[test]
    fn search_ranks_title_matches_and_snippets() {
        let mut idx = Index::open_in_memory().unwrap();
        idx.upsert(&note("A", "Solar still.md", "Distils water with sunlight."), 0, 0)
            .unwrap();
        idx.upsert(
            &note(
                "B",
                "Notes.md",
                "A solar panel powers the pump. Lots of other words here.",
            ),
            0,
            0,
        )
        .unwrap();
        let hits = idx.search("solar", 10).unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].id, "A");
        assert!(hits[1].snippet.contains("[solar]"));
        assert!(idx.search("\"unbalanced", 10).unwrap().is_empty());
    }

    #[test]
    fn upsert_replaces_moved_note() {
        let mut idx = Index::open_in_memory().unwrap();
        idx.upsert(&note("A", "a.md", "x"), 0, 0).unwrap();
        idx.upsert(&note("A", "folder/a.md", "x"), 0, 0).unwrap();
        assert_eq!(idx.count().unwrap(), 1);
        assert_eq!(idx.path_of("A").unwrap().as_deref(), Some("folder/a.md"));
        assert_eq!(idx.remove_by_path("folder/a.md").unwrap().as_deref(), Some("A"));
        assert_eq!(idx.count().unwrap(), 0);
    }
}
