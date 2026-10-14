# Graphite

A local-first, multi-modal "second brain" for capturing ideas, projects and knowledge areas, organising the material around them (text, images, video, 3D models, source code, web pages), connecting it all together, and turning it into finished projects.

Your data is a folder of Markdown notes and attachments ([vault format](docs/spec/vault-format.md)); Graphite keeps a rebuildable index next to it. The plan is in [`ROADMAP.md`](ROADMAP.md) and [`docs/plan/`](docs/plan/); decisions in [`docs/decisions/`](docs/decisions/).

## Status

Milestone **M0 (foundations)** with the first slice of **M1**: a desktop app that opens a vault, lists notes, edits them in a rich Markdown editor with an insert toolbar and `/` menu, links notes with `[[`, shows backlinks and unlinked mentions, and searches the vault. Plus a CLI over the same core.

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
```

Open `fixtures/sample-vault` (or any folder) from the app's welcome screen. Shortcuts: `Ctrl/Cmd+K` search, `Ctrl/Cmd+N` new note, `/` in a note for blocks, `[[` to link.

## Licence

MIT, see `LICENSE`.
