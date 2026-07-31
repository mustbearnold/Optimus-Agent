---
doc_id: architecture-phase-10-verification
doc_type: history
plane: history
status: historical
authority: historical
summary: - Import Hermes tokens into Optimus home first: auth codex import-hermes - Tokens never logged
reviewed_on: 2026-07-31
review_by: never
---

# Phase 10 verification — 2026-07-18

## Scope delivered

| Piece | Evidence |
|---|---|
| Codex SSE + originator headers | Live: `optimus-codex-ok` |
| JWT → `ChatGPT-Account-ID` | from `https://api.openai.com/auth.chatgpt_account_id` |
| `stream: true` required | ChatGPT 400 without it |
| terminal tool + SmartDeny grant | unit test green |
| browser tool stubs | unit test green |

## Live Codex smoke

```text
session f827e769-…
optimus-codex-ok
[provider=codex … steps=1 packs=["core"] …]
```

## Gates

| Gate | Result |
|---|---|
| clippy `-D warnings` | pass |
| `cargo test --workspace` | **57 passed** (prefer `--test-threads=1` if HTTP mock flakes) |
| Live chat `--provider codex` | **pass** → `optimus-codex-ok` |

## Notes

- Import Hermes tokens into Optimus home first: `auth codex import-hermes`
- Tokens never logged
