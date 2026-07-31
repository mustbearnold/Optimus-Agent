---
doc_id: decisions-0008-openai-compat-provider
doc_type: decision
plane: decision
status: current
authority: record
summary: Decision record for ADR-0008: OpenAI-compatible ModelProvider, including its context, consequences, and current documentary status.
reviewed_on: 2026-07-31
review_by: 2026-10-31
---

# ADR-0008: OpenAI-compatible ModelProvider

## Status

Accepted — 2026-07-18

## Context

Phase 5 proved `Kernel::turn` offline via `ScriptedModel`. Phase 6 must talk to real endpoints without abandoning the pack waist or SmartDeny. Hermes already supports many providers; Optimus starts with the **OpenAI Chat Completions** wire format (used by OpenRouter, many local servers, and Azure-compatible gateways).

## Decision

1. **`OpenAiCompatModel`** implements `ModelProvider`
   - Config: `base_url`, `api_key`, `model`, optional `organization`
   - POST `{base_url}/chat/completions` (base_url may already include `/v1`)
2. **Pure mappers** (unit-tested without network):
   - `to_openai_request(CompletionRequest, model) -> Value`
   - `from_openai_response(Value) -> CompletionResponse`
3. **Transport** via `ureq` (blocking, small). Fail closed on non-2xx with body snippet.
4. **Tools** map pack `ToolSchema` → OpenAI function tools with a minimal JSON Schema (`type: object`, empty properties) unless extended later.
5. **CLI**
   - `optimus chat --message ...` uses env:
     - `OPTIMUS_API_BASE` (default `https://api.openai.com/v1`)
     - `OPTIMUS_API_KEY` (required for live chat)
     - `OPTIMUS_MODEL` (default `gpt-4.1-mini`)
   - `chat-offline` remains the zero-network path
6. **No secrets in logs**; API key only in Authorization header.

## Non-goals

- Anthropic native Messages API (can add as second adapter)
- Streaming SSE
- Credential pools / OAuth

## Consequences

- Local llama.cpp / vLLM OpenAI servers work with base_url override
- Tests never hit the public internet; mock TCP server + pure parse tests
