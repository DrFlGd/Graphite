# ADR-003 · Automerge for sync

- **Status:** accepted
- **Date:** 2026-10-14
- **Relates to:** Q3, Q4 in `docs/plan/04-open-questions.md`; I1–I3

## Context

Multiple instances (desktops, phones) must converge on concurrent edits without conflict dialogs, and must work through a dumb transport (a shared folder) as well as a relay. The core is Rust (ADR-001).

## Decision

Per-note Automerge documents (text + property map) stored under `.graphite/crdt/`, exchanged as change files for folder-based sync (v1) and over a WebSocket relay (v2). Note text on disk is derived from the CRDT and external edits are applied to the CRDT as a diff from a virtual "disk" peer.

## Consequences

- Easier: a Rust-native CRDT with a stable binary format; no server needed for v1.
- Harder: the note storage layer must be CRDT-compatible from M1 even though sync ships in M4; document growth needs periodic compaction.
- Revisit at M4: Loro, if its ecosystem has matured, for performance on large notes.
