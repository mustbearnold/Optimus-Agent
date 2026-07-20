# Desktop shell (desktop-0) — 2026-07-18

## Deliverable

`apps/optimus-desktop` — native Windows **WebView2** host (`tao` + `wry`) embedding the conversation-first UI.

| Piece | Detail |
|---|---|
| UI | Port of `docs/design/optimus-desktop-ui.html` (dark default, light toggle, 100fps motion) |
| Home | `%LOCALAPPDATA%/optimus` (override `--home`) |
| IPC | `doctor`, `sessions`, `chat_offline` via `window.ipc` → Kernel |
| Send | Composer calls live `chat_offline` when running inside desktop |

## Run

```bash
export CARGO_TARGET_DIR="E:/Projects/Optimus Agent/local/tmp/cargo-target"
export TEMP=C:/Users/mustb/AppData/Local/Temp
export TMP=C:/Users/mustb/AppData/Local/Temp
cargo run -p optimus-desktop
```

## Verified

- `cargo build -p optimus-desktop` — success (Windows)

## Not yet

- Live Codex provider from UI (offline scripted path wired; OAuth chat next)
- Sessions list → sidebar threads
- Full Tauri packaging / auto-updater
