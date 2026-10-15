# Graphite

A local-first, multi-modal "second brain" for capturing ideas, projects and knowledge areas, organising the material around them (text, images, video, 3D models, source code, web pages), connecting it all together, and turning it into finished projects.

Your data is a folder of Markdown notes and attachments ([vault format](docs/spec/vault-format.md)); Graphite keeps a rebuildable index next to it. The plan is in [`ROADMAP.md`](ROADMAP.md) and [`docs/plan/`](docs/plan/); decisions in [`docs/decisions/`](docs/decisions/).

## Status

Milestone **M0 (foundations)** and the capture half of **M1**: a desktop app that opens a vault, lists notes, edits them in a rich Markdown editor with an insert toolbar and `/` menu, links notes with `[[`, shows backlinks and unlinked mentions, and searches the vault. `Ctrl/Cmd+Shift+Space` anywhere opens a quick-capture box that saves to the inbox (text, links, dropped or pasted files); an inbox view files captures into folders; files dropped or pasted into a note are copied into `attachments/` and linked; `Ctrl/Cmd+D` opens today's daily note. Plus a CLI over the same core.

## Repository

| Path | What |
| --- | --- |
| `core/` | Rust crate: vault model, front-matter, links, SQLite/FTS index, file watcher |
| `apps/cli/` | `graphite` command-line tool |
| `apps/desktop/` | Tauri 2 desktop app (Rust shell in `src-tauri/`, React UI in `src/`) |
| `packages/ui/` | Shared React components: the note editor and its block registry |
| `fixtures/sample-vault/` | A small vault to try things on |
| `docs/` | Plan, specs, ADRs |

## Developing

Prerequisites: Rust (stable), Node 22, pnpm 10. On Linux, Tauri also needs `libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev` (see `.github/workflows/ci.yml`).

```sh
pnpm install
cargo test -p graphite-core          # core tests
pnpm test                            # editor round-trip tests
pnpm dev                             # desktop app (dev mode)

cargo build -p graphite-cli
GRAPHITE_VAULT=fixtures/sample-vault target/debug/graphite list
GRAPHITE_VAULT=fixtures/sample-vault target/debug/graphite capture "Look into brushless motor drivers" -f spec.pdf
GRAPHITE_VAULT=fixtures/sample-vault target/debug/graphite daily
```

Open `fixtures/sample-vault` (or any folder) from the app's welcome screen. Shortcuts: `Ctrl/Cmd+Shift+Space` quick capture (works while the app is in the background), `Ctrl/Cmd+K` search, `Ctrl/Cmd+N` new note, `Ctrl/Cmd+D` today's note, `/` in a note for blocks, `[[` to link. Drop or paste files into a note to attach them.

## Licence

MIT, see `LICENSE`.
