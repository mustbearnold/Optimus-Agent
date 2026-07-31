---
doc_id: architecture-phase-5-verification
doc_type: history
plane: history
status: historical
authority: historical
summary: Per ADR-0007 — crate optimus-kernel:
reviewed_on: 2026-07-31
review_by: never
---

# Phase 5 verification — 2026-07-18

## Scope delivered

Per ADR-0007 — crate `optimus-kernel`:

| Piece | Behavior |
|---|---|
| `ModelProvider` | Vendor-agnostic `complete(request) -> response` |
| `ScriptedModel` | Offline/test script of tool calls + text |
| `Kernel::turn` | User message → model loop → tools → final text |
| Tools | `memory_recall`, `skill_resolve`, `activate_pack`, `write_file`/`read_file` |
| Pack waist | Only loaded pack tools offered each step |
| Limits | `max_steps` (default 8) trips cleanly |
| Memory | Inform-only recall; fence in EvidencePacket |
| Jobs | `write_file` creates durable one-node job via Runtime |

CLI: `optimus chat-offline [--demo-memory] "…"`

## Gates

| Gate | Result |
|---|---|
| fmt | pass |
| clippy `-D warnings` | pass |
| `cargo test --workspace` | **35 passed** |
| doctor | phase 5 kernel-turn; core_schema_tokens=1070 |
| chat-offline --demo-memory | recall → "you prefer helix" |

### Kernel tests (5)
- memory recall then answer
- activate_pack grows tools/tokens
- skill_resolve body
- write_file durable job
- max_steps circuit

## Exceeds Hermes (this slice)
Hermes couples the loop to a large Python monolith and ships heavy tool schemas by default. Optimus Kernel is a thin Rust FSM over `ModelProvider`, with progressive pack tools and offline-proven assembly of memory/skills/jobs.

## Not yet
- Real OpenAI/Anthropic/xAI adapters
- Streaming / prompt-cache breakpoints
- Full tool parity / browser pack handlers
- Subagent delegation inside turn
