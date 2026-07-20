# Phase 9 verification — 2026-07-18

## Scope delivered

Per ADR-0011 — OpenAI Codex OAuth for Optimus:

| Piece | Behavior |
|---|---|
| `CodexAuthStore` | Optimus-owned `{home}/auth.json` (never writes Hermes/CLI) |
| Import Hermes | Read-only from `%LOCALAPPDATA%/hermes/auth.json` |
| Import CLI | Read-only from `~/.codex/auth.json` |
| Refresh | `auth.openai.com/oauth/token` + JWT exp skew 120s |
| Device login | OpenAI deviceauth usercode/token + code exchange |
| `CodexOAuthModel` | Responses API `POST {base}/responses` |
| CLI | `auth codex status\|import-hermes\|import-cli\|login\|logout` |
| Chat | `optimus chat --provider codex "…"` |

## Gates

| Gate | Result |
|---|---|
| fmt / clippy `-D warnings` | pass |
| `cargo test --workspace` | **54 passed** |
| doctor | phase 9 codex-oauth + codex_oauth status line |
| import-hermes smoke | `present=true has_refresh=true mode=import:hermes` |

### New tests
- Unit: Hermes provider/pool extract, Responses map/parse, JWT opaque
- Integration: store roundtrip, mock Responses HTTP, import helpers

## Live use

```bash
optimus auth codex import-hermes   # or import-cli / login
optimus auth codex status          # no secrets printed
optimus chat --provider codex "hello"
# model override:
OPTIMUS_CODEX_MODEL=gpt-5.4 optimus chat --provider codex "…"
```

## Exceeds Hermes (this slice)
Same Codex OAuth wire as Hermes, but:
- Separate Optimus token file (no shared-write races with Hermes)
- Explicit import + device login
- Provider sits behind the same `ModelProvider` + pack waist + sessions + compression stack

## Not yet
- Live end-to-end chat against ChatGPT (needs network + valid token at run time)
- Credential pool rotation
- Full Responses reasoning/encrypted_content replay
