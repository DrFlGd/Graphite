# ADR-004 · MIT licence

- **Status:** accepted
- **Date:** 2026-10-14
- **Relates to:** Q7, Q9 in `docs/plan/04-open-questions.md`

## Context

A licence had to be chosen before third-party libraries are pulled in, since it constrains which ones are compatible.

## Decision

MIT, for now. `LICENSE` at the repo root.

## Consequences

- Easier: maximal compatibility with the libraries the plan recommends; contributions are frictionless.
- Watch: tldraw (Q9, M5) ships under its own licence with a watermark clause for commercial use; verify terms or use Excalidraw when M5 starts. Any dependency with a copyleft licence (GPL/AGPL) is excluded from core.
- Can be changed later: "for now" means re-evaluating before public release (M8).
