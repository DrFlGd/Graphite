# Graphite Roadmap

This is the top-level plan for Graphite. It was reviewed and accepted on 2026-10-14 (see `docs/plan/04-open-questions.md` for the answers and `docs/decisions/` for the ADRs). The **Decision** columns in the feature and milestone docs stay in place so individual items can still be accepted, cut or reprioritised as development proceeds.

The detail behind each section lives in `docs/plan/`:

| Doc | What it covers |
| --- | --- |
| [00-vision.md](docs/plan/00-vision.md) | What Graphite is for, who it is for, what it is not, guiding principles |
| [01-features.md](docs/plan/01-features.md) | Full feature catalogue: the requested features, plus suggested additions, each with a priority and a decision slot |
| [02-architecture.md](docs/plan/02-architecture.md) | Technical approach: data model, storage, sync, platforms, plugin surface, recommended stack |
| [03-milestones.md](docs/plan/03-milestones.md) | Milestones M0 to M8 with scope, exit criteria and what can be cut |
| [04-open-questions.md](docs/plan/04-open-questions.md) | Decisions that need an answer before or during M0 |
| [docs/decisions/](docs/decisions/) | Architecture decision records (ADRs), one per settled question |

## One-paragraph summary

Graphite is a local-first, cross-platform (desktop first, phone second) second brain. Everything is stored in a plain-files **vault** (Markdown notes plus an `attachments/` folder) with a SQLite index alongside it, so the data stays readable by other tools and specialised software can work on the files directly. On top of that vault sit four things: **rapid capture** from anywhere, **connected documentation** (links, backlinks, tags, properties, graph), **project tracking** (tasks, subtasks, gaps and information needs), and **multiple views** of the same data (task tracker, node graph, whiteboard, board, timeline). Content is pulled in from external sources (GitHub repos, web pages, videos) by **importers** that remember where it came from and can check for updates. Instances sync through a conflict-free (CRDT) layer so the phone and desktop stay consistent without a central server being required.

## Milestones at a glance

| # | Milestone | Outcome when done | Decision |
| --- | --- | --- | --- |
| M0 | Foundations | Stack chosen, vault format specified, repo scaffolded, design language agreed | |
| M1 | Capture & Connect (desktop MVP) | Create/edit notes, link them, see backlinks and a graph, quick-capture from a global hotkey | |
| M2 | Projects & Progress | Projects, tasks, subtasks, gaps/information needs, task tracker and board views | |
| M3 | Media & Importers | Attach any file, previews for images/video/PDF/3D, importers for GitHub, web pages and video with update checks | |
| M4 | Sync & Mobile | CRDT sync between instances; phone app for capture, reading and task updates | |
| M5 | Views & Whiteboard | Whiteboard/canvas, timeline, and saved queries that drive any view | |
| M6 | Integrations & Plugins | "Open in" / file-watch round-trips with CAD, 3D and vector tools; plugin API; browser clipper; share sheet | |
| M7 | Intelligence | Full-text + semantic search, ask-your-vault assistant, transcription/OCR, resurfacing | |
| M8 | Hardening & Release | Encryption at rest, backups, performance on large vaults, packaging, docs, 1.0 |

The order is the recommended one; M4 (sync/mobile) can move earlier if phone capture is more important than media importers, and M7 can be pulled forward piece by piece (full-text search in particular belongs in M1). See `03-milestones.md` for what each milestone can shed.

## Priorities used in this plan

| Priority | Meaning |
| --- | --- |
| **P0** | Without this Graphite is not the product described. Ships in M1–M3. |
| **P1** | Strongly expected; ships before 1.0. |
| **P2** | Valuable, but can follow 1.0 or be a plugin. |

## Suggested additions beyond the original brief

These are the features I added on top of the requirements in the original request; each is argued in `01-features.md`. Headline ones:

1. **Capture from anywhere**: global hotkey, phone share sheet, browser clipper, email/URL inbox, voice memo. The capture barrier is the whole game.
2. **Inbox and triage**: everything captured lands untyped in an inbox and is filed later, so capture never asks questions.
3. **Properties and typed links**: structured fields on any note (`status`, `due`, `source`) and relationship types on links (`depends-on`, `derived-from`, `contradicts`), which is what makes a node graph actually useful and lets queries drive views.
4. **Gap tracking as a first-class object**: "information needs" and "open questions" are items in their own right, link to the task they block, and surface in a per-project "what do I still need to find out" view.
5. **Saved queries → views**: any view (board, list, graph, timeline) is a query plus a layout, so the same data can be sliced per project, per status, per media type.
6. **Source records with update checks**: every imported thing keeps its origin URL, hash, and last-checked time; the app can diff a re-fetch and show "changed upstream".
7. **Transcripts and OCR**: imported video gets a transcript, images get OCR, so everything is searchable and "interrogate quickly" works across media.
8. **Ask-your-vault assistant**: semantic search and a chat-style interface scoped to a project or the whole vault, with citations back to notes, local-model capable.
9. **Version history**: per-note history and vault snapshots; cheap to add on a file-based vault and essential once sync exists.
10. **Decision log and templates**: project templates (research, build, software, writing) and a lightweight decision log so the "why" of a project is captured alongside the "what".
11. **Resurfacing / review**: periodic "you captured this three weeks ago, still relevant?" prompts to keep the vault from rotting.
12. **Plugin API + CLI**: a small, documented extension surface and a command-line tool, so integrations with niche software don't have to live in core.
13. **Encryption at rest and E2E-encrypted sync**: table stakes for a personal knowledge base that holds everything.
14. **Export and publish**: export a project (or the whole vault) to a static site, PDF or zip; the vault is already plain files so this mostly means "pretty".

## How to review this plan

- Edit the **Decision** columns in `01-features.md` and `03-milestones.md` directly in this PR, or leave review comments on the lines you want changed.
- Anything marked `cut` is removed from the plan; anything marked `later` moves to the "deferred" list at the bottom of `01-features.md`.
- The open questions in `04-open-questions.md` each have a recommended answer; confirming or overriding those is the first thing that unblocks M0.
