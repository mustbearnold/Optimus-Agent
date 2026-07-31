---
doc_id: decisions-0012-kernel-effectors
doc_type: decision
plane: decision
status: current
authority: record
summary: Superseded in part by ADR-0016 — 2026-07-20
reviewed_on: 2026-07-31
review_by: 2026-10-31
---

# ADR-0012: Kernel effectors — terminal + browser stubs

## Status

Superseded in part by ADR-0016 — 2026-07-20

## Context

Phase 9 added Codex OAuth. Live ChatGPT backend requires SSE `stream=true` plus Cloudflare originator headers. Core pack listed `terminal` / browser tools but kernel only implemented memory/skills/fs/activate.

## Decision

1. **Codex live wire**
   - Always `stream: true` on Responses requests
   - Headers: `originator: codex_cli_rs`, UA `codex_cli_rs/…`, `ChatGPT-Account-ID` from JWT claim
   - Parse SSE (`response.output_text.delta`, `function_call` items)
2. **`terminal` tool**
   - Creates durable `RunCommand` job
   - Originally issued a kernel session `grant_approval`; ADR-0016 now requires SmartDeny to leave model-originated command jobs awaiting explicit approval
   - Returns job status JSON
3. **Browser pack tools**
   - `browser_navigate` / `snapshot` / `click` return structured “not implemented” stubs
   - Pack activation still works (schema waist)

## Non-goals

- Real browser CDP/playwright
- Capturing command stdout into tool result (job status only for now)
