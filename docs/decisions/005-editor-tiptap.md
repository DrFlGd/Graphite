# ADR-005 · TipTap/ProseMirror editor that reads and writes Markdown

- **Status:** accepted
- **Date:** 2026-10-14
- **Relates to:** Q10 in `docs/plan/04-open-questions.md`; C1, C11, C12

## Context

The note interface has to be intuitive and robust. Jerred's explicit requirement: tables, code blocks, embeds and other block types must be easy to add and change, with at least a graphical/iconographic way of inserting them, not only Markdown syntax. The on-disk format stays Markdown (ADR-002).

## Decision

TipTap (ProseMirror) is the editor. Markdown is parsed to a ProseMirror document on load and serialised back on save; a raw-Markdown toggle shows the source. Every block type is a single registered module in `packages/ui/src/editor/blocks/` declaring its schema, Markdown round-trip, icon, label, shortcut and insert command; the icon toolbar, the `/` slash menu and the keyboard shortcuts are all generated from that registry, so adding a block type is one file.

## Consequences

- Easier: WYSIWYG editing of tables, embeds and code; one place to add block types; mobile reuses the same component.
- Harder: Markdown round-tripping must be lossless for everything Graphite writes and tolerant of everything it reads; unknown syntax is preserved verbatim in a raw block rather than dropped.
- Test rule: every block type ships with a Markdown → doc → Markdown round-trip test.
