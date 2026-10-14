# 04 · Open questions

Decisions that shape the plan. Each has a recommendation; confirming or overriding them is the first step of M0, and each confirmed answer becomes an ADR in `docs/decisions/`.

| # | Question | Options | Recommendation | Decision |
| --- | --- | --- | --- | --- |
| Q1 | **Stack** | A. Tauri 2 + Rust core + TS/React UI · B. Electron + React Native · C. Flutter + Rust FFI · D. Native per platform | **A.** One core across every platform, small binaries, strongest CRDT/SQLite/file-watch story in Rust, richest UI component ecosystem (editor, graph, canvas, 3D) in web tech. | |
| Q2 | **Vault storage** | A. Plain Markdown files + rebuildable index · B. Database-only (SQLite as source of truth) · C. Hybrid with files exported on demand | **A.** Interoperability with specialised tools and other note apps, trivial backup, no lock-in. Cost: a reconciler between disk, CRDT and index, which is the hardest piece of core. | |
| Q3 | **CRDT library** | Automerge (Rust-native) · Yjs (JS-native) · Loro (newer, Rust, fast) | **Automerge** if Q1 = A (Rust core). Revisit Loro at M4 if its ecosystem has matured. | |
| Q4 | **Sync transport for v1** | A. Folder-based (user's own iCloud/Dropbox/Syncthing) · B. Self-hosted relay first · C. Hosted service first | **A then B.** Folder sync means zero infrastructure for desktop↔desktop; the relay is needed for phones without a shared folder and comes in the same milestone. No hosted service unless wanted later. | |
| Q5 | **Platform priority** | Desktop first (M1–M3), phone in M4 · Phone capture earlier (swap M3/M4) | **Desktop first**, but confirm: if phone capture is the thing you'd use most on day one, swap M3 and M4. | |
| Q6 | **Mobile scope at first release** | Capture + read + tasks + search (no full editor) · Full parity | **Capture + read + tasks + search.** Full editing on phone can follow once the editor is stable on desktop; it is the same web component. | |
| Q7 | **Licence** | MIT · Apache-2.0 · AGPL-3.0 · Source-available / proprietary | **MIT or Apache-2.0** if the goal is adoption and contributions; **AGPL** if you want to prevent hosted forks. Affects library choices (e.g. tldraw has a licence with a watermark clause for commercial use). | |
| Q8 | **AI provider policy** | Local-only · Local default + opt-in API (Claude/OpenAI/etc.) · API-first | **Local default + opt-in API.** Keeps the privacy promise; API gives better answers when you choose. | |
| Q9 | **Whiteboard engine** | tldraw · Excalidraw · custom canvas | **tldraw** (extensible shapes, strong editor model) subject to licence check under Q7; Excalidraw as fallback. | |
| Q10 | **Editor model** | Markdown source with live preview (CodeMirror 6) · Rich editor that saves Markdown (TipTap/ProseMirror) | **TipTap/ProseMirror saving Markdown**, with a raw-Markdown toggle. Better for embeds, tables and non-technical editing; Markdown stays the on-disk format either way. | |
| Q11 | **Monorepo layout** | Single repo (`core/`, `apps/`, `packages/`, `plugins/`) · Separate repos per app | **Single repo**; the whole point is one core shared by every shell. | |
| Q12 | **Large / binary assets in sync** | Sync everything · Size threshold, reference-only above it · Never sync assets, only metadata | **Size threshold** (default 500 MB, configurable), with cloned repos and downloaded video defaulting to reference-only on phones. | |
| Q13 | **Name collision** | "Graphite" is also an existing open-source 2D graphics editor (graphite.rs) and a code-review tool (graphite.dev) | Keep the name for the repo; decide on a product name before public release (M8). Not blocking. | |
| Q14 | **Anything to drop from the feature catalogue now?** | See the `Decision` columns in `01-features.md` | Nothing is required; P2 items are already positioned post-1.0 or as plugins. | |
