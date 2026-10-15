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
├── templates/             the vault's own note and project templates (optional)
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

Standard syntax with a vault-relative path: `![alt](attachments/Garden Robot/chassis.png)`. Paths are relative to the vault root, use `/`, and never start with `/` or contain `..`. Spaces and parentheses in a path are written percent-encoded (`attachments/Garden%20Robot/x.png`), as CommonMark requires; readers decode them. A relative path in `[text](path)` or `![alt](path)` with no scheme is a **file link** and is indexed (kind `file`), so an asset knows which notes reference it; `![[chassis.stl]]` embeds the file by name. A relative `.md` path is an ordinary note link.

### Assets (M3)

Every non-Markdown file in the vault (outside `.graphite/` and hidden folders) is an **asset**, keyed by its vault-relative path, which is stable so external tools can link back to it. The file is never modified: everything Graphite learns about it goes in the index and everything it generates goes under `.graphite/derived/`.

- **Kind** follows the extension: `image`, `vector` (SVG, AI, EPS, DXF), `video`, `audio`, `pdf`, `model` (glTF/GLB, OBJ, STL, PLY, 3MF, STEP/IGES, FBX…), `code`, `data` (CSV, JSON, YAML, XML, spreadsheets), `document`, `archive`, `other`.
- **Metadata** recorded at index time, from the file alone: size, modified time, image or SVG dimensions, model bounding box and dimensions (STL, OBJ, glTF), CSV rows and columns. Derived afterwards, in the background: SHA-256 content hash, video/audio duration, dimensions and codecs (ffprobe), PDF page count, title and author (pdfinfo).
- **Derived files** under `.graphite/derived/`, named by the first 24 hex characters of the content hash: `<hash>.jpg` thumbnails (video frame, PDF first page, images, via ffmpeg and pdftoppm) and `<hash>.txt` extracted text (PDF text via pdftotext; code and data files verbatim up to 2 MB). Extracted text and file names are full-text searchable alongside notes. Missing tools simply mean no thumbnail or text; nothing else changes.
- **Folders** are a convention: `attachments/<Project>/` for a project's own files, `attachments/` for shared ones, `attachments/sources/` for imported content. An asset can be referenced from any number of notes and projects without copying.
- A note with `type: asset` and a `file` property naming the path can carry tags, properties and prose about a file; the file itself stays untouched.
- Editing a file in another app is picked up by the watcher: the asset is re-indexed and its previews regenerated.

### Captures and daily notes

Quick capture (global shortcut, CLI `graphite capture`, phone share sheet in M4) writes a plain note; nothing about it is special beyond where it lands:

- The note goes in `notes/Inbox/` with `type: capture` and a title of the form `2026-10-14 2130 <first line>` (first line of the text, or the URL's host and path, or the first attachment's name). Users rename and move it when they file it; the id keeps links intact.
- A capture whose text is a single URL records it as the `source` property so later importers (M3) can pick it up.
- Files dropped or pasted into a capture are copied to `attachments/Inbox/` (never moved) and referenced from the body: images as `![name](attachments/Inbox/name.png)`, everything else as `[name](attachments/Inbox/name.pdf)`. Name clashes get a `-2`, `-3` suffix.

Daily notes live at `notes/Daily/YYYY-MM-DD.md` with `type: daily` and the date as title. Opening "today" creates the file if it does not exist and never overwrites it.

### Projects, decisions and templates

A **project** is a note with `type: project`. Nothing else is required: its tasks are the tasks attributed to it (see above), its decisions are `type: decision` notes that link to it or name it in a `project` property, its sources are `type: source` notes linked either way, and its linked notes are everything else linking in or out. The hub view is computed from the index; the file holds only what the user wrote.

A **decision** is a note with `type: decision`, usually with `status` (`proposed`, `accepted`, `rejected`, `superseded`), `date` and `project: "[[Project]]"` properties and `## Context`, `## Options`, `## Outcome` headings. Only the type matters to Graphite.

**Templates** fill a new note. Graphite ships `project`, `research`, `build`, `software`, `writing`, `decision` and `query`. A vault adds or replaces templates with notes under `templates/` whose front-matter has `type: template`; the file name (lower-cased, spaces to `-`) is the template's name. Template front-matter keys with a meaning: `description` (shown in pickers), `for` or `note_type` (the `type` of the note it creates, default `note`), `folder` (where it is created; defaults by type to `notes/Projects`, `notes/Decisions`, `notes/Ideas`, `notes/Queries`, else `notes/`). Every other key becomes a property of the new note. In the body and in property values, `{{title}}`, `{{date}}` (`YYYY-MM-DD`), `{{datetime}}` (RFC 3339) and `{{body}}` are replaced; `{{body}}` is the note's existing text when an idea is promoted to a project (otherwise empty) and is appended at the end when the template has no slot for it.

Promoting a note to a project changes its `type`, adds the template's properties where the note has none, and wraps its text in the template body. The file stays where it is and keeps its id, so nothing linking to it changes.

### Queries and saved queries

Every list, table, board and tracker view is a query. A query is one line of space-separated terms; `"quoted text"` and `[[links]]` stay together.

| Term | Meaning |
| --- | --- |
| `key:value` | Field equals value (case-insensitive). `key:!value` not equal, `key:>v` `key:<v` `key:>=v` `key:<=v` compare (numbers, dates and `p1`–`p4` compare as such), `key:*` the field is set. |
| `#tag` | Has the tag (a tag's parents match its children: `#hardware` matches `hardware/motors`). |
| `word` | Free text: full-text search over notes, substring match on task and gap text. |
| `sort:field,-field` | Sort ascending by `field`, descending with `-`. |
| `group:field` | Group the rows by a field; the result lists each group's key and size. |
| `limit:n` | At most `n` rows. |

`type:task`, `type:gap` or `type:item` makes the query run over tasks and gaps; any other query runs over notes.

Note keys: `type`, `tag`, `in` (folder or path prefix; `folder`, `path`), `title`, `links` or `to` (links to a note), `from` or `backlinks` (linked from a note), `project` (the `project` property), `created`, `updated`, `id`, and any front-matter property by name. Item keys: `type`, `project`, `note`, `in`, `status` (`open` also matches `doing`; `closed` is done, cancelled or answered), `due`, `priority`, `tag`, `heading`, `text`, `depth`, and any inline field by name. Dates accept `today`, `tomorrow`, `yesterday` and offsets such as `+7d`, `-2w`, `+1m`, `+1y`.

A **saved query** is a note with `type: query` and the query in a `query` property; `view` (`list`, `table`, `board`, `tree`) is a hint for how to show it. Opening the note runs the query.

## File names

New notes are written as `<folder>/<Title>.md`, with characters that are illegal on any supported OS replaced (`/ \ : * ? " < > | # ^ [ ]`) and whitespace collapsed. If the name is taken, creation fails (the UI asks for another title) rather than silently suffixing.

## Index (`.graphite/index.sqlite`)

SQLite. Tables: `notes`, `tags`, `aliases`, `properties`, `links`, `items` (tasks and gaps), `assets`, and the FTS5 tables `notes_fts` and `assets_fts` (file names and extracted text). The schema version lives in `meta`; any mismatch triggers a rebuild. The index is kept current by mtime+size comparison on each sync and by a file watcher while an app is open.

## Identity and stability

- A note's identity is its `id`. Moving or renaming the file keeps the id and the index follows it.
- A link's target is text; it is resolved at query time, so renaming a note does not rewrite other files (unlinked-mentions and rename-assist are UI features, not format rules).
- Two notes with the same title are allowed; links resolve to the first by path order, and the UI should prompt to disambiguate with `[[folder/Title]]`.

## Compatibility

The format is deliberately close to Obsidian's so an existing vault opens in Graphite and vice versa. Differences: Graphite requires nothing Obsidian lacks, writes `id` into front-matter, and reads `relation:: [[x]]` as a typed link (Obsidian/Dataview treat it as an inline field, which is harmless).
