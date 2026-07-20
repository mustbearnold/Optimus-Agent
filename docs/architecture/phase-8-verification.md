# Phase 8 verification — 2026-07-18

## Scope delivered

Per ADR-0010 — extractive context compression:

| Piece | Behavior |
|---|---|
| `CompressionConfig` | max_message_chars=48k, keep_tail=8, enabled |
| Algorithm | Keep leading System; fold middle into `[context_compressed N messages]` extractive summary; keep tail |
| When | Each model step + before session save |
| `TurnResult.compressed` | Observability flag |
| No aux LLM | Offline, deterministic |

## Gates

| Gate | Result |
|---|---|
| fmt / clippy `-D warnings` | pass |
| `cargo test --workspace` | **46 passed** |
| doctor | phase 8 context-compression |

### New tests
- compress unit: no-op under threshold; middle fold keeps system+tail
- kernel integration: bloated history compresses before model call

## Exceeds Hermes (this slice)
Same compression *class* as Hermes (stable system prefix, fold middle), shipped as a pure function with tests — no Python god-module, works offline without aux model cost.

## Not yet
- Token-accurate budgets / provider tokenizer
- LLM summarizer option
- Prompt-cache breakpoint APIs
