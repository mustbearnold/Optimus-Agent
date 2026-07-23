# optimus-electron

T3 Code–style Electron shell for Optimus. Durable work still runs in the **Rust host**
(`optimus-desktop --host-only`).

## Prerequisites

```bash
cargo build -p optimus-desktop
cd apps/optimus-electron && npm install
cd ../optimus-ui && npm install   # for React UI
```

## Run

```bash
# Legacy assembled HTML (from Rust host GET /)
cd apps/optimus-electron
npm run dev:legacy-html

# React SPA (Vite) + host
# terminal A is started by dev script when OPTIMUS_UI_AUTOSTART is on:
npm run dev:ui
```

Environment:

| Var | Meaning |
|---|---|
| `OPTIMUS_HOST_PORT` | Host port (default `17865`) |
| `OPTIMUS_HTTP_TOKEN` | Optional; host mints one if unset |
| `OPTIMUS_HOME` | Optimus data home |
| `OPTIMUS_ELECTRON_UI` | `legacy` or `react` |
| `OPTIMUS_HOST_EXTERNAL=1` | Do not spawn host; use existing |

## Architecture

```text
Electron main → spawn optimus-desktop --host-only
             → BrowserWindow → legacy HTML or React (Vite)
preload       → window.optimusElectron (chrome helpers)
React/legacy  → POST /api/ipc + /api/chat/stream (Bearer token)
```
