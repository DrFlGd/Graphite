# ADR-001 · Tauri 2 with a Rust core and a TypeScript/React UI

- **Status:** accepted
- **Date:** 2026-10-14
- **Relates to:** Q1, Q11 in `docs/plan/04-open-questions.md`; J1, J2

## Context

Graphite needs a desktop app on macOS, Windows and Linux, a phone app on iOS and Android, a CLI, and a browser clipper, all sharing one data model, index, sync engine and set of importers. Heavy work (file watching, SQLite, CRDT merging, running yt-dlp/git/ffmpeg, transcription) belongs in native code; the rich UI pieces (Markdown editor, graph, whiteboard, 3D preview) have their best off-the-shelf implementations in web technology.

## Decision

One repository. A Rust core crate (`core/`) owns the vault model, index, query engine, importers, jobs and sync. Tauri 2 wraps it for desktop and mobile; the UI is TypeScript + React in `packages/ui`, shared by `apps/desktop` and `apps/mobile`. Platform-specific pieces (share extensions, widgets) are small native shims. The CLI (`apps/cli`) links the core crate directly.

## Consequences

- Easier: one implementation of every correctness-critical piece; small binaries; native file-system access; a single UI codebase.
- Harder: Tauri's mobile side is younger than its desktop side; some iOS/Android polish needs Swift/Kotlin. Linux desktop builds need webkit2gtk system packages; CI must install them.
- Revisit: if Tauri mobile blocks M4, the fallback is the same Rust core behind a React Native shell (UniFFI bindings), keeping `packages/ui` where possible.
