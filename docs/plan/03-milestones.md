# 03 · Milestones

Each milestone has a goal, the feature IDs it delivers (from `01-features.md`), exit criteria, and what can be cut from it without undermining the next milestone. Milestones are meant to each end in something usable; M1 should already be a tool worth living in for notes.

No dates are attached. Once the feature set is agreed, each milestone gets broken into issues and sized.

---

## M0 · Foundations

**Goal:** make the decisions that are expensive to change later and set up the repo so every later milestone has a place to land.

**Delivers:** A1 (vault format spec), L2, the answers in `04-open-questions.md` as ADRs.

| Work item | Decision |
| --- | --- |
| Confirm stack (recommended: Tauri 2 + Rust core + TypeScript/React UI) and write ADR-001 | |
| Write the vault format spec (file layout, front-matter schema, link syntax, task/gap syntax, IDs) as `docs/spec/vault-format.md` | |
| Choose CRDT library and write ADR-002 (even though sync is M4, note storage must be CRDT-compatible from day one) | |
| Choose licence and write ADR-003 | |
| Repo scaffolding: monorepo layout (`core/`, `apps/desktop`, `apps/mobile`, `packages/ui`, `plugins/`), CI (lint, test, build on 3 OSes), release pipeline skeleton | |
| Design language: typography, spacing, colour tokens, iconography, keyboard conventions; low-fi mockups of the main screens (inbox, note, project hub, graph) | |
| Sample vault fixture for development and tests | |

**Exit criteria:** a fresh clone builds an empty desktop app on all three OSes in CI; the vault spec is written and has a fixture; ADRs 001–003 merged.

**Can be cut:** mockups can be sketches; release pipeline can be a stub.

---

## M1 · Capture & Connect (desktop MVP)

**Goal:** a desktop notes app you would actually use: capture fast, link everything, see the connections.

**Delivers:** A2, A3, A4, A5, B1, B2, B3, B4, B10, C1, C2, C3, C6 (title match), C7, C11, C12, G5 (basic), G10, G11, H1 (notes), J1, J6, L2.

**Exit criteria:**
- Open a vault folder; notes on disk appear; edits round-trip to disk; external edits show up live.
- Global hotkey opens quick capture; text/URL/file lands in Inbox in one keystroke.
- `[[` autocomplete, backlinks panel, unlinked mentions, create-note-on-link.
- Every block type (table, code block, embed, image, callout, task list…) can be inserted from an icon toolbar, a `/` menu, or a shortcut; tables and code blocks have in-place editing controls.
- Local graph view navigable; global graph renders a 1k-note vault smoothly.
- Full-text search across notes under 100 ms on a 10k-note fixture.
- Command palette reaches every command; split panes and tabs.

**Can be cut:** daily notes (B10), aliases (C7), global graph (keep local graph only).

---

## M2 · Projects & Progress

**Goal:** turn notes into projects with tasks, gaps and progress, and introduce the query engine that every later view uses.

**Delivers:** A7, C4, C9, D1–D7, D10, G1–G4.

**Exit criteria:**
- Create a project from a template; its hub shows tasks, gaps, sources (empty for now), linked notes and decisions.
- Tasks nest arbitrarily; inline `- [ ]` in any note shows up in the project's task tracker; board view groups by any property; progress roll-up on the hub.
- Gaps are creatable from any note (`- [?]`), link to the tasks they block, appear in the per-project Open Questions view, and resolve by linking an answer.
- Saved queries: write a query, save it, show it as list/board/task tree.
- Typed links render distinctly in the graph.

**Can be cut:** progress roll-up visuals (D6), decision log (C9), milestones within projects (D7).

---

## M3 · Media & Importers

**Goal:** make Graphite multi-modal: any file is a first-class node with a preview, and a URL becomes content that Graphite keeps current.

**Delivers:** C5, E1–E7, E10, F1–F7, F9, H1 (media text), K1–K3.

**Exit criteria:**
- Drop a STEP/STL/GLB, an SVG, a PDF, a video and an image into a project: each gets a preview, metadata and can be embedded in a note.
- Paste a GitHub URL → repo cloned under `attachments/sources/`, README shown as a note, source record created. Paste a YouTube URL → video downloaded with thumbnail. Paste an article URL → single-file archive + readable text note + screenshot.
- "Check for updates" on each source reports changes (new commits, changed page text) and can re-import.
- Edit a file in an external app → Graphite notices, re-indexes, regenerates the preview.
- Full-text search covers extracted PDF/web text.

**Can be cut:** site mirror (F5), DXF/AI conversion (E5), gallery view (E7), dedup (E10).

---

## M4 · Sync & Mobile

**Goal:** the same vault on a phone and a desktop, converging without conflict dialogs, with phone capture that is faster than any other app on the device.

**Delivers:** B5, B7, B9, C8, D9, E9, I1–I6, J2.

**Exit criteria:**
- Two desktops syncing via a shared folder (iCloud/Dropbox/Syncthing) converge on concurrent edits to the same note with no data loss (property-tested).
- Self-hosted relay: a phone with no shared folder syncs in near-real-time over the relay; everything is E2E encrypted; relay stores only ciphertext.
- Phone app: share sheet capture (text/URL/image/file), voice memo with transcription, inbox triage, note reading with embeds, search, task tick-off, project hub read-only.
- Selective asset sync: phone holds thumbnails and recent/pinned assets only.
- Per-note history with restore; scheduled vault snapshots.

**Can be cut:** voice capture (B7), widgets (B9), reminders (D9), QR pairing (I5, use a pasted key).

**Reordering note:** if phone capture matters more than importers, swap M3 and M4; nothing in M4 depends on M3 except the asset-sync policy for large downloaded media (E9), which can be set conservatively.

---

## M5 · Views & Whiteboard

**Goal:** the remaining workflows from the brief: spatial thinking on a canvas, time on a timeline, and layouts that make a project feel like a workspace.

**Delivers:** C10, D8, E8, G5 (full), G6–G9, G12, J3, K4.

**Exit criteria:**
- Whiteboard: infinite canvas with notes, assets (image/video/3D/PDF cards), text, shapes, connectors; drawing a connector creates a real link; a canvas is a note that can be linked and embedded.
- Full graph: filter by query, typed-link styling, cluster by project, drag-to-link, pin and save layouts.
- Timeline/Gantt for tasks + milestones with blocked-by edges.
- Workspaces: save and restore pane layouts per project.
- Export a canvas to SVG/PNG, a task list to CSV/Markdown.

**Can be cut:** image annotation (E8), calendar (G8), tablet layout (J3), focus mode (G12), math/Mermaid (C10).

---

## M6 · Integrations & Plugins

**Goal:** open the edges: a plugin API, a CLI and local API, a browser clipper, and the first bridges into specialised software.

**Delivers:** B6, B8, F8, F10, J4, J5, K5–K8.

**Exit criteria:**
- Local HTTP API with token auth; CLI covers capture, import, query, export.
- Browser clipper (Chrome/Firefox) clipping page/selection/screenshot with source URL.
- Plugin API: manifest, capability permissions, extension points for importer/previewer/converter/view/command; two reference plugins shipped (e.g. STEP→glTF converter, RSS importer).
- At least one tool-specific bridge demonstrated end-to-end (recommended first: Blender add-on that pushes a render + the `.blend` path into a project note).
- Feed/playlist/channel subscriptions as sources with update checks.

**Can be cut:** email-in (B8), webhooks (K8), all but one tool-specific plugin (K6).

---

## M7 · Intelligence

**Goal:** "interrogate it quickly": everything is searchable including media, related things surface themselves, and you can ask the vault questions.

**Delivers:** C6 (semantic), D11, H2–H8.

**Exit criteria:**
- Imported/captured video and audio get timestamped transcripts; images and scanned PDFs get OCR text; both are searchable and link back into the media at the right spot.
- Semantic search and a "related" panel driven by local embeddings; works offline.
- Assistant: ask a question scoped to a project or vault, get an answer with citations that open the note/timestamp; model is pluggable with local default.
- Resurfacing: a weekly review surface of stale inbox items, untouched ideas and unresolved gaps.

**Can be cut:** assistant actions (H6), auto-tagging (H8), resurfacing (H7), project health (D11).

---

## M8 · Hardening & Release

**Goal:** 1.0: safe, fast, installable, documented.

**Delivers:** A6, F11, I7, J7, L1, L3–L5.

**Exit criteria:**
- Encryption at rest with keychain-held passphrase; export-everything; auto-update channels.
- 50k-note / 100 GB-asset fixture: UI stays responsive, index rebuild in minutes, sync converges.
- Importers from Obsidian/Logseq/Notion exports.
- Signed, notarised installers for all platforms; app-store builds for phone; user docs; a sample vault that doubles as the tutorial.

**Can be cut:** multiple vaults (A6), git-backed vault (I7), some import formats (F11).

---

## Dependency sketch

```
M0 ─► M1 ─► M2 ─► M3 ─► M5 ─► M6 ─► M8
              │      ▲          ▲
              └─► M4 ┘          │
                     └──────────┴─► M7 (H2/H3 need M3 media; H4/H5 need M1 index)
```

- M4 depends on M1/M2 only, so it can run in parallel with M3 or precede it.
- M7 pieces can be pulled forward individually: full-text search is already in M1, transcription/OCR can ship with M3 if the sidecar tooling is ready.
