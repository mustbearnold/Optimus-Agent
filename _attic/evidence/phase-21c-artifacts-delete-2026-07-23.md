---
doc_id: evidence-phase-21c-artifacts-delete-2026-07-23
doc_type: evidence
plane: evidence
status: historical
authority: record
summary: Bounded evidence record for Phase 21C Artifacts delete + filter — evidence; it does not override current product authority.
reviewed_on: 2026-07-31
review_by: never
---

# Phase 21C Artifacts delete + filter — evidence

**Date:** 2026-07-23

## Behaviour

- `ArtifactStore::delete` appends a tombstone and removes the blob; the index stays append-only
- IPC `artifacts_delete`
- UI: filter by label/source/sha; Delete on preview (confirm)
- Cross-process shared/exclusive locks serialize index and blob operations
- Blob publication is unique-temp + no-clobber hard-link, and reads verify bounded size and SHA-256
- Symlinked state/blob paths and oversized base64 fail closed
- Text previews truncate on UTF-8 character boundaries

## Validation

```text
cargo test -p optimus-kernel --lib artifacts
cargo test -p optimus-desktop artifacts_
cargo test -p optimus-desktop method_registry
python scripts/check-parity-ledger.py

# Additional hardening evidence
artifact concurrency stress: 20/20 passed
optimus-kernel artifacts: 10 passed
Unicode artifact IPC preview: 1 passed
```
