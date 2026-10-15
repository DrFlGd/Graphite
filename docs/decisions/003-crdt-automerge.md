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

## Amendment (M4, 2026-10-15)

Built as decided, with these refinements (see `docs/spec/vault-format.md`, "Sync"):

- Documents live under `.graphite/sync/<device-id>/<note-id>.automerge`, one folder per device, each device writing only its own. Whole documents are published, not change files: a sync client then never has to deliver a sequence in order, and a device joining late reads one file per note.
- The "virtual disk peer" is realised as deterministic recording: a changed file is diffed against the document state it was last reconciled at (or the closest recent state, for a file the sync client replaced with another device's copy) by an actor derived from that state and the content, so two devices recording the same file produce the same change. Which content hashes a device has written or recorded is kept beside its documents in `seen.json`, outside the CRDT, so it never perturbs the heads that determinism depends on.
- Liveness is a `state` string carrying the content hash rather than a boolean, because Automerge drops a put of an unchanged value; a delete racing an edit is then a visible conflict, resolved for the edit.
- Automerge 0.12 was kept; Loro was not needed. Compaction is still open.
- The relay (transport v2) moves the same per-device document folders and the assets, as encrypted objects named by an HMAC of their path, so the relay stores only ciphertext and the device code path is the one the folder transport already exercises. Pairing is a pasted invite (relay URL plus key); QR pairing (I5) was cut as planned.
