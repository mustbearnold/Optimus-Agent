# Phase 7 verification — 2026-07-18

## Scope delivered

Per ADR-0009 — durable kernel chat sessions:

| Piece | Behavior |
|---|---|
| `sessions.db` | SQLite WAL under Optimus home |
| `Kernel::open_session` | create or resume by UUID |
| Auto-save | after successful turn (and on max_steps trip) |
| Pack restore | `CapabilitySession::restore_loaded` |
| Auto-title | first 48 chars of first user message |
| CLI | `--session <uuid>` on chat/chat-offline; `optimus sessions` |

## Gates

| Gate | Result |
|---|---|
| fmt / clippy `-D warnings` | pass |
| `cargo test --workspace` | **43 passed** (+2 session tests) |
| CLI resume smoke | msgs 3→5 after second turn same session |
| doctor | phase 7 durable-sessions |

### Evidence
```text
session decd9775-... first turn → msgs=3
resume same id second turn → msgs=5 packs=[core]
```

## Exceeds Hermes (this slice)
Chat continuity matches the durable-jobs story: process death does not wipe the conversation or pack waist state.

## Not yet
- Context compression / FTS session search
- Gateway multi-user session routing
