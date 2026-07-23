# Phase 21D Artifacts bulk delete — evidence

**Date:** 2026-07-23

## Behaviour

- `ArtifactStore::delete_many` collapses duplicates, caps at 50, single exclusive lock
- Per-sha best-effort: successes in `deleted`, failures in `failed` with error text
- IPC `artifacts_delete_many` registered on Files domain
- UI: row checkboxes + **Delete selected** on inspector and Artifacts page

## Validation

```text
cargo test -p optimus-kernel --lib artifacts -- --test-threads=1
  → 11 passed (includes delete_many_removes_batch_and_reports_missing)

cargo test -p optimus-desktop --bin optimus-desktop -- artifacts_ method_registry
  → 5 passed (includes artifacts_delete_many_clears_batch + method_registry)
```
