---
knowledge_type: specification
status: historical
covers:
  - crates/optimus-eval/src/evaluation.rs
  - crates/optimus-eval/tests/evaluation_contracts.rs
  - docs/maps/observability-and-evaluations.md
depends_on:
  - docs/specifications/candidate-aware-evaluation-comparison.md
  - docs/decisions/0023-fixture-replay-trace-telemetry-evaluation.md
validated_by:
  - crates/optimus-eval/tests/evaluation_contracts.rs
last_verified_commit: df70591ab07a67c1589651c266b836879f8ced9b
---

# Fail-closed evaluation report validation

Date: `2026-07-20`

## Problem and outcome

`BaselineStore::accept`, `BaselineStore::report`, and candidate comparison call a
report verifier, but that verifier checks only report version and the self-hash.
A caller can therefore construct malformed evidence, recompute its hash, and
persist it as an immutable baseline.

The observable outcome is that only structurally and semantically valid
`EvaluationReportV1` evidence can be returned from report construction, accepted
by the baseline store, loaded from it, or compared. Invalid evidence fails before
any baseline insert or comparison result.

## Repository truth

### Observed facts

- Report construction computes seven fixed metrics, explicit threshold failures,
  a `passed` projection, and a content hash.
- Public report fields permit callers to construct or mutate reports.
- The private verifier currently checks only report version and hash.
- Baseline acceptance validates before its SQLite insert, so strengthening the
  verifier preserves the existing no-partial-write ordering.
- Candidate comparison already invokes the verifier on both reports.

### Inference

The verifier must enforce the same deterministic invariants used by the report
builder; a valid self-hash proves integrity of bytes, not validity of their
meaning.

### Unresolved assumptions

None. Producing the complete ten-case observation set and exposing a CLI gate
remain separate milestones.

## Scope

- Validate dataset and candidate-binding identities.
- Require the exact seven evaluation metric dimensions.
- Validate each metric score's key identity, sample bounds, arithmetic, and
  direction-independent value projection.
- Validate thresholds, reject duplicate threshold dimensions, recompute exact
  threshold failures, and validate `passed`.
- Make report construction run the same verifier before returning.
- Prove invalid rehashed evidence is rejected before baseline persistence.
- Refresh current evaluation documentation and generated Engineering Memory.

## Non-scope

- New report, dataset, or database schema versions.
- Producing integrity observations or running live providers/effects.
- CLI, CI, release, routing, or deployment gates.
- Rewriting or migrating already accepted baseline rows.
- Changing metric definitions or threshold direction semantics.

## Contracts and invariants

1. Validation occurs before `BaselineStore::accept` executes its insert.
2. Report and dataset versions are exact supported versions; IDs and hashes are
   canonical.
3. Candidate binding validates all four SHA-256 identities and non-empty
   provider/model identity.
4. The metric key set is exactly `ExactText`, `ToolPrecision`, `ToolRecall`,
   `TerminalAccuracy`, `ReplayAccuracy`, `LatencyMillis`, and `CostMicrounits`.
5. Every score's embedded metric equals its map key and has a bounded nonzero
   sample count shared by all dimensions.
6. Ratio metrics use `10000` when their denominator is zero; otherwise value is
   checked integer `numerator * 10000 / denominator`, with numerator no greater
   than denominator.
7. Latency/cost denominator equals sample count and value equals integer
   `numerator / denominator`.
8. Threshold dimensions are unique and each threshold passes existing policy
   validation.
9. `threshold_failures` is the sorted exact recomputation from scores and
   thresholds; `passed` is true exactly when that list is empty.
10. The report self-hash is valid after all semantic checks.
11. Rejection performs no insert, replacement, or partial comparison.
12. Existing valid reports and baseline immutability remain compatible.

## Failure and recovery

- Invalid reports return an error before persistent mutation.
- SQLite insertion behavior is unchanged; an insertion failure remains atomic.
- Loading a corrupt historical row fails without rewriting it.
- Comparison remains read-only.
- Retrying requires corrected evidence; no rollback or repair is automatic.
- Concurrent valid accepts retain existing primary-key immutability behavior.

## Vertical execution ledger

### Slice 1 — structural authority

- **Outcome:** a correctly rehashed report with a missing metric or invalid
  binding cannot enter `BaselineStore`.
- **Dependencies:** existing report rehash test helper and pre-insert verifier.
- **RED:** remove one metric, rehash, and attempt acceptance; current verifier
  accepts it.
- **GREEN:** validate versions, IDs/hashes, binding, and exact metric schema.
- **Refactor:** centralize the canonical metric set.
- **Verify:** focused baseline acceptance tests and comparison tests.
- **Complete when:** malformed evidence errors and the store contains no row.

### Slice 2 — semantic authority

- **Outcome:** rehashed arithmetic, threshold, failure-list, or pass-state drift
  is rejected.
- **Dependencies:** Slice 1 schema validation.
- **RED:** introduce one semantic inconsistency at a time and rehash it.
- **GREEN:** validate score arithmetic, unique thresholds, recomputed failures,
  and `passed`.
- **Refactor:** share threshold evaluation between construction and validation.
- **Verify:** complete evaluation contract suite and focused strict Clippy.
- **Complete when:** every invalid report fails and deterministic valid reports
  remain byte-identical.

## Acceptance criteria

- Missing or additional metric dimensions fail closed.
- Invalid candidate binding fails closed.
- Inconsistent score arithmetic or samples fail closed.
- Duplicate/invalid thresholds fail closed.
- Incorrect threshold failures or `passed` fail closed.
- A rejected report is absent from the baseline store.
- Valid report construction, immutable storage, loading, and candidate comparison
  remain green.

## Final verification

- Focused evaluation contract tests.
- Workspace format, strict Clippy, all-feature tests, and strict rustdoc.
- Engineering Memory semantic tests, generation, strict validation, and
  currentness.
- Config-neutral detached staged-tree archive verification.
- Exact diff, forbidden-path, post-commit, and remote-identity checks.

## Prohibited actions

- Do not weaken hashes, metric checks, thresholds, or baseline immutability.
- Do not fabricate integrity observations or provider results.
- Do not migrate, delete, or rewrite accepted baseline rows.
- Do not add dependencies or change persistent schemas.
- Do not create branches or pull requests, deploy, release, publish, access
  credentials, or modify unrelated files.
- One verified commit and push to `origin/main` are authorized after all gates
  pass.
