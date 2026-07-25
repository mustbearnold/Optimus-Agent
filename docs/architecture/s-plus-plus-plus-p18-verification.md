# S+++ P18 verification — durability / crash safety

Date: 2026-07-25  
Planes: program **P18** · decision (process; local SQLite durability scope) · delivery **PR pending**

## Exit evidence

| Microtask | Evidence |
|---|---|
| Y1 Doctor inventory | `apps/optimus-cli/src/doctor.rs`; `optimus doctor --json`; `tests/doctor_durability.rs` |
| Y2 Backup contract | `docs/architecture/durability-and-backup.md`; `optimus doctor backup-list` |
| Y3 Chaos WriteFile/command | `crates/optimus-runtime/tests/crash_resume.rs` (write crash + ambiguous command); campaign crash recover in `campaign.rs` tests |
| Y4 Workflow cancel matrix | `workflow_run::durability_tests::double_request_cancellation_is_idempotent` |
| Y5 Session multi-link repair | `session_resume` multi-link repair test |
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
