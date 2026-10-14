# ADR-002 · The vault is plain files; the database is a rebuildable index

- **Status:** accepted
- **Date:** 2026-10-14
- **Relates to:** Q2 in `docs/plan/04-open-questions.md`; A1, A5, E1

## Context

Jerred's requirement: most content must stay human-readable so that a major failure of the software never takes the data with it. Specialised tools (CAD, 3D, vector editors) also need to open and save files in place.

## Decision

A vault is a folder of Markdown notes with YAML front-matter plus an `attachments/` tree of ordinary files. That folder is the source of truth. Graphite keeps a SQLite index (full-text, links, properties, embeddings), CRDT state and derived files (thumbnails, transcripts, previews) under `.graphite/`, all of which can be deleted and rebuilt from the files. The on-disk format is specified in `docs/spec/vault-format.md`; Graphite never writes a note in a form that a plain text editor cannot read.

## Consequences

- Easier: backups, interoperability, trust, "open in" workflows, recovery.
- Harder: the reconciler between disk, CRDT and index is the hardest piece of core and must handle external edits as a first-class writer. Rename/move handling relies on front-matter IDs.
- Rule: any feature that would require a non-text source of truth for note content is rejected or redesigned.
