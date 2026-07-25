# Release and parity gates (operator matrix)

Date: 2026-07-25  
Planes: program **P17** · grade mark **Release / parity gating** · delivery **PR #27**

**Status:** Confirmed operator contract for merge hygiene vs product release.  
Architecture **S+++** for this mark does **not** require full Hermes parity
(2,063 feature contracts). It requires that gates remain **fail-closed**, that
operators know which command is merge-safe vs release-blocking, and that grade
claims cannot greenwash missing phase evidence.

## Two version questions (do not collapse)

| Question | Answer lives in | Green means |
|---|---|---|
| May I ship an ordinary Optimus development build? | `optimus_version.py release-check` | Product SemVer is honest and does not falsely equal Hermes without a verified claim |
| May I claim Hermes parity `X.Y.Z`? | `optimus_version.py gate` + evidence corpus | Every frozen feature, rollup row, and performance scenario is proven on a clean revision |

Full policy: [optimus-versioning.md](./optimus-versioning.md).

## Gate matrix

| Gate | Command | Pre-merge (PR / local) | Pre-release (ship binary / install) | Pre-parity-claim | Notes |
|---|---|:---:|:---:|:---:|---|
| Version structural integrity | `python3 scripts/optimus_version.py validate` | ✅ | ✅ | ✅ | Incomplete parity is reported; structural errors fail |
| Development release honesty | `python3 scripts/optimus_version.py release-check` | ✅ | ✅ (required by installer) | ✅ | Passes independent SemVer; blocks numeric Hermes collision without verified claim |
| Strict Hermes parity | `python3 scripts/optimus_version.py gate` | ❌ (expected red until complete) | ❌ unless claiming parity | ✅ required | Fail-closed; **not** an architecture S+++ blocker |
| Parity ledger rollup | `python3 scripts/check-parity-ledger.py` | ✅ | ✅ | ✅ | Evidence paths must exist; `parity`/`win` need trajectory |
| Architecture marks claim hygiene | `python3 scripts/check-architecture-marks.py` | ✅ | ✅ | optional | Fails if a mark is graded **S+++** without done phase / required paths |
| Observability | `python3 scripts/check-observability-gate.py` | ✅ when touching kernel/runtime/packs/eval | recommended | optional | Cargo integrity + causal/export surface |
| Desktop IPC matrix | `python3 scripts/check-desktop-ipc-matrix.py` | ✅ when touching desktop/electron/ui | recommended | optional | Host ⊇ Electron = React classification |
| Domain modularity | `python3 scripts/check-domain-modularity.py` | ✅ when touching packs/kernel/store | recommended | optional | Single `ToolDesc` catalog / plane separation |
| Crate layers | `python3 scripts/check-crate-layers.py` | ✅ when touching crate graph | recommended | optional | Control-plane peel deps |
| Engineering Memory | `python3 scripts/engineering_memory.py check` (+ `generate` / `validate` when stale) | ✅ | ✅ | optional | Generated maps must not be hand-edited |
| Runtime / pack hold suites | `cargo test -p optimus-runtime` / `optimus-kernel` / `optimus-packs` as touched | ✅ scoped | full workspace before major ship | optional | See program hold suites |
| Installer re-gate | `scripts/rebuild-install-relaunch.*` | n/a | ✅ | n/a | Runs `release-check` before binary selection and again before replace |

Legend: ✅ expected green for that class of change · ❌ not required (and often red by design).

## What architecture S+++ does **not** require

- Full Hermes feature inventory green (`gate` PASS).
- Every parity-ledger row at `parity` or `win`.
- Performance scenario suite complete.
- Marketing claim that Optimus “is” Hermes `X.Y.Z`.

Those remain **product parity** work under `program:parity` / version promote. The
Release mark grades the **gate system** (fail-closed scripts, docs, claim hygiene),
not product completeness.

## Operator quick paths

### Before opening / merging a PR

```bash
python3 scripts/optimus_version.py release-check
python3 scripts/check-parity-ledger.py
python3 scripts/check-architecture-marks.py
python3 scripts/engineering_memory.py check
# plus dimension gates for the files you touched
```

### Before install / ship of a development build

```bash
python3 scripts/optimus_version.py release-check
python3 scripts/check-parity-ledger.py
# installer scripts re-run release-check around binary selection
```

### Only when claiming Hermes parity

```bash
python3 scripts/optimus_version.py validate
python3 scripts/check-parity-ledger.py
python3 scripts/optimus_version.py gate          # must PASS
python3 scripts/optimus_version.py release-check
python3 scripts/optimus_version.py promote --reviewer "…"
```

## Sources of truth

| Artifact | Role |
|---|---|
| `docs/architecture/optimus-version.json` | Product version, Hermes target, claim, release rules |
| `docs/architecture/parity-capability-ledger.json` | Human rollup (51 rows); not the 2,063-feature gate |
| `docs/architecture/architecture-marks.md` | Architecture quality grades (S+++ climb) |
| `docs/plans/s-plus-plus-plus-program.md` | Phase exit criteria; P17 owns this matrix |
| `scripts/optimus_version.py` | Capture / validate / gate / release-check / promote |
| `scripts/check-parity-ledger.py` | Rollup + version integrity |
| `scripts/check-architecture-marks.py` | S+++ claim ↔ phase done / path existence |

## Related verification

- [s-plus-plus-plus-p17-verification.md](./s-plus-plus-plus-p17-verification.md)
- [optimus-versioning.md](./optimus-versioning.md)
- [sota-scorecard.md](./sota-scorecard.md) (parity planning rollup, not architecture grades)
