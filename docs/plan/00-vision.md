# 00 · Vision

## The problem

Working on anything substantial (a build, a research area, a piece of software, a creative project) means gathering material in many forms: notes, screenshots, reference images, videos, datasheets, repositories, 3D models, vector drawings, CAD files, web pages. Today that material ends up scattered across a file system, a browser's bookmarks, a notes app, a task manager and a download folder. The cost of *putting something in the right place* is high enough that most of it never gets filed, and the cost of *finding and connecting it later* is high enough that projects stall on "I know I saw something about this".

## What Graphite is

Graphite is a personal, multi-modal second brain built around three loops:

1. **Capture**: get a thought, link, file or snippet into Graphite in seconds, from any device, without being asked to organise it first.
2. **Organise and connect**: turn captured material into connected documentation around projects, ideas and knowledge areas, with linking that is as cheap as typing `[[`, and with structure (properties, tags, typed relations) that can be added incrementally rather than up front.
3. **Interrogate and build**: ask questions of the material (search, graph, queries, an assistant), see what is missing (gaps and information needs), and track the work (tasks, subtasks, progress) all the way to a finished project.

A **project** in Graphite is the unit that ties these together: a hub note, a set of linked notes and files, a task tree, a list of sources being tracked, and a set of views. Material can belong to more than one project; an **idea** is a project that has not been committed to yet; a **knowledge area** is a long-lived project with no end state.

## Who it is for

The primary user is a single person running several concurrent projects that mix physical and digital work, who already uses specialised tools (CAD, 3D modelling, vector editors, code editors) and does not want Graphite to replace them. Graphite is the connective tissue, not the editor of last resort.

Multi-user collaboration is **not** a goal for 1.0. The architecture should not forbid it (the sync layer is multi-writer by nature) but no feature is designed for it.

## Guiding principles

| Principle | What it means in practice |
| --- | --- |
| **Capture first, structure later** | No capture path asks a question. Everything lands in an inbox and can be filed, linked and typed afterwards. |
| **Your files, readable without us** | The vault is plain files on disk (Markdown + attachments). Any other tool can open, edit or back it up. Graphite's database is a rebuildable index, never the only copy. |
| **Local-first** | Works fully offline. Sync is a feature layered on top, not a dependency. No account is required to use it. |
| **One data model, many views** | A task tracker, a graph, a whiteboard and a board are all projections of the same notes and links. Changing something in one view changes it everywhere. |
| **Connections are cheap and typed** | Linking is one keystroke away everywhere; links can optionally carry a relationship type so the graph means something. |
| **Multi-modal by default** | Images, video, PDFs, 3D models, repos and web pages are first-class nodes with previews, metadata and searchable derived text, not opaque attachments. |
| **Bridges, not walls** | Specialised software is reached through "open in", file watching and importers. Graphite never traps a file in a format only it can read. |
| **Extensible core** | A small core with a plugin API and a CLI, so niche integrations don't bloat the product. |
| **Private by default** | Encryption at rest and end-to-end encrypted sync. AI features can run on local models; nothing leaves the machine without an explicit opt-in. |

## What Graphite is not

- Not a replacement for Blender, Fusion, Illustrator, VS Code or any other specialist editor.
- Not a team wiki or a collaborative document editor (for now).
- Not a cloud service you need an account for.
- Not a general file manager; it manages files *in the context of projects and ideas*.

## Success looks like

- Capturing a thought or a link from a phone takes under five seconds and zero decisions.
- Starting a new project from an idea takes one action and yields a hub, a task list and a place for files.
- "What do I still need to know to finish X?" is a single view, not a search.
- Any note or file is reachable by link, search, graph or query in under three steps.
- A video, repo or web page added by URL is downloaded, indexed and searchable without further action, and Graphite tells you when it changed upstream.
- Desktop and phone show the same vault within seconds of each other, with no data loss on conflicting edits.
