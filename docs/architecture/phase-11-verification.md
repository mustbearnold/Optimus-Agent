# Phase 11 verification — 2026-07-18

## Scope delivered

Per ADR-0013 — command stdout/stderr capture:

| Piece | Behavior |
|---|---|
| `CommandCapture` | stdout, stderr, exit_code, truncate flags, timed_out |
| Pipe readers | Background threads, 32 KiB/stream cap |
| Durability | `command_output` event on job ledger |
| Kernel `terminal` | Tool JSON includes stdout/stderr/exit_code |
| Failures | Non-zero exit & timeout still emit capture |

## Gates

| Gate | Result |
|---|---|
| clippy `-D warnings` | pass |
| `cargo test --workspace -- --test-threads=1` | **59 passed** |
| doctor | phase 11 command-capture |

### New tests
- `command_capture`: success stdout contains `capture-me`; nonzero exit keeps capture
- kernel terminal tool message contains `hello-capture`
- phase1 timeout accepts `CommandFailed { timed_out: true }`

## Exceeds Hermes (this slice)
Durable job ledger stores command output as events (crash-resumeable audit), not only ephemeral tool messages.
