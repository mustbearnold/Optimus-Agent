---
doc_id: architecture-s-plus-plus-plus-p17-verification
doc_type: history
plane: history
status: historical
authority: historical
summary: Date: 2026-07-25 Planes: program P17 · decision (process; version/parity policy) · delivery PR #27
reviewed_on: 2026-07-31
review_by: never
---

# S+++ P17 verification — release / parity gating

Date: 2026-07-25  
Planes: program **P17** · decision (process; version/parity policy) · delivery **PR #27**

## Exit evidence

| Microtask | Evidence |
|---|---|
| R1 Gate matrix | `docs/architecture/release-and-parity-gates.md` (pre-merge / pre-release / pre-parity-claim) |
| R2 Marks claim hygiene | `scripts/check-architecture-marks.py` + `scripts/test_architecture_marks.py` (negated status / unbolded S+++ fail-closed) |
| R3 Version/ledger green | `check-parity-ledger.py` OK; `optimus_version.py release-check` PASS; artifacts evidence path post-P11 peel |
| R4 Release **S+++** | `architecture-marks.md` + program microtasks done |

## Commands

```bash
python3 scripts/check-parity-ledger.py
python3 scripts/optimus_version.py release-check
python3 scripts/check-architecture-marks.py
python3 scripts/test_architecture_marks.py
```

## Grade moves

| Mark | Before | After |
|---|---|---|
| Release / parity gating | A | **S+++** |

## Explicit non-claims

- Hermes `optimus_version.py gate` remains **BLOCKED** until full feature/perf evidence.
- Architecture S+++ for Release grades the **gate system**, not product Hermes parity.
