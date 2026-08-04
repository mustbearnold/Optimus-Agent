---
doc_id: architecture-phase-1-verification
doc_type: history
plane: history
status: historical
authority: historical
summary: Historical record for Phase 1 verification — 2026-07-18; retained for provenance and excluded from default retrieval.
reviewed_on: 2026-07-31
review_by: never
---

# Phase 1 verification — 2026-07-18

## Scope delivered

Per ADR-0003:

1. **SmartDeny policy** — `RunCommand` requires durable approval grant; file effects auto-run.
2. **Job budgets** — `max_steps`, `max_consecutive_failures`, `command_timeout_ms` durable on job rows (schema v2).
3. **Bounded subprocess** — timeout + `KillOnDrop` child guard.
4. **Multi-job resume** — `resume_all` recovers running→interrupted then finishes resumable jobs.
5. **CLI** — `optimus resume-all`, doctor reports phase 1.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all --check` | pass |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | pass |
| `cargo test --workspace` | **8 passed** |
| `optimus doctor` | phase 1 policy+budgets |
| `optimus demo` | Succeeded |

### Tests

- Phase 0 crash-resume (2)
- `run_command_denied_without_approval_in_smart_deny`
- `run_command_succeeds_after_grant`
- `max_steps_budget_trips_circuit`
- `command_timeout_kills_long_sleep` (~0.5s wall, kills 30s sleep)
- `resume_all_recovers_multiple_interrupted_jobs`
- store schema version = 2

## Not yet (Phase 2+)

- MetaMemory claims / evidence packets
- LLM turn loop / providers
- Tauri desktop
- Gateway adapters
- Windows Job Object process trees (best-effort kill of direct child only in Phase 1)
