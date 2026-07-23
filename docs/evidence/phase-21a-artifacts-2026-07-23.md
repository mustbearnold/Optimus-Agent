# Phase 21A Artifacts store — evidence

**Date:** 2026-07-23

## Validation

```text
cargo test -p optimus-kernel --lib artifacts
  → 3 passed

cargo test -p optimus-desktop method_registry
  → 1 passed

cargo test --workspace
  → 0 FAILED

python scripts/check-parity-ledger.py
  → parity-ledger ok capabilities=51 missing=23 parity=10 partial=14 win=4
```

## Behaviour proven

- Content-addressed put under `{home}/artifacts/blobs/<aa>/<sha256>`
- Idempotent re-put of same bytes
- Newest-first unique list from JSONL index
- IPC `artifacts_list` / `artifacts_put_text`
- Browser screenshots best-effort published via `maybe_publish_browser_screenshot`

## Ledger

- `artifacts.store-ui` → partial
- `browser.cdp`, `browser.annotations` → partial (Phase 20 evidence)
