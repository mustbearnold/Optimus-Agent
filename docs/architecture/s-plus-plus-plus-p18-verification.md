---
doc_id: architecture-s-plus-plus-plus-p18-verification
doc_type: history
plane: history
status: historical
authority: historical
summary: Date: 2026-07-25 Planes: program P18 · decision (process; local SQLite durability scope) · delivery PR #28
reviewed_on: 2026-07-31
review_by: never
---

# S+++ P18 verification — durability / crash safety

Date: 2026-07-25  
Planes: program **P18** · decision (process; local SQLite durability scope) · delivery **PR #28**

## Exit evidence

| Microtask | Evidence |
|---|---|
| Y1 Doctor inventory | `apps/optimus-cli/src/doctor.rs`; `optimus doctor --json`; `tests/doctor_durability.rs` |
| Y2 Backup contract | `docs/architecture/durability-and-backup.md`; `optimus doctor backup-list` |
| Y3 Chaos WriteFile/command | `crash_resume`: pre-effect crash seam + resume terminal uniqueness; ambiguous command non-replay; campaign crash recover (existing) |
| Y4 Workflow/agent terminals | cancel-request idempotence unit test; pre-existing `workflow_dag` / `agent_contracts` terminal uniqueness; vertical cancel fan-out |
| Y5 Session multi-link repair | multi-link repair for `write_file` effect links; repair-on-open is link-driven (not tool-name-specific) |
| Y6 Durability **S+++** | architecture-marks + this file |

## Commands

```bash
cargo test -p optimus-cli --test doctor_durability -- --test-threads=1
cargo test -p optimus-runtime --test crash_resume -- --test-threads=1
cargo test -p optimus-kernel --test session_resume -- --test-threads=1
cargo test -p optimus-workflow --lib durability_tests -- --test-threads=1
python3 scripts/check-architecture-marks.py
```

## Grade moves

| Mark | Before | After |
|---|---|---|
| Durability / crash safety | A+ | **S+++** |

## Explicit non-claims

- External messaging exactly-once across off-box channels is **out of scope**.
- Multi-DB homes are not one distributed transaction; backup is multi-file under one home.
