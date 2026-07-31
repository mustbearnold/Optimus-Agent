---
doc_id: architecture-phase-6-verification
doc_type: history
plane: history
status: historical
authority: historical
summary: Per ADR-0008 — OpenAI-compatible ModelProvider:
reviewed_on: 2026-07-31
review_by: never
---

# Phase 6 verification — 2026-07-18

## Scope delivered

Per ADR-0008 — OpenAI-compatible `ModelProvider`:

| Piece | Behavior |
|---|---|
| `OpenAiCompatModel` | POST `{base}/chat/completions` via `ureq` |
| Pure mappers | `to_openai_request` / `from_openai_response` unit-tested |
| Tool mapping | Pack tools → OpenAI function tools |
| Assistant history | Kernel tool-call JSON expanded to OpenAI `tool_calls` |
| Auth | `Authorization: Bearer` only; no key logging |
| Env | `OPTIMUS_API_KEY`, `OPTIMUS_API_BASE`, `OPTIMUS_MODEL`, `OPTIMUS_API_ORG` |
| CLI | `optimus chat "…"` live; `chat-offline` unchanged |
| Tests | Local mock TCP server (no internet) |

## Gates

| Gate | Result |
|---|---|
| fmt | pass |
| clippy `-D warnings` | pass |
| `cargo test --workspace` | **41 passed** |
| doctor | phase 6 openai-compat; api_key_set=false locally |

### New tests
- 4 unit (map/parse)
- 2 HTTP mock (200 round-trip + 401 surface)
- prior kernel/pack/memory/skills/runtime still green

## Live use (optional)

```bash
export OPTIMUS_API_KEY=...
export OPTIMUS_API_BASE=https://api.openai.com/v1   # or local llama.cpp /v1
export OPTIMUS_MODEL=gpt-4.1-mini
optimus chat "hello"
```

## Exceeds Hermes (this slice)
Same OpenAI-wire breadth path, but behind a thin Rust `ModelProvider` with proven pack waist and offline mapper/HTTP tests — not a god-module provider soup.

## Not yet
- Anthropic native adapter
- Streaming
- Credential pools / OAuth
- Browser pack tool handlers
