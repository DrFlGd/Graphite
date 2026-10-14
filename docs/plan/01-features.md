# 01 · Feature catalogue

Every feature has an ID (used in milestones), a priority (P0/P1/P2, defined in `ROADMAP.md`), the milestone it is proposed for, a note on origin (**brief** = in the original request, **added** = suggested addition), and a **Decision** column to fill in during review.

Features are grouped by the capability area they belong to. Within an area, the order is roughly the build order.

---

## A · Vault and data model

The foundation everything else sits on.

| ID | Feature | Pri | MS | Origin | Decision |
| --- | --- | --- | --- | --- | --- |
| A1 | **Plain-files vault**: a folder of Markdown notes + `attachments/`; Graphite keeps a rebuildable SQLite index next to it | P0 | M0 | added | |
| A2 | **Notes with YAML front-matter properties** (`type`, `status`, `tags`, `project`, arbitrary user fields) | P0 | M1 | added | |
| A3 | **Note types**: `note`, `project`, `idea`, `area`, `task`, `gap`, `source`, `asset`, `decision`, `person`, user-defined | P0 | M1 | added | |
| A4 | **Tags** (nested `a/b/c`) and **properties** editable from a sidebar, not just front-matter text | P0 | M1 | added | |
| A5 | **Stable IDs** for every note and asset so links survive renames and moves | P0 | M1 | added | |
| A6 | **Multiple vaults** (e.g. personal vs. work), switchable | P2 | M8 | added | |
| A7 | **Templates** for note types and whole projects (research, build, software, writing) | P1 | M2 | added | |

## B · Rapid capture

The brief's top priority: lower the barrier to getting things in.

| ID | Feature | Pri | MS | Origin | Decision |
| --- | --- | --- | --- | --- | --- |
| B1 | **Inbox**: all captures land in an untyped inbox; filing, linking and typing happen later via a triage view | P0 | M1 | added | |
| B2 | **Global quick-capture hotkey** on desktop: a floating window for text, a URL or a file drop, one keystroke to save | P0 | M1 | added | |
| B3 | **Paste anything**: images, files, URLs and rich text pasted into any note become assets/sources automatically | P0 | M1 | added | |
| B4 | **Drag-and-drop files and folders** into a note or project | P0 | M1 | brief | |
| B5 | **Phone share sheet / intent** (iOS share extension, Android share target) | P0 | M4 | brief | |
| B6 | **Browser clipper** extension: clip a page, selection or screenshot, with the source URL recorded | P1 | M6 | added | |
| B7 | **Voice capture** on phone with on-device transcription | P1 | M4 | added | |
| B8 | **Email-in / URL inbox**: send a link or note to a private address or paste into a watched folder | P2 | M6 | added | |
| B9 | **Capture widgets / home-screen shortcuts** on mobile | P2 | M4 | added | |
| B10 | **Daily note / journal** as a default landing place for chronological capture | P1 | M1 | added | |

## C · Connected documentation

| ID | Feature | Pri | MS | Origin | Decision |
| --- | --- | --- | --- | --- | --- |
| C1 | **Markdown editor** with live preview (WYSIWYG-ish), slash commands, tables, code blocks, embeds | P0 | M1 | brief | |
| C2 | **Wiki-links** `[[note]]` with autocomplete, create-on-link, and `[[note#heading]]` / `[[note^block]]` targets | P0 | M1 | brief | |
| C3 | **Backlinks panel** and **unlinked mentions** (text that matches a note title but isn't linked yet) | P0 | M1 | added | |
| C4 | **Typed links**: optional relation on a link (`depends-on`, `derived-from`, `references`, `contradicts`, `part-of`, user-defined), expressed inline (`[[note|depends-on]]`) or via properties | P1 | M2 | added | |
| C5 | **Embeds / transclusion** of notes, blocks, images, video, PDFs, 3D models inside other notes | P0 | M3 | added | |
| C6 | **Link suggestions** while typing (title match now, semantic match in M7) | P1 | M1/M7 | added | |
| C7 | **Aliases** for notes so links can use alternate names | P1 | M1 | added | |
| C8 | **Note history**: per-note version history with diff and restore | P1 | M4 | added | |
| C9 | **Decision log**: `decision` note type with context/options/outcome, listed per project | P1 | M2 | added | |
| C10 | **Footnotes, callouts, math, Mermaid diagrams** | P2 | M5 | added | |

## D · Projects and progress tracking

| ID | Feature | Pri | MS | Origin | Decision |
| --- | --- | --- | --- | --- | --- |
| D1 | **Project hub**: a `project` note that aggregates its tasks, gaps, sources, assets, decisions and linked notes | P0 | M2 | brief | |
| D2 | **Tasks and subtasks** (arbitrary nesting) with status, due date, priority, effort, assignee (self), checklists | P0 | M2 | brief | |
| D3 | **Tasks inline in notes**: `- [ ]` lines in any note are real tasks, attributed to that note's project | P0 | M2 | added | |
| D4 | **Gaps / information needs** as first-class items: "I need to find out X", linked to the task(s) they block, with a resolution that links to the note/source that answered it | P0 | M2 | brief | |
| D5 | **Open questions view** per project: all unresolved gaps, what they block, how old they are | P0 | M2 | added | |
| D6 | **Progress roll-up**: % complete per project/milestone from task tree; simple burn-up | P1 | M2 | brief | |
| D7 | **Milestones / phases** inside a project; tasks and gaps attach to a phase | P1 | M2 | added | |
| D8 | **Dependencies between tasks** (blocked-by) with a critical-path highlight in timeline view | P1 | M5 | added | |
| D9 | **Recurring tasks and reminders** (local notifications on desktop and phone) | P2 | M4 | added | |
| D10 | **Idea → project promotion**: one action converts an `idea` into a `project` using a template | P1 | M2 | added | |
| D11 | **Project status / health**: active, paused, done, archived; stale-project nudges | P2 | M7 | added | |

## E · Media, files and assets

| ID | Feature | Pri | MS | Origin | Decision |
| --- | --- | --- | --- | --- | --- |
| E1 | **Assets**: any file in `attachments/` is an `asset` node with metadata, tags, properties and links; the file stays on disk unmodified | P0 | M3 | brief | |
| E2 | **Attachment folder conventions**: per-project folder by default, with a global shared folder; an asset can be referenced by many projects without copying | P0 | M3 | brief | |
| E3 | **Previews**: image, video (with scrubbing), audio, PDF (with page links), code, CSV/spreadsheet | P0 | M3 | brief | |
| E4 | **3D preview**: glTF/GLB, OBJ, STL, 3MF in a viewer; STEP/IGES via a conversion step | P0 | M3 | brief | |
| E5 | **Vector preview**: SVG natively; AI/EPS/DXF via conversion | P1 | M3 | brief | |
| E6 | **Metadata extraction**: EXIF, dimensions, duration, page count, model bounding box, file hash | P1 | M3 | added | |
| E7 | **Asset gallery view** per project / per type with filters | P1 | M3 | added | |
| E8 | **Image annotation** (arrows, boxes, text) saved as a sidecar so the original is untouched | P2 | M5 | added | |
| E9 | **Large-file policy**: configurable size threshold above which files are referenced in place (not copied) and excluded from sync | P1 | M4 | added | |
| E10 | **Deduplication** by content hash | P2 | M3 | added | |

## F · Importers and source tracking

"I provide a link and this software integrates it."

| ID | Feature | Pri | MS | Origin | Decision |
| --- | --- | --- | --- | --- | --- |
| F1 | **Source records**: every import creates a `source` note holding URL, type, fetch date, content hash, importer settings, and links to produced assets | P0 | M3 | brief | |
| F2 | **URL dispatcher**: paste a URL anywhere and the right importer is chosen (GitHub, YouTube/Vimeo/etc., generic web page, direct file) | P0 | M3 | brief | |
| F3 | **GitHub / git importer**: clone or shallow-clone a repo into the vault, record commit; README rendered as a note | P0 | M3 | brief | |
| F4 | **Web page importer**: single-file archive (HTML + inlined assets), readable-text extraction as a note, screenshot | P0 | M3 | brief | |
| F5 | **Website mirror** (crawl within a domain/path to a depth limit) | P1 | M3 | brief | |
| F6 | **Video/audio downloader** (yt-dlp-based), with chosen quality, thumbnails and chapter markers | P0 | M3 | brief | |
| F7 | **Update checks**: manual "check for updates" per source and a scheduled check; shows diff summary (new commits, changed page text, new video in playlist) and lets you re-import | P0 | M3 | brief | |
| F8 | **Playlist / channel / feed subscriptions**: RSS/Atom, YouTube channels, GitHub releases | P1 | M6 | added | |
| F9 | **PDF / document importer**: arXiv, DOI, generic PDF URLs with metadata | P1 | M3 | added | |
| F10 | **Importer plugin interface** so new source types can be added without touching core | P1 | M6 | added | |
| F11 | **Import from other tools**: Obsidian/Logseq vaults, Notion export, Evernote, Apple Notes, browser bookmarks | P2 | M8 | added | |

## G · Views and workflows

Every view is a **saved query + a layout**. The same filter (project = X, type = task, status != done) can be shown as a list, board, graph, timeline, or canvas.

| ID | Feature | Pri | MS | Origin | Decision |
| --- | --- | --- | --- | --- | --- |
| G1 | **Query language + saved queries** (filter by type, tag, property, link relation, text; sort; group) | P0 | M2 | added | |
| G2 | **List / table view** with inline property editing | P0 | M2 | added | |
| G3 | **Task tracker view** (tree + filters + due dates) | P0 | M2 | brief | |
| G4 | **Board / Kanban view** grouped by any property | P0 | M2 | brief | |
| G5 | **Node-graph view**: local graph (N hops from a note) and global graph; typed links drawn differently; filter by query; click-to-navigate; drag-to-link | P0 | M1 (basic) / M5 (full) | brief | |
| G6 | **Whiteboard / canvas**: free-form spatial arrangement of notes, assets, text, shapes and connectors; infinite canvas; a canvas is itself a note | P0 | M5 | brief | |
| G7 | **Timeline / Gantt** for tasks and milestones | P1 | M5 | added | |
| G8 | **Calendar view** (due dates, daily notes, captured-on) | P2 | M5 | added | |
| G9 | **Workspaces / layouts**: saved arrangements of panes and views per project | P1 | M5 | added | |
| G10 | **Split panes and tabs** on desktop | P0 | M1 | added | |
| G11 | **Command palette** (everything reachable by keyboard) | P0 | M1 | added | |
| G12 | **Focus / reading mode** | P2 | M5 | added | |

## H · Search and intelligence

| ID | Feature | Pri | MS | Origin | Decision |
| --- | --- | --- | --- | --- | --- |
| H1 | **Full-text search** across notes, properties, transcripts, OCR text, extracted PDF/web text, file names | P0 | M1 (notes) / M3 (media) | brief | |
| H2 | **Transcription** of imported/captured video and audio (local Whisper-class model), stored as a sidecar note with timestamps linking back into the video | P1 | M7 | added | |
| H3 | **OCR** on images and scanned PDFs | P1 | M7 | added | |
| H4 | **Semantic search** (local embeddings) with "related notes" panel | P1 | M7 | added | |
| H5 | **Ask-your-vault assistant**: chat over a project or the whole vault with citations to notes/timestamps; pluggable model (local via Ollama-style runtime, or API with explicit opt-in) | P1 | M7 | added | |
| H6 | **Assistant actions**: summarise a source, draft a task list from a note, propose links, extract gaps/questions from a note | P2 | M7 | added | |
| H7 | **Resurfacing / review**: periodic prompts for stale captures and untouched ideas; "on this day" | P2 | M7 | added | |
| H8 | **Auto-tagging and link suggestions** from embeddings | P2 | M7 | added | |

## I · Sync and multi-device

| ID | Feature | Pri | MS | Origin | Decision |
| --- | --- | --- | --- | --- | --- |
| I1 | **CRDT-based sync** of notes and metadata (conflict-free; no "pick a version" dialogs) | P0 | M4 | brief | |
| I2 | **Asset sync** with content-addressed blobs; selective sync (phone keeps thumbnails + recent) | P0 | M4 | brief | |
| I3 | **Transport options**: (a) folder-based via the user's existing sync (iCloud, Dropbox, Syncthing), (b) self-hosted relay, (c) optional hosted relay later | P0 | M4 | brief | |
| I4 | **End-to-end encryption** for anything that leaves the device | P1 | M4 | added | |
| I5 | **Device pairing** by QR code | P1 | M4 | added | |
| I6 | **Vault snapshots and backups** (local, scheduled, restore) | P1 | M4 | added | |
| I7 | **Git-backed vault option** for users who want history + hosting in one | P2 | M8 | added | |

## J · Platforms and interfaces

| ID | Feature | Pri | MS | Origin | Decision |
| --- | --- | --- | --- | --- | --- |
| J1 | **Desktop app**: macOS, Windows, Linux | P0 | M1 | brief | |
| J2 | **Phone app**: iOS and Android, focused on capture, reading, search and task updates (not a full editor at first) | P0 | M4 | brief | |
| J3 | **Tablet layout** (two-pane, pen-friendly canvas) | P2 | M5 | added | |
| J4 | **CLI** (`graphite capture "…"`, `graphite import <url>`, `graphite query …`) for scripting and automation | P1 | M6 | added | |
| J5 | **Local HTTP API** (what the CLI, clipper and plugins talk to) | P1 | M6 | added | |
| J6 | **Themes** (light/dark/custom) and keyboard-first design | P0 | M1 | added | |
| J7 | **Accessibility** basics (screen reader labels, reduced motion, scalable text) | P1 | M8 | added | |

## K · Integrations with specialised software

The brief asks for connectivity with 3D modelling, vector graphics and CAD tools. The strategy is: never try to edit those formats in Graphite; make it trivial to go there and back, and keep Graphite's view of the file current.

| ID | Feature | Pri | MS | Origin | Decision |
| --- | --- | --- | --- | --- | --- |
| K1 | **Open in…**: open any asset in the OS default app or a chosen app; Graphite records the association | P0 | M3 | brief | |
| K2 | **File watching**: when an asset changes on disk (edited externally), Graphite re-indexes, regenerates the preview, bumps the version | P0 | M3 | brief | |
| K3 | **Reveal in Finder/Explorer** and stable on-disk paths so external tools can link back | P0 | M3 | brief | |
| K4 | **Export views**: a project's assets as a folder, a canvas as SVG/PNG, a task list as CSV/Markdown | P1 | M5 | added | |
| K5 | **Conversion pipelines** (plugin-provided): STEP→glTF for preview, DXF→SVG, AI→SVG, Fusion/Blender export watchers | P1 | M6 | brief | |
| K6 | **Tool-specific plugins**: Blender add-on (push render/screenshot to a note), Fusion 360 script, Inkscape/Illustrator "send to Graphite", VS Code extension | P2 | M6 | brief | |
| K7 | **Plugin API**: importers, previewers, converters, views, commands, with a manifest and sandboxing | P1 | M6 | added | |
| K8 | **Webhooks / automation hooks** (on capture, on import, on task done) | P2 | M6 | added | |

## L · Security, privacy and operations

| ID | Feature | Pri | MS | Origin | Decision |
| --- | --- | --- | --- | --- | --- |
| L1 | **Encryption at rest** (optional, per-vault passphrase, OS keychain integration) | P1 | M8 | added | |
| L2 | **No telemetry by default**; opt-in crash reports | P0 | M1 | added | |
| L3 | **Performance targets**: 50k notes / 100 GB assets without the UI degrading; index rebuild in minutes | P1 | M8 | added | |
| L4 | **Export everything** (it's already files, but one-click zip + JSON of the index) | P1 | M8 | added | |
| L5 | **Auto-update** channels (stable/beta) | P1 | M8 | added | |

---

## Deferred list

Items moved here during review (marked `later`) with a one-line reason, so they are not lost.

| ID | Feature | Reason deferred |
| --- | --- | --- |
| | | |
