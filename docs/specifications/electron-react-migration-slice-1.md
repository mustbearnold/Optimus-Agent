---
knowledge_type: specification
status: active
covers:
  - apps/optimus-desktop/src/main.rs
  - apps/optimus-electron/**
  - apps/optimus-ui/**
  - docs/contracts/desktop-ipc-methods.md
depends_on:
  - docs/decisions/0028-electron-react-shell-rust-host.md
validated_by:
  - cargo test -p optimus-desktop --bin optimus-desktop
  - apps/optimus-electron package scripts
---

# Electron + React migration — Slice 1 host + Electron + UI scaffold

## Acceptance

| # | What | Done when |
|---|---|---|
| 1 | ADR-0028 | present |
| 2 | IPC inventory | `docs/contracts/desktop-ipc-methods.md` |
| 3 | `--host-only` | loopback host without wry window |
| 4 | Electron app | spawns host, loads UI |
| 5 | React scaffold | Vite app with typed IPC client + shell shell |

## Run (dev)

```bash
cargo build -p optimus-desktop
cd apps/optimus-electron && npm install && npm run dev
```
