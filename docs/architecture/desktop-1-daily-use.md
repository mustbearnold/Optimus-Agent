# Desktop daily-use path (desktop-1) — 2026-07-18

## Honest status

**Still not a full Hermes replacement** (no messenger gateway, cron UI, browser automation, skill editor).

**Now usable for local daily chat** if Codex OAuth is imported:

| Capability | Status |
|---|---|
| Real multi-turn sessions (SQLite) | yes |
| Sidebar = real sessions list | yes |
| Resume prior session | yes |
| Live **Codex** chat (SSE OAuth) | yes (default provider) |
| OpenAI-compatible API key chat | yes |
| Offline echo / memory demo | yes |
| Import Codex from Hermes (read-only) | yes (button + CLI) |
| Non-blocking chat (UI thread) | yes (worker thread) |
| Light/dark theme | yes |
| Terminal/tools in loop | yes when model calls tools |
| Browser effector | stub only |
| Gateway / Telegram / cron UI | no |
| Streaming tokens to UI | no (full turn then paint) |

## Run

```bash
cargo run -p optimus-desktop
```

1. Click **Import Codex** (or `optimus auth codex import-hermes` with same home)
2. **New session**
3. Provider = **gpt-5.4 · Codex**
4. Chat

Home: `%LOCALAPPDATA%/optimus`

## Build

`cargo build -p optimus-desktop` — green after this slice.
