---
doc_id: architecture-product-complete-p25-verification
doc_type: history
plane: history
status: historical
authority: historical
summary: Planes: program P25 · delivery PR #35 · architecture hold (Durability / Control-plane / UI / Security) · ledger artifacts.store-ui, cron.lifecycle → parity
reviewed_on: 2026-07-31
review_by: never
knowledge_type: verification
owns:
  - docs/architecture/product-complete-p25-verification.md
covers:
  - docs/plans/product-complete-program.md
depends_on:
  - docs/plans/product-complete-program.md
  - docs/decisions/0025-artifact-workbench-and-owned-presentation-state.md
validated_by:
  - crates/optimus-artifacts/src/lib.rs
  - crates/optimus-ops/src/cron.rs
  - crates/optimus-host/src/files.rs
  - crates/optimus-host/src/scheduling.rs
  - apps/optimus-ui/src/components/workspace/ArtifactsSurface.tsx
  - apps/optimus-ui/src/components/cron/CronWorkbench.tsx
  - scripts/check-desktop-ipc-matrix.py
---

# Product-complete program P25 verification

Planes: **program P25** · delivery **PR #35** · architecture hold (Durability /
Control-plane / UI / Security) · ledger `artifacts.store-ui`, `cron.lifecycle`
→ **parity**

Date: 2026-07-25

## Goal

Artifacts gallery/filters/export/zip; cron list/create/pause/resume/remove/history
in React without UI lease minting.

## What landed

| Item | Result | Evidence |
|---|:---:|---|
| Gallery + lazy image thumbs | **PASS** | ArtifactsSurface gallery mode |
| Type/label filter chips | **PASS** | UI unit test |
| Single export confined to `artifacts/exports/` | **PASS** | free-form host paths refused |
| Bulk store-method zip | **PASS** | `export_zip` count + basenames-only |
| Cron create / pause / resume / remove | **PASS** | CronWorkbench + IPC |
| Cron attempt history | **PASS** | `CronStore::history` + `cron_history` |
| Lease non-bypass | **PASS** | UI only add/set_enabled/remove/history/list; no claim APIs |
| IPC matrix | **PASS** | new methods registered |

## Residuals

| Residual | Owner |
|---|---|
| Native save-as dialog (uses host default exports dir + openPath) | Optional UX |
| Zip compression (stored method only) | Acceptable for parity |
| Automations route outside Settings | Settings embeds CronWorkbench |

## Hold suite

```bash
cargo test -p optimus-artifacts --lib
cargo test -p optimus-ops --lib -- pause_resume
cargo test -p optimus-desktop -- --test-threads=1
cd apps/optimus-ui && npm test -- ArtifactsSurface CronWorkbench
python3 scripts/check-desktop-ipc-matrix.py
python3 scripts/check-parity-ledger.py
```

## Non-claims

- Hermes gate PASS
- UI minting of cron leases
- Zip-slip capable entries (basenames only)

## Verdict

**program P25 exit: PASS** (pending three-expert board + merge).
Next: program P26 consoles or parallel P27/P28.
