# Vault format (v1)

A Graphite vault is a folder. This document is the contract between everything that reads or writes one: the Rust core, the apps, the CLI, sync, and any other tool a user points at the folder. ADR-002: the files are the source of truth; everything under `.graphite/` can be deleted and rebuilt.

## Layout

```
MyVault/
├── notes/                 default folder for new notes (any folder works)
│   ├── Inbox/             quick captures land here
│   ├── Daily/             daily notes (M1, optional)
│   └── …/*.md
├── attachments/           files; sub-folders by project are a convention, not a rule
│   └── sources/           imported content (M3): repos, page archives, video
├── canvases/              whiteboards (M5), *.canvas.json
└── .graphite/             Graphite's own state (never synced, never hand-edited)
    ├── index.sqlite       full-text index, links, properties (rebuildable)
    ├── crdt/              per-note CRDT docs and sync log (M4)
    ├── derived/           thumbnails, transcripts, OCR text, converted previews
    └── config.json        vault settings
```

Rules:

- Every `*.md` file anywhere under the root (except under `.graphite/`) is a note. Folders are for humans; they carry no semantics beyond the default `notes/` for new notes and `notes/Inbox/` for captures.
- Every other file is an asset if it is referenced from a note or lives under `attachments/`. Assets are never modified by Graphite.
- Hidden files and folders (dot-prefixed) are ignored.

## Notes

A note is UTF-8 Markdown (CommonMark + GFM tables, task lists, strikethrough) with an optional YAML front-matter block delimited by `---` lines at the very top of the file.

```markdown
---
id: 01JA7Q2Z8K3M4N5P6Q7R8S9T0V
type: project
tags: [garden, robot]
aliases: [GR]
created: 2026-10-14T21:00:00Z
updated: 2026-10-14T21:30:00Z
status: active
---
# Garden Robot

Needs a [[Motor driver]]. depends-on:: [[Battery]]
```

### Reserved front-matter keys

| Key | Type | Meaning |
| --- | --- | --- |
| `id` | string (ULID) | Stable identity. Minted by Graphite on first index if missing; this is the only change Graphite makes to a file it did not write. Links resolve by title, so renames do not break them, but the id is what history, sync and external tools use. |
| `type` | string | `note` (default), `project`, `idea`, `area`, `task`, `gap`, `source`, `asset`, `decision`, `person`, `capture`, `query`, or user-defined. |
| `title` | string | Display title. Defaults to the file name without `.md`. Only written when it differs from the file name. |
| `tags` | list of strings | Hierarchy with `/` (`hardware/motors`). |
| `aliases` | list of strings | Alternative titles that links may use. |
| `created`, `updated` | RFC 3339 timestamp | Set by Graphite on create/save. A bare `YYYY-MM-DD` is accepted on read. |

Every other key is a **property** and is preserved verbatim (strings, numbers, booleans, lists, maps). Properties are indexed for queries (M2).

A file with no front-matter is a valid note (title = file name, type = note); it gains an `id` block the first time Graphite indexes it.

### Links

| Syntax | Meaning |
| --- | --- |
| `[[Title]]` | Link to the note whose title, alias, or file stem matches (case-insensitive). Title match wins, then alias, then path. |
| `[[Title#Heading]]` | Link to a heading inside the note. |
| `[[Title\|Display text]]` | Link shown with different text. |
| `[[folder/Title]]` | Disambiguate by path when two notes share a title. |
| `![[file.png]]`, `![[Title]]` | Embed an asset or a note (M3). |
| `relation:: [[Title]]` | Typed link. An inline field at the start of a line (or list item) types every link on that line. Common relations: `depends-on`, `derived-from`, `references`, `contradicts`, `part-of`, `source`, `blocks`. |
| `[text](https://…)` | Ordinary web link, not indexed as a note link. |

Links inside inline code or fenced code blocks are not links.

Following a link to a note that does not exist creates it in `notes/` with that title.

### Tasks and gaps

Tasks and gaps live inline in any note, as list items with a checkbox. Graphite indexes them (table `items`) so views can list, group and tick them, and edits made in a view rewrite only that line of the file.

| Line | Meaning |
| --- | --- |
| `- [ ] text` | Open task |
| `- [/] text` | Task in progress |
| `- [x] text` | Done |
| `- [-] text` | Cancelled (left out of progress) |
| `- [?] text` | Gap: something that needs finding out |
| `- [?] text answered:: [[Note]]` | Answered gap; the link is where the answer lives |

Rules:

- Any list marker works (`-`, `*`, `+`, `1.`), and items nest by indentation: an indented task under a task is its subtask, to any depth. Items inside fenced code are not items.
- **Inline fields** on the line add properties: `- [ ] Order motors due:: 2026-11-01 priority:: p1`. A value runs to the next field or the first `#tag`. Reserved fields: `due` (`YYYY-MM-DD`), `priority` (`p1`–`p4`, or `high`/`medium`/`low`), `project` (`[[Project]]`), `answered` (`[[Note]]`), `blocks` (`[[Task note]]` a gap stands in the way of). Other fields are kept and shown as-is.
- `#tags` anywhere on the line tag the item; `#p1`–`#p4` double as priority.
- **Project attribution**, first match wins: a `project:: [[X]]` field on the item or on an ancestor item; the note's `project` front-matter property; the note itself when its `type` is `project`. Tasks in a daily note or a meeting note therefore roll up to the project they name, and tasks written in a project note need nothing extra.
- A gap nested under a task is taken to block that task. Gaps are closed by answering them (`answered:: [[Note]]`), never by ticking.
- The nearest heading above an item is recorded, so a `## Tasks` section can be shown as such. New tasks added through the app go under `## Tasks` (gaps under `## Open questions`) when the note has that heading, else at the end of the note.
- A `type: task` note is a task that outgrew a line: its `status`, `due`, `priority` and `project` are front-matter properties and the tasks in its body are its subtasks.

Progress for a project is the share of its done tasks over all its tasks except cancelled ones, counted across every note.

### Images and attachments in Markdown

Standard syntax with a vault-relative path: `![alt](attachments/Garden Robot/chassis.png)`. Paths are relative to the vault root, use `/`, and never start with `/` or contain `..`.

### Captures and daily notes

Quick capture (global shortcut, CLI `graphite capture`, phone share sheet in M4) writes a plain note; nothing about it is special beyond where it lands:

- The note goes in `notes/Inbox/` with `type: capture` and a title of the form `2026-10-14 2130 <first line>` (first line of the text, or the URL's host and path, or the first attachment's name). Users rename and move it when they file it; the id keeps links intact.
- A capture whose text is a single URL records it as the `source` property so later importers (M3) can pick it up.
- Files dropped or pasted into a capture are copied to `attachments/Inbox/` (never moved) and referenced from the body: images as `![name](attachments/Inbox/name.png)`, everything else as `[name](attachments/Inbox/name.pdf)`. Name clashes get a `-2`, `-3` suffix.

Daily notes live at `notes/Daily/YYYY-MM-DD.md` with `type: daily` and the date as title. Opening "today" creates the file if it does not exist and never overwrites it.

## File names

New notes are written as `<folder>/<Title>.md`, with characters that are illegal on any supported OS replaced (`/ \ : * ? " < > | # ^ [ ]`) and whitespace collapsed. If the name is taken, creation fails (the UI asks for another title) rather than silently suffixing.

## Index (`.graphite/index.sqlite`)

SQLite. Tables: `notes`, `tags`, `aliases`, `properties`, `links`, and the FTS5 table `notes_fts`. The schema version lives in `meta`; any mismatch triggers a rebuild. The index is kept current by mtime+size comparison on each sync and by a file watcher while an app is open.

## Identity and stability

- A note's identity is its `id`. Moving or renaming the file keeps the id and the index follows it.
- A link's target is text; it is resolved at query time, so renaming a note does not rewrite other files (unlinked-mentions and rename-assist are UI features, not format rules).
- Two notes with the same title are allowed; links resolve to the first by path order, and the UI should prompt to disambiguate with `[[folder/Title]]`.

## Compatibility

The format is deliberately close to Obsidian's so an existing vault opens in Graphite and vice versa. Differences: Graphite requires nothing Obsidian lacks, writes `id` into front-matter, and reads `relation:: [[x]]` as a typed link (Obsidian/Dataview treat it as an inline field, which is harmless).
