---
doc_id: evidence-phase-21b-artifacts-gallery-2026-07-23
doc_type: evidence
plane: evidence
status: historical
authority: record
summary: Bounded evidence record for Phase 21B Artifacts gallery — evidence; it does not override current product authority.
reviewed_on: 2026-07-31
review_by: never
---

# Phase 21B Artifacts gallery — evidence

**Date:** 2026-07-23

## Behaviour

- `ArtifactStore::get_meta` / `get_base64`
- IPC `artifacts_get` returns `kind=image|text|binary` with preview payload
- Desktop sidebar + Artifacts page: click row → provenance meta + image/text preview

## Validation

```text
cargo test -p optimus-kernel --lib artifacts
cargo test -p optimus-desktop artifacts_
cargo test -p optimus-desktop method_registry
python scripts/check-parity-ledger.py
```
