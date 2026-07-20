# Desktop streaming (desktop-2) — 2026-07-19

## What shipped

End-to-end **token streaming** for daily chat:

| Layer | Behavior |
|---|---|
| `StreamEvent` | `TextDelta` · `ToolStatus` · `Status` |
| `ModelProvider::complete_streaming` | default one-shot; overrides stream |
| `ScriptedModel` | ~12-char chunks (UI/Playwright) |
| `CodexOAuthModel` | live SSE line reader → delta sink |
| `Kernel::turn_with_sink` | forwards model + tool events |
| HTTP | `POST /api/chat/stream` (SSE) |
| WebView | `chat_stream` IPC + `__optimusStream` pushes |
| UI | progressive bubble + caret while streaming |

## Verification

```text
cargo test --workspace -- --test-threads=1   # all green
cd apps/optimus-desktop && npx playwright test
  7 passed (3.6s)
```

Includes:
- Enter streams offline reply progressively
- SSE endpoint emits `delta` then `done`

## Run

```bash
# native window (streams via WebView IPC)
cargo run -p optimus-desktop

# Playwright / browser
cargo run -p optimus-desktop -- --http 8787
cd apps/optimus-desktop && npx playwright test
```

## Daily-use status (updated)

| Need | Status |
|---|---|
| Multi-turn sessions | yes |
| Sidebar sessions | yes |
| Enter-to-send | yes (Playwright) |
| Live Codex OAuth | yes |
| **Streaming tokens** | **yes** |
| HTTP e2e harness | yes |
| Gateway / cron / browser agent | no |

Still not a full Hermes OS — but local chat is now usable with progressive replies.
