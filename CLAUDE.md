# Graphite

Local-first, multi-modal second brain. Plan: `ROADMAP.md`, `docs/plan/`; decisions: `docs/decisions/`; vault file format: `docs/spec/vault-format.md`.

## Layout

- `core/` — Rust crate `graphite-core`: vault model, front-matter, links, SQLite index/FTS, file watcher. Everything that touches the vault goes here, never in an app.
- `apps/cli/` — `graphite` CLI over the core.
- `apps/desktop/` — Tauri 2 desktop app. `src-tauri/` is the Rust shell exposing core operations as commands; `src/` is the React app.
- `packages/ui/` — shared React components, chiefly the note editor (Tiptap, Markdown in/out) and its block registry.
- `fixtures/sample-vault/` — a small vault used by tests and for trying the app.

## Commands

- `cargo test -p graphite-core` — core tests (fast, no system deps).
- `cargo build -p graphite-cli` then `GRAPHITE_VAULT=fixtures/sample-vault target/debug/graphite list`.
- `pnpm install`, `pnpm typecheck`, `pnpm test` — UI package and desktop frontend.
- `pnpm dev` — desktop app in dev mode (needs Rust and, on Linux, webkit2gtk-4.1 dev packages).
- `cargo fmt --all` and `cargo clippy -p graphite-core -p graphite-cli --all-targets -- -D warnings` before pushing; CI enforces both.

## Conventions

- The vault on disk is the source of truth; `.graphite/` is a rebuildable cache. Never add a feature that needs a non-text source of truth for note content (ADR-002).
- Markdown written by the editor must round-trip; every block type has a test in `packages/ui/src/editor/markdown.test.ts` (ADR-005).
- New editor block types: one file in `packages/ui/src/editor/blocks/` + one entry in `blocks/index.ts`; the toolbar and `/` menu pick it up.
- Tauri commands are thin: validate, call core, map errors to strings. Logic lives in `core/`.
- Keep user-facing strings plain and short; no telemetry.
