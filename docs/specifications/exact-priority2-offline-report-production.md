---
knowledge_type: specification
status: historical
covers:
  - crates/optimus-kernel/src/evaluation.rs
  - crates/optimus-kernel/src/lib.rs
  - crates/optimus-kernel/tests/evaluation_contracts.rs
  - docs/architecture/system-overview.md
  - docs/maps/observability-and-evaluations.md
  - scripts/engineering_memory.py
  - scripts/test_engineering_memory.py
---

# Exact Priority-2 offline report production

**Date:** 2026-07-20

## Problem and outcome

**Observed fact:** the exact four-case trajectory executor and exact six-case
integrity executor now return the output, canonical tool, terminal, replay, and
persisted trace evidence required by the Priority-2 dataset. No production path
projects those results into ten `EvaluationObservation` values or produces one
`EvaluationReportV1`.

**Observed fact:** latency and cost are required report dimensions, but the offline
scripted executors do not measure billing or define a stable performance clock.
Fixed zero would falsely claim measurement, while elapsed wall time would make the
fixture report nondeterministic.

**Intended outcome:** a public projector derives all behavioural observation fields
from exact executor results and accepts explicit per-case resource measurements. A
public isolated runner executes both exact suites and returns one candidate-bound,
threshold-checked, deterministic `EvaluationReportV1`.

## Scope

- Add a typed per-case latency/cost measurement input.
- Project one exact result and one exact measurement per dataset case into canonical
  dataset order.
- Derive text, tool, terminal, replay, and trace fields from executor evidence only.
- Run both exact offline suites under one fresh evaluation-owned run directory.
- Build and return one Priority-2 report using existing dataset, binding, metric,
  threshold, hash, and validation contracts.
- Verify malformed evidence, setup failure, retry isolation, and deterministic
  report identity.
- Update current observability authority and generated Engineering Memory.

## Non-scope

- Measuring wall-clock latency, token usage, billing, energy, CPU, or GPU.
- Inventing fixed resource values or accepting omitted measurements.
- Persisting reports or accepting a baseline.
- Automatically deriving `CandidateBinding` hashes.
- Changing dataset cases, metrics, arithmetic, thresholds, baseline comparison, or
  either exact executor's subsystem checks.
- Live providers, network evaluation, release gates, CLI/UI commands, or universal
  workflow evaluation.

## Authoritative existing behaviour

- `priority2_dataset` owns the exact ten case contracts and canonical order.
- The trajectory executor returns four exact outputs, canonical invoked tools,
  succeeded terminal manifests, fixture replay classes, and persisted root traces.
- The integrity executor returns six canonical outcomes, succeeded terminal status,
  deterministic replay, and persisted evaluation roots; pre-trace setup failure is
  explicitly untraced.
- `build_evaluation_report` validates identity/tool counts and required traces,
  computes checked integer metrics, evaluates thresholds, binds the candidate, and
  verifies its own deterministic report hash.
- Successful executor retries have fresh trace identities. Trace identity itself is
  evidence, but `EvaluationObservation` intentionally records only presence.

## Contracts and invariants

1. `EvaluationResourceMeasurement` contains an explicit canonical case ID,
   `latency_millis`, and `cost_microunits`; no default or inferred value exists.
2. Projection requires result IDs and measurement IDs to each equal the dataset case
   set exactly. Missing, duplicate, or unknown identities fail before output.
3. Projection output order equals dataset order regardless of input order.
4. Exact text is true only when output equals the contract text. For a contract with
   no expected text, only empty output is exact.
5. `expected_tools` is the contract identity count; `observed_tools` is the full
   invocation count; `matched_tools` is the unique set intersection. Duplicate or
   unexpected invocations therefore reduce precision without inflating recall.
6. Terminal and replay correctness are exact typed equality with the case contract.
   Trace presence derives only from a returned persisted context.
7. Resource values are copied from the identity-matched explicit measurement; the
   projector never substitutes zero, measures elapsed time, or reads billing state.
8. Existing report construction remains the final fail-closed validator. Required
   untraced results, inconsistent counts, arithmetic overflow, invalid binding, or
   invalid thresholds return `Err` and no report.
9. The exact runner creates one fresh UUID run directory and executes trajectory and
   integrity suites only below it. Existing run directories are never resumed,
   mutated, or removed.
10. With equal dataset, binding, thresholds, semantic executor outcomes, and resource
    measurements, independent retries produce byte-identical observations and
    reports despite fresh run/trace identities.

## State, failure, interruption, and recovery

- Failure to create the owned report-run directory returns `Err` before either suite
  executes and preserves the obstructing path.
- A trajectory setup failure remains an untraced result; projection/report
  construction fails instead of fabricating evidence.
- Integrity trace/storage failure already returns `Err` and aborts report production.
- Behavioural mismatches that retain complete typed evidence are represented as
  failed metric dimensions rather than converted to infrastructure errors.
- A crash may leave a partial UUID run directory and no report. Retry uses a fresh
  directory and never repairs or mutates the abandoned run.
- Concurrent invocations own separate UUID directories; there is no shared lease,
  report row, or cross-run mutation.
- No report or baseline persistence is added, so there is no report rollback or
  migration boundary.

## Interface and compatibility

- Add public `EvaluationResourceMeasurement`.
- Add public `project_evaluation_observations(dataset, results, measurements)`.
- Add public `run_priority2_offline_evaluation(home, binding, measurements,
  thresholds)` returning `Result<EvaluationReportV1>`.
- Existing executor, observation, report, baseline, and serialized contracts remain
  unchanged.

## Acceptance criteria

- Real outputs from both exact suites project to exactly ten observations in dataset
  order with correct exact-text, tool, terminal, replay, trace, latency, and cost
  values.
- Reordered result and measurement inputs produce identical canonical observations.
- Duplicate, missing, or unknown result/measurement identities fail closed.
- Duplicate expected-tool invocations cannot inflate matched-tool recall.
- One public call executes both suites and returns a passing ten-sample report with
  caller-selected thresholds and the exact supplied binding.
- Two runs with equal explicit inputs produce identical report bytes/hash while
  owning distinct on-disk run and trace identities.
- An unusable report home returns `Err`, preserves blocking bytes, creates no sibling
  execution state, and returns no report.
- Focused, canonical, Engineering Memory, exact-scope, and detached-tree gates pass.

## Execution plan and ledger

### Slice 1 — exact result projection

- **Outcome:** exact executor evidence plus explicit resources becomes ten canonical
  observations without fabrication.
- **Dependencies:** existing dataset and typed executor outputs.
- **RED:** add a projection contract test over real suite outputs; compilation fails
  because the measurement type and projector do not exist.
- **GREEN:** validate exact ID sets, map by case ID, derive behavioural fields, and
  copy matched resources.
- **Refactor:** centralize exact set collection and unique tool intersection only if
  needed by the implementation.
- **Verification:** selected projection test and evaluation contracts.
- **Complete when:** all ten fields are exact and malformed identity/tool boundaries
  fail closed.
- **Observed evidence:** RED failed because the measurement type and projector were
  absent. GREEN projected reordered real suite results into ten dataset-ordered,
  fully correct observations, copied identity-matched resources, kept duplicate
  expected-tool invocations from inflating recall, and rejected duplicate,
  missing, and unknown identities.

### Slice 2 — isolated report production

- **Outcome:** one public call returns the real ten-case candidate report.
- **Dependencies:** Slice 1 and both exact executors.
- **RED:** require a passing report, stable retry bytes/hash, distinct run identities,
  and fail-closed unusable-home behavior; the runner does not exist.
- **GREEN:** create a fresh owned run directory, execute both suites, project their
  combined results, and call the existing report builder.
- **Refactor:** keep orchestration separate from projection and report arithmetic.
- **Verification:** selected integration test, complete evaluation/integrity/trace
  contracts, and strict focused Clippy.
- **Complete when:** success, deterministic retry, ownership, and setup-failure
  acceptance all pass.
- **Observed evidence:** RED failed because the report runner was absent. GREEN
  produced equal passing ten-sample report bytes and hashes across two fresh owned
  runs, preserved the exact binding, thresholds, and supplied resource means, and
  returned `Err` without changing the blocking file for an unusable home.

## Final verification

- Evaluation, integrity integration, trace contracts, and affected unit tests.
- Engineering Memory tests, generation, strict validation, and currentness.
- `cargo fmt --all --check`.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- `cargo test --workspace --all-features`.
- `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps`.
- Exact diff/path review and detached staged-tree focused verification.

## Prohibited actions

- Do not fabricate resource measurements, trace presence, tool matches, or success.
- Do not use wall-clock timing in deterministic report identity.
- Do not persist or accept a baseline automatically.
- Do not mutate existing evaluation run directories or manually edit generated
  Engineering Memory JSON.
- Do not create a branch or pull request, install dependencies, deploy, release,
  publish, access credentials, or modify unrelated paths.

## Assumptions and unresolved work

- **Reasonable inference:** explicit caller-owned resource measurements are the
  narrowest honest boundary until model/provider telemetry can be causally joined
  to these exact cases.
- **Unresolved:** this API validates measurement identity and integer arithmetic but
  cannot independently prove the measurement source. Live provenance-bound billing
  and timing remain future work.
