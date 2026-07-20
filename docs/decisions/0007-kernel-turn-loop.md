# ADR-0007: Provider-agnostic Kernel turn loop

## Status

Accepted — 2026-07-18

## Context

Phases 0–4 built durable jobs, policy, MetaMemory, skills, and packs. Phase 5 must assemble them into a single **conversation turn loop** without locking to one LLM vendor — and remain offline-testable.

## Decision

1. **Crate `optimus-kernel`**
   - `ModelProvider` trait: `complete(CompletionRequest) -> CompletionResponse`
   - Messages: system / user / assistant / tool
   - `Kernel::turn(user_text) -> TurnResult`
2. **Tool waist from `CapabilitySession`**
   - Only loaded pack tools are offered to the model each step
   - Built-in handlers: `activate_pack`, `memory_recall`, `skill_resolve`, `job_write_file`
3. **Loop**
   - max model steps per turn (default 8)
   - tool results appended; final assistant text ends turn
   - pack activation is an explicit tool (segment boundary), not silent
4. **Offline**
   - `ScriptedModel` for tests / `optimus chat-offline`
   - No network providers in Phase 5
5. **Security**
   - memory recall uses Inform purpose only (never ActionAuthorize)
   - job_write_file uses Runtime under SmartDeny (file effects auto-allowed)
   - activate_pack respects pack budget errors surfaced as tool errors

## Non-goals

- Streaming, prompt caching breakpoints, real OpenAI/Anthropic adapters
- Full Hermes tool parity
- Subagent delegation inside turn

## Public seam

```text
Kernel::open(home) / Kernel::with_parts(...)
Kernel::turn(&mut dyn ModelProvider, user) -> TurnResult
```
