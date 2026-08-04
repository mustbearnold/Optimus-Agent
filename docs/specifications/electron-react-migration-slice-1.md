---
doc_id: specifications-electron-react-migration-slice-1
doc_type: history
plane: history
status: historical
authority: historical
summary: Historical scaffold record superseded by the React workbench cutover; the Electron transport itself was retired with Electron on 2026-08-05.
reviewed_on: 2026-07-31
review_by: never
knowledge_type: specification
covers:
  - apps/optimus-desktop/src/main.rs
  - apps/optimus-tauri/src/**
  - apps/optimus-ui/**
  - docs/contracts/desktop-ipc-methods.md
depends_on:
  - docs/decisions/0028-electron-react-shell-rust-host.md
validated_by:
  - cargo test -p optimus-desktop --bin optimus-desktop
  - apps/optimus-desktop/e2e/**
---

# Electron + React migration — Slice 1 host + Electron + UI scaffold

## Current disposition

**Confirmed current behaviour:** this scaffold slice is complete and has been
superseded by
`docs/specifications/react-workbench-electron-preview-cutover.md` for current
renderer, preload, native Browser, responsive, motion, accessibility, and
verification authority. It remains as the historical bootstrap specification.

## Acceptance

| # | What | Done when |
|---|---|---|
| 1 | ADR-0028 | present |
| 2 | IPC inventory | `docs/contracts/desktop-ipc-methods.md` |
| 3 | `--host-only` | loopback host without wry window |
| 4 | Electron app | spawns host, loads UI |
| 5 | React scaffold | Vite app with typed IPC client + shell |

## Run (dev)

```bash
cargo build -p optimus-desktop
bun run --cwd apps/optimus-ui dev
bun run --cwd apps/optimus-tauri dev
```
