---
knowledge_type: specification
status: historical
covers:
  - crates/optimus-eval/src/evaluation.rs
  - crates/optimus-eval/tests/evaluation_contracts.rs
  - docs/maps/observability-and-evaluations.md
depends_on:
  - docs/decisions/0023-fixture-replay-trace-telemetry-evaluation.md
  - docs/contracts/high-risk-contracts.md
validated_by:
  - crates/optimus-eval/tests/evaluation_contracts.rs
last_verified_commit: 0c07762a40e4264a08c38eaf04532b41cb90e0d4
---

# Candidate-aware evaluation comparison

Date: `2026-07-20`

## Problem and outcome

The implemented evaluation comparator requires baseline and candidate
`CandidateBinding` values to be equal. Because that binding includes
`source_tree_sha256`, any real source change is rejected before metrics can be
compared. The comparator also trusts supplied report structures without
revalidating their content hash.

The observable outcome is that a baseline and candidate from different source
trees can be compared when, and only when, their evaluation dataset, contract,
tool catalog, route policy, provider/model, threshold policy, and metric schema
remain exact. Tampered or context-drifted evidence fails closed before a
comparison is returned.

## Repository truth

### Observed facts

- `EvaluationReportV1` binds source tree, contract, tool catalog, route policy,
  provider, model, metrics, thresholds, and a report SHA-256.
- `build_evaluation_report` creates deterministic report hashes.
- `BaselineStore::accept` and `BaselineStore::report` call the private report
  verifier.
- `compare_evaluation_reports` currently rejects every unequal binding and does
  not call the verifier.
- The only comparison regression test reuses one identical binding, so it does
  not exercise a real changed candidate.
- The CLI offline suite emits the older four-case `EvalReport`, not a versioned
  ten-case `EvaluationReportV1`.

### Inference

Candidate source identity must be allowed to differ; otherwise regression
comparison cannot evaluate a source change. Other binding fields describe the
evaluation context and must remain exact to prevent incomparable evidence from
being treated as a regression result.

### Unresolved assumptions

None. CLI/release integration requires an honest producer for the complete
versioned report and is excluded rather than inferred.

## Scope

- Verify both report hashes before semantic comparison.
- Allow baseline and candidate `source_tree_sha256` values to differ.
- Require exact dataset ID, dataset version, dataset hash, contract hash, tool
  catalog hash, route policy hash, provider, and model.
- Require exact threshold policy and metric-key set.
- Preserve deterministic improved/equal/regressed ordering.
- Add focused regressions for changed-source comparison and fail-closed drift.
- Refresh current evaluation documentation and generated Engineering Memory.

## Non-scope

- Running live providers or external effects.
- Producing synthetic observations for the six integrity cases.
- Adding a CLI, CI, release, routing, or deployment gate.
- Accepting a new baseline or mutating `BaselineStore` during comparison.
- Changing metrics, thresholds, dataset contents, report schema, or routing.

## Contracts and invariants

1. Comparison is read-only and performs no durable mutation.
2. Each report must pass its existing version/hash verification before any
   compatibility or metric result is returned.
3. Baseline and candidate source-tree hashes remain visible and may be equal or
   different.
4. Dataset identity and all non-source binding fields must be byte-for-byte
   equal.
5. Threshold vectors must be equal; moving the gate is not a quality result.
6. Metric key sets must be equal; missing or additional dimensions fail closed.
7. A lower latency/cost value is an improvement; a higher accuracy/quality value
   is an improvement, preserving current direction semantics.
8. Any verification or compatibility failure returns no partial comparison.
9. Existing report construction, baseline storage, and public types remain
   compatible.

## Failure and recovery

Comparison has no writes, leases, or external I/O. Interruption leaves no state.
Invalid hashes, context drift, threshold drift, or metric drift return an error.
The caller may retry only with unchanged, valid evidence; no rollback is needed.
Concurrent comparisons are independent and deterministic.

## Vertical execution ledger

### Slice 1 — changed source candidate

- **Outcome:** a valid candidate from a different source tree compares against
  its baseline.
- **Dependencies:** existing deterministic report builder and comparator.
- **RED:** modify the candidate binding source hash in the integration test; the
  current comparator rejects it.
- **GREEN:** compare only the fixed evaluation-context portion of the binding.
- **Refactor:** isolate compatibility checks if it improves clarity.
- **Verify:** focused changed-source regression test.
- **Complete when:** comparison reports expected regressed metrics while retaining
  distinct report identities.

### Slice 2 — fail-closed evidence and schema

- **Outcome:** tampered reports and policy/schema drift return no comparison.
- **Dependencies:** Slice 1 compatibility boundary.
- **RED:** one focused test at a time for stale report hash, changed threshold,
  and metric-key drift.
- **GREEN:** verify both reports first, then require exact threshold and metric
  key sets.
- **Refactor:** keep failure checks before metric iteration.
- **Verify:** focused evaluation contract suite.
- **Complete when:** every incompatible input errors and the valid comparison
  remains deterministic.

## Acceptance criteria

- A changed `source_tree_sha256` alone no longer prevents comparison.
- Dataset or non-source binding drift is rejected.
- Threshold-policy drift is rejected.
- Missing or additional metrics are rejected.
- A stale/tampered report hash is rejected.
- Existing baseline immutability remains green.
- Evaluation comparison performs no writes.

## Final verification

- Focused evaluation contract tests.
- Workspace format, strict Clippy, all-feature tests, and strict rustdoc.
- Engineering Memory semantic tests, generation, strict validation, and
  currentness.
- Exact staged-tree archive validation, diff review, and forbidden-path check.
- Post-commit and post-push Engineering Memory currentness.

## Prohibited actions

- Do not invent integrity observations or provider results.
- Do not weaken hash, dataset, policy, metric, or provenance checks.
- Do not mutate or replace accepted baselines during comparison.
- Do not add dependencies or alter persistent schemas.
- Do not create branches or pull requests, deploy, release, publish, access
  credentials, or modify unrelated files.
- One verified commit and push to `origin/main` are authorized after all gates
  pass.
