---
doc_id: evidence-product-complete-p25-hold-2026-07-25
doc_type: evidence
plane: evidence
status: historical
authority: record
summary: Planes: program P25 · delivery PR #35 · architecture hold
reviewed_on: 2026-07-31
review_by: never
---

# Product-complete program P25 hold — 2026-07-25

Planes: **program P25** · delivery **PR #35** · architecture hold

## Board

Three-expert review (security-export / product-ledger / correctness) →
**APPROVE-WITH-FIXES** (security + correctness initially **BLOCK** on export confinement).

### MUST-FIX applied

1. Export writes **only** under `{home}/artifacts/exports/` (basename optional; absolute paths refused)
2. Secret basenames expanded (auth.json, .netrc, .pem, …)
3. Zip `count` is post-dedupe entry count
4. `cron_set_enabled` fails closed on unknown id
5. Scorecard “gallery incomplete” + cron capability “delivery” wording cleaned

## Commands (green after fixes)

```text
cargo test -p optimus-artifacts --lib
cargo test -p optimus-ops --lib -- pause_resume
cargo test -p optimus-desktop
npm test ArtifactsSurface CronWorkbench
python3 scripts/check-desktop-ipc-matrix.py
python3 scripts/check-parity-ledger.py
```

## Ledger

- `artifacts.store-ui` → parity
- `cron.lifecycle` → parity

## Non-claims

- Native OS save-as dialog (exports dir + openPath)
- Zip compression beyond store method
- Hermes gate PASS

## Verdict

**program P25 closed after review board fixes.** Next: **program P26**.
