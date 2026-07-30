---
knowledge_type: decision
status: current
covers:
  - docs/architecture/optimus-version.json
  - docs/architecture/hermes-baselines/hermes-0.19.0.json
  - docs/architecture/hermes-feature-evidence.json
  - docs/architecture/hermes-performance-evidence.json
  - docs/architecture/hermes-manual-capabilities.json
  - scripts/optimus_version.py
  - scripts/check-parity-ledger.py
  - scripts/rebuild-install-relaunch.sh
  - scripts/rebuild-install-relaunch.ps1
  - apps/optimus-cli/src/main.rs
depends_on:
  - docs/decisions/0023-fixture-replay-trace-telemetry-evaluation.md
  - docs/architecture/parity-capability-ledger.json
validated_by:
  - scripts/test_optimus_version.py
  - scripts/check-parity-ledger.py
  - apps/optimus-cli/src/main.rs
last_verified_commit: null
---

# ADR-0024: Fail-closed Hermes parity version gate

- **Status:** Accepted
- **Date:** 2026-07-23

## Context

Optimus used workspace SemVer and a 51-row Hermes capability ledger. The ledger
was useful for planning, but it grouped many user-visible behaviors into broad
rows and only required an existing evidence path plus a trajectory for a
`parity` or `win` state. It did not freeze one exact Hermes release, enumerate
individual CLI/options/tools/providers/platforms, bind every claim to one
Optimus revision, compare runtime performance, or prevent Optimus from taking a
Hermes version number before full parity.

A single version number therefore could not distinguish Optimus development
progress from a verified compatibility claim. Treating broad rollup rows as a
complete product claim would permit accidental under-inventory and marketing
parity without equivalent speed, quality, reliability, or cost.

## Decision

1. Optimus product SemVer remains independent and is sourced from the workspace
   `Cargo.toml`.
2. Hermes target version and Hermes parity version are separate fields. The
   parity version is null unless the exact target passes every gate.
3. A frozen baseline is generated from an exact clean upstream Hermes commit.
   It recursively inventories CLI commands/options and reads slash-command,
   toolset, tool, provider, and bundled-platform registries. Curated manual rows
   cover non-CLI product behavior.
4. Normalized inventory collisions are preserved as deterministic variant IDs;
   capture never silently drops a contract. Dynamic third-party MCP tool names
   are represented by the finite MCP protocol/product contract rather than an
   impossible unbounded name snapshot.
5. A separate official-documentation audit is mandatory. Machine capture alone
   cannot assert completeness.
6. Every baseline feature requires current evidence paths, a named executable
   trajectory, a verification timestamp, and one full Optimus revision. There
   are no feature waivers or `not-applicable` parity states.
7. Every human rollup row must be `parity` or `win`. The rollup remains useful
   for ownership but cannot replace per-feature evidence.
8. Comparative performance uses raw paired samples. The gate recomputes success,
   quality, p50/p95 latency, p50/p95 TTFT, cost per success, and p50/p95 peak RSS.
   Optimus must equal or beat Hermes on every required axis and scenario.
9. Comparisons require the same machine, model, provider, and tool permissions,
   randomized paired order, minimum sample/seed counts, fresh evidence, and an
   exact Hermes baseline plus Optimus revision.
10. Promotion requires a clean immutable Optimus tree. A verified claim records
    target version, reviewer, timestamp, and revision.
11. Release preflight allows honest independent development versions. If the
    Optimus product number numerically equals the tracked Hermes version, or a
    verified claim is active, any parity blocker aborts release.
12. Linux and Windows installers run release preflight before build/binary
    selection, then rerun it and revalidate the selected binaries immediately
    before stopping or replacing the installed app. They persist target/parity
    metadata. The CLI exposes the embedded product, target, claim, and frozen
    contract count without opening Optimus state.

## Alternatives considered

### Reuse workspace SemVer as the parity statement

Rejected. Product iteration and cross-product equivalence are different claims.
A single number would either stall normal Optimus releases or imply parity too
early.

### Trust the 51-row rollup alone

Rejected. Broad rows can conceal missing subcommands, options, providers,
platforms, edge behaviors, and regressions.

### Permit feature waivers for unrelated Optimus advantages

Rejected. The requirement is complete Hermes capability coverage. An Optimus
advantage may produce a `win`, but it cannot compensate for an absent Hermes
feature.

### Compare average latency only

Rejected. Mean or p50-only checks can hide tail regressions that materially slow
tool loops, browser work, session resume, or delegated tasks.

### Capture the developer's current Hermes checkout

Rejected. The local Hermes tree can contain later commits and uncommitted
changes while still reporting the same release string. Baselines use an exact
clean upstream revision.

## Reasons

The split makes development velocity and compatibility truth independently
measurable. Exact-release inventory prevents target drift; immutable evidence
prevents current behavior from being inferred from stale prose; paired raw
samples make performance claims recomputable; and installer enforcement places
the truth check on the path that can publish a misleading version.

## Consequences

- Optimus can continue normal `0.x` development without pretending to match the
  tracked Hermes release.
- Taking the exact Hermes number is mechanically blocked until complete proof.
- A Hermes release update invalidates the prior baseline hash and evidence,
  forcing explicit recapture and re-audit.
- Feature evidence is intentionally verbose: each frozen contract must be
  reviewable and revision-bound.
- Performance evidence has nontrivial runtime/API cost because 30 paired samples
  across eight scenarios are the minimum, not optional documentation.
- The current honest parity state is null even though several rollup slices are
  already at parity or win.

## Risks and unresolved boundaries

- Machine inventory can still miss a feature that has no registry, help entry,
  source marker, or curated manual row; the official-documentation audit is the
  independent backstop.
- A deterministic quality grader can be incomplete. Scenario definitions and
  graders require review and version binding before evidence is accepted.
- Same-provider network variance can distort end-to-end timing. Randomized paired
  order, p50/p95 thresholds, seeds, and freshness reduce but do not eliminate it.
- Peak RSS and cost attribution must be collected consistently on each platform.
- The current report schema ingests raw evidence but does not itself provision
  paid provider credentials or launch benchmarks automatically.
- PowerShell installer syntax cannot be executed on hosts without PowerShell;
  Windows CI remains the authoritative platform check.

## Evaluation evidence

- `scripts/test_optimus_version.py` covers short/long option identity,
  collision preservation, numerical-version rejection, independent development
  versions, protocol proof, equal-or-faster comparisons, slower p95 rejection,
  and checked-in manifest integrity.
- `scripts/check-parity-ledger.py` validates the rollup and version-system
  structural errors together.
- `cargo test -p optimus-cli --all-targets -- --test-threads=1` compiles and
  tests the embedded version command.
- `optimus version --json` is checked to avoid creating an Optimus home and to
  expose parity as null while unverified.
- `scripts/optimus_version.py gate` is expected to fail until all blockers are
  removed; `release-check` is expected to pass the independent `0.1.0`
  development version.

## Relevant code

- `scripts/optimus_version.py`
- `scripts/check-parity-ledger.py`
- `apps/optimus-cli/src/main.rs`
- `scripts/rebuild-install-relaunch.sh`
- `scripts/rebuild-install-relaunch.ps1`

## Relevant tests

- `scripts/test_optimus_version.py`
- `apps/optimus-cli/tests/eval_compare.rs`
- `apps/optimus-cli/tests/eval_report.rs`

## Conditions for reconsideration

Reconsider the exact scenario set, sample floor, evidence age, or measured axes
only with a versioned benchmark study showing that the replacement is at least
as strict and harder to game. Reconsider dual versioning only if Optimus no
longer makes any Hermes-equivalence claim. Any replacement must remain
fail-closed, exact-release-bound, revision-bound, waiver-free, and independently
recomputable from raw evidence.

## Addendum — linked-worktree Git queries (2026-07-31)

Revision and cleanliness checks pass `--work-tree=<evaluated root>` on the
individual Git invocation. The canonical Optimus repository has
`core.bare=true` with linked worktrees, so cwd alone cannot make `git status`
recognise a real worktree. A process-wide `GIT_WORK_TREE` workaround is
forbidden: it leaks into verification fixtures that intentionally create bare
remotes and changes the meaning of their Git commands.

`scripts/test_optimus_version.py` reproduces the shared `core.bare=true` shape
and proves the scoped query sees the candidate worktree without mutating the
process environment. This changes only repository discovery; the clean-tree
promotion requirement remains unchanged.
