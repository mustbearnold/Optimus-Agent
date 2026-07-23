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
