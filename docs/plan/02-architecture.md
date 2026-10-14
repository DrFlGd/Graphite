# 02 · Architecture considerations

This document lays out the technical shape that supports the feature catalogue, and recommends a stack. Each major choice is also an open question in `04-open-questions.md`; once agreed it becomes an ADR in `docs/decisions/`.

## 1. Core shape

```
┌───────────────────────────────────────────────────────────────┐
│  Clients                                                      │
│  Desktop (mac/win/linux)   Phone (iOS/Android)   CLI  Clipper │
└───────────────┬───────────────────────┬───────────────┬───────┘
                │ UI layer (shared TS)  │               │ local HTTP API
┌───────────────▼───────────────────────▼───────────────▼───────┐
│  Core (shared library)                                        │
│  • Vault model (notes, assets, links, properties)             │
│  • Index (SQLite + FTS + vector table)                        │
│  • Query engine (saved queries → views)                       │
│  • Importers, previewers, converters (plugin host)            │
│  • CRDT document store + sync engine                          │
│  • File watcher / reconciler (disk ⇄ index ⇄ CRDT)            │
└───────────────┬───────────────────────────────────────────────┘
                │
┌───────────────▼───────────────┐   ┌───────────────────────────┐
│  Vault on disk                │   │  Sync transport           │
│  notes/*.md  attachments/**   │◄─►│  folder / relay / hosted  │
│  .graphite/ (index, crdt log) │   │  (E2E encrypted)          │
└───────────────────────────────┘   └───────────────────────────┘
```

Three load-bearing ideas:

1. **The vault is files; the database is a cache.** Markdown notes with YAML front-matter and an `attachments/` tree are the source of truth. `.graphite/index.sqlite` is rebuilt from them on demand. This gives interoperability with every other tool, trivial backups, and a safety net if Graphite ever breaks.
2. **A CRDT layer for sync, not for storage.** Each note's text and properties are also represented as a CRDT document (Automerge or Yjs) in `.graphite/crdt/`. Sync exchanges CRDT changes. The reconciler keeps the Markdown file, the CRDT doc and the index consistent, and treats *external edits to the Markdown file* as just another writer.
3. **One core, several shells.** Desktop, phone, CLI and the clipper all talk to the same core library (in-process on desktop/phone, over a local HTTP API for the CLI/clipper/plugins).

## 2. Vault format

```
MyVault/
├── notes/
│   ├── Projects/Garden Robot.md          # type: project
│   ├── Ideas/Solar still.md               # type: idea
│   ├── Inbox/2026-10-14 1412 clip.md      # untyped capture
│   ├── Daily/2026-10-14.md
│   └── …                                  # folders are a convenience, not semantics
├── attachments/
│   ├── Garden Robot/chassis-v3.step
│   ├── Garden Robot/chassis-v3.step.preview.glb   # derived, regenerable
│   ├── _shared/datasheet-bme280.pdf
│   └── sources/github/octocat~hello-world/        # cloned repo
├── canvases/Garden Robot layout.canvas.json
└── .graphite/
    ├── index.sqlite                       # FTS5, properties, links, embeddings
    ├── crdt/                              # per-note CRDT docs + sync log
    ├── derived/                           # thumbnails, transcripts, OCR text
    ├── sources.json                       # importer state (URLs, hashes, last check)
    └── config.json
```

- **Note identity**: a `id:` ULID in front-matter. Links are written as `[[Title]]` for readability but resolved by ID; the index maps both. Renames rewrite link text lazily.
- **Front-matter is the property store.** Reserved keys: `id`, `type`, `tags`, `aliases`, `project`, `status`, `created`, `updated`, `source`. Everything else is user-defined.
- **Typed links**: `[[Target|depends-on]]` inline, or a `relations:` block in front-matter. Both index to the same `links(from, to, type, position)` table.
- **Tasks** are `- [ ]` lines with optional inline properties (`due:: 2026-11-01`, `#p1`, `^blockid`) *or* a `task` note when they need their own body. Both appear in the same task queries.
- **Gaps** are `- [?] I need to know …` lines or `gap` notes. A gap resolves by linking to the note/source that answered it.
- **Assets** have no Markdown file by default; metadata lives in the index with an optional sidecar `file.ext.md` when the user adds notes/properties to it. Derived files (`*.preview.glb`, thumbnails) live beside the original or in `.graphite/derived/` and are never synced (regenerated per device).
- **Sources** are `source` notes with front-matter: `url`, `importer`, `fetched`, `hash`, `check: weekly`, `produces: [asset ids]`.

## 3. Index and query

- **SQLite** with FTS5 for full-text, a `links` table, a `properties` key/value table (EAV with typed columns), and a vector table (`sqlite-vec` or similar) for embeddings.
- **Query language**: a small, readable filter language (think Dataview/Obsidian-Bases style), compiled to SQL. Example: `type:task project:[[Garden Robot]] status:!done sort:due`. Saved queries are notes of type `query`; views reference a query + layout.
- Index rebuild from disk must be fast and idempotent; it is the recovery path for everything.

## 4. Sync

| Concern | Approach |
| --- | --- |
| Model | Per-note CRDT (text + map) using **Automerge** (Rust core, good binary format) or **Yjs** (bigger ecosystem, JS-native). Recommendation: Automerge if the core is Rust, Yjs if the core is TypeScript. |
| Assets | Content-addressed blobs (`sha256`), manifest synced as a CRDT map; blobs fetched lazily; phones keep thumbnails + pinned/recent. Files over a size threshold are reference-only. |
| Transport v1 | **Folder-based**: `.graphite/crdt/` and blobs written as append-only files that any file-sync service (iCloud Drive, Dropbox, Syncthing) can carry safely. Zero servers. |
| Transport v2 | **Self-hosted relay** (small Rust/Node service, WebSocket) for real-time sync and phones that don't have the folder. |
| Transport v3 | Optional hosted relay (same software) if it is ever wanted. |
| Encryption | E2E: a vault key held in each device's keychain; relay and folder only ever see ciphertext. |
| External edits | A Markdown file edited outside Graphite is diffed against the last-known text and applied to the CRDT as a change from a virtual "disk" peer. |

## 5. Platforms and stack options

The main fork is whether the shared core is Rust or TypeScript, and whether the UI shells are web-tech-in-a-wrapper or native.

| Option | Desktop | Mobile | Core | Pros | Cons |
| --- | --- | --- | --- | --- | --- |
| **A. Tauri 2 + Rust core + web UI** (recommended) | Tauri 2 (small binaries, native webview) | Tauri 2 mobile (iOS/Android) | Rust (SQLite, Automerge, file watching, yt-dlp/git subprocesses) exposed to a TypeScript UI | One core for every platform; fast; small; native file access; strong CRDT/SQLite story in Rust | Tauri mobile is newer than its desktop side; some platform polish (share extensions, widgets) still needs Swift/Kotlin glue |
| B. Electron + TS core + React Native mobile | Electron | React Native / Expo | TypeScript (better-sqlite3 / op-sqlite, Yjs) | Largest ecosystem, fastest UI iteration | Two runtimes to keep in step; bigger binaries; heavier work (transcription, 3D conversion) wants native anyway |
| C. Flutter everywhere | Flutter | Flutter | Dart + Rust via FFI | Single UI codebase incl. mobile; good canvas performance | Weaker rich-text/Markdown editor ecosystem; web-tech libraries (graph, 3D, editors) harder to reuse |
| D. Native (SwiftUI / Kotlin / etc.) | per-OS | per-OS | Rust via FFI | Best platform fit | 3–4× the UI work for one person |

**Recommendation: A.** Rust core for the heavy, correctness-critical parts (vault reconciliation, index, CRDT, sync, importers), TypeScript/React UI reused across desktop and mobile, with small native shims where the platform demands it (iOS share extension, Android share target, widgets). The web stack also gives the richest off-the-shelf components for the hard UI pieces:

- Editor: ProseMirror/TipTap or CodeMirror 6 (Markdown-native) with a live-preview mode.
- Graph: Sigma.js / Cytoscape.js / a custom WebGL force layout.
- Whiteboard: tldraw (excellent, MIT-licensed, extensible shapes) or Excalidraw.
- 3D preview: three.js (glTF/GLB/OBJ/STL/3MF loaders out of the box).
- PDF: pdf.js. Video: native `<video>` + HLS where needed.

Heavy processing stays out of the UI thread and out of the webview: yt-dlp, git, ffmpeg, OCR and Whisper run as sidecar processes managed by the Rust core, with a job queue and progress reporting.

## 6. Importers

Each importer is a small module implementing: `canHandle(url)`, `fetch(source) → artifacts`, `checkForUpdates(source) → changes`, `diff(old, new) → summary`. Built-ins for M3: git/GitHub, generic web page (single-file archive + readability text + screenshot), site mirror (depth-limited crawl), video/audio (yt-dlp), direct file/PDF. Importers run in the job queue, record everything in the `source` note, and are idempotent on re-run. Scheduling: a per-source `check:` interval plus a global "check all sources" command.

## 7. Plugin surface

Kept deliberately small and introduced in M6 once the core has stabilised:

- **Extension points**: importers, previewers (file type → component), converters (format → format), views (query → component), commands, capture handlers.
- **Runtime**: plugins are JS modules loaded in an isolated webview context with a capability-gated API (no raw filesystem; vault API only). Native helpers ship as separate sidecar binaries the plugin can request permission to run.
- **Local HTTP API** (loopback, token-authenticated) is the same API plugins see; the CLI, the browser clipper and third-party tools (a Blender add-on, a Fusion script) all go through it.

## 8. Cross-cutting

- **Jobs**: one job queue for imports, indexing, previews, transcription, sync; visible in a "Activity" panel with cancel/retry.
- **Performance**: virtualised lists everywhere; index queries not file reads for all views; lazy previews; vault watch via OS APIs (FSEvents/inotify/ReadDirectoryChangesW).
- **Testing**: Rust core has unit + property tests (CRDT/reconciler invariants); vault fixtures of increasing size for perf tests; UI e2e with Playwright on desktop, Maestro/Detox on mobile.
- **Privacy**: no network calls without a user-visible reason; AI providers are pluggable and default to local.
- **Licensing**: recommend an open-source licence (MIT or AGPL) decided in M0; it affects which libraries can be used (tldraw's licence, for example, has a watermark/commercial clause to check).
