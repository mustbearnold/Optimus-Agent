---
doc_id: decisions-0010-context-compression
doc_type: decision
plane: decision
status: current
authority: record
summary: Decision record for ADR-0010: Context compression for durable sessions, including its context, consequences, and current documentary status.
reviewed_on: 2026-07-31
review_by: 2026-10-31
---

# ADR-0010: Context compression for durable sessions

## Status

Accepted — 2026-07-18

## Context

Phase 7 made sessions durable; long multi-turn tool traces will blow context. Hermes uses threshold compression with a stable system prefix for prompt caching. Optimus needs the same discipline without an aux LLM in Phase 8.

## Decision

1. **`CompressionConfig`** on `KernelConfig`
   - `max_message_chars` (default 48_000)
   - `keep_tail_messages` (default 8 non-system messages)
   - `enabled` (default true)
2. **Extractive compressor** (no model call)
   - Always preserve leading `Role::System` message(s)
   - When total estimated chars exceed threshold, fold the middle into one synthetic user message:
     `[context_compressed N messages]` + bullet lines `role: snippet…`
   - Preserve the newest `keep_tail_messages` intact
3. **When**
   - Run at the start of each model step inside `turn`, after user message append / tool results
   - Persist compressed transcript via existing session save
4. **Observability**
   - `TurnResult.compressed: bool` if any compression ran this turn
   - Compression marker string is stable for tests

## Non-goals

- LLM summarizer / aux model
- Token-accurate counting (char estimate is enough for v1)
- Prompt-cache breakpoint APIs

## Consequences

- Long sessions remain operable offline
- Lossy middle is explicit and recoverable from session history only via the summary (full history not retained once compressed — same class of tradeoff as Hermes compression)
