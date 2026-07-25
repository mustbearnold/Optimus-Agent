---
knowledge_type: verification
status: current
owns:
  - docs/architecture/product-complete-p22-verification.md
covers:
  - docs/plans/product-complete-program.md
depends_on:
  - docs/decisions/0039-files-mutate-effect-taxonomy.md
  - docs/plans/product-complete-program.md
validated_by:
  - crates/optimus-runtime/tests/path_confinement.rs
  - crates/optimus-runtime/tests/approvals_surface.rs
  - crates/optimus-kernel/tests/domain_modularity.rs
  - crates/optimus-packs/tests/packs_budget.rs
last_verified_commit: null
---

# Product-complete program P22 verification

Planes: **program P22** · delivery **PR #31** · architecture hold (Security / Durability / Domain) ·
ledger `files.mutate` (parity), `projects.scope` (partial honesty)

Date: 2026-07-25

## Goal

Files mutate (mkdir/rename/delete/patch + write) under Work Graph + SmartDeny +
cap-std; project isolation **honesty** (configured vs enforced modes).

## What landed

| Item | Result | Evidence |
|---|:---:|---|
| ADR-0039 effect taxonomy | **PASS** | `docs/decisions/0039-files-mutate-effect-taxonomy.md` |
| Effect + is_high_risk + skill class | **PASS** | `optimus-graph` |
| Runtime execute/preflight/crash classes | **PASS** | `optimus-runtime` |
| Tools + closed registry | **PASS** | packs `mkdir`/`delete_path`/`rename_path`/`patch_file` |
| Kernel Project* dispatch | **PASS** | `run_project_file_job` |
| path_confinement mutate suite | **PASS** | 10 tests |
| approvals_surface hold | **PASS** | 11 tests (incl. mkdir) |
| Isolation honesty fields | **PASS** | `product_settings` unit test |
| Security map + high-risk contracts | **PASS** | docs updated |
| Ledger `files.mutate` | **PASS** | parity |
| Ledger `projects.scope` | **partial** | honesty fields only; concurrent lease residual |

## Residuals (owned, not grade failures)

| Residual | Owner |
|---|---|
| Concurrent multi-project mutate **lease store** (runtime deny across projects) | Follow-up under program P22 / P26 doctor UI — settings flag remains default false; honesty fields present |
| Campaign `StepKind` only Write/Run | Campaign follow-up |
| Specialist ceiling not auto-widened | ADR-0033 |
| Sealed `isolated_profiles` homes | After P29 / S7 |

Hold record: [product-complete-p22-hold-2026-07-25.md](../evidence/product-complete-p22-hold-2026-07-25.md).

## Hold suite

```bash
cargo test -p optimus-runtime --test path_confinement --test approvals_surface -- --test-threads=1
cargo test -p optimus-packs --test packs_budget -- --test-threads=1
cargo test -p optimus-kernel --test domain_modularity --test kernel_turn -- --test-threads=1
python3 scripts/check-domain-modularity.py
python3 scripts/check-parity-ledger.py
python3 scripts/check-architecture-marks.py
```

## Non-claims

- Full concurrent-project lease product UI
- Isolated profile homes
- Hermes gate PASS

## Verdict

**program P22 files.mutate exit: PASS**. Isolation honesty fields landed; `projects.scope` remains **partial** until S2.14 concurrent lease.
Next: program P23 (coordinated browser) or S2.14 lease.
