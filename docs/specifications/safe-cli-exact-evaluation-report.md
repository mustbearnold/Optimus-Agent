---
knowledge_type: specification
status: historical
covers:
  - apps/optimus-cli/src/main.rs
  - apps/optimus-cli/tests/eval_report.rs
  - crates/optimus-eval/src/evaluation.rs
  - crates/optimus-eval/tests/evaluation_contracts.rs
  - docs/architecture/phase-14-gateway-eval.md
  - docs/architecture/system-overview.md
  - docs/maps/observability-and-evaluations.md
  - scripts/engineering_memory.py
  - scripts/test_engineering_memory.py
---

# Safe CLI operation for exact evaluation reports

**Date:** 2026-07-20

## Problem and outcome

**Observed fact:** `run_priority2_offline_evaluation` produces the exact ten-case
candidate report, but the CLI exposes only the legacy four-case `eval run` command.
Operators cannot invoke the complete producer without writing Rust code.

**Observed fact:** the kernel runner currently creates and executes its owned run
before invalid binding, measurement identity, or threshold policy is rejected by
report construction. These are caller inputs and can be checked before mutation.

**Intended outcome:** `optimus eval report` accepts bounded JSON files for candidate
binding, exact per-case resource measurements, and optional thresholds, then prints
the complete report as JSON. All typed inputs are validated before run-directory
creation or fixture execution.

## Scope

- Preflight candidate binding, exact Priority-2 measurement identities, and threshold
  policy before creating evaluation run state.
- Add `eval report --binding PATH --measurements PATH [--thresholds PATH]`.
- Bound each JSON input using the existing one-megabyte evaluation input policy.
- Parse the existing public types; do not introduce a second CLI schema.
- Print one pretty JSON `EvaluationReportV1` to stdout.
- Return a non-zero exit after printing when report thresholds fail.
- Exercise the installed binary seam with success and invalid-input tests.
- Update CLI/evaluation authority and generated Engineering Memory.

## Non-scope

- Replacing or changing `eval run`.
- Generating candidate binding hashes or resource measurements.
- Reading route telemetry, timing fixtures, billing integration, or defaulting values.
- Persisting or accepting baselines.
- Live providers, network evaluation, UI changes, release gates, or automatic
  invocation during delivery.
- Changing dataset cases, report metrics, hashes, comparison, or exact suite checks.

## Authoritative existing behaviour

- `eval run [--json]` executes only `run_offline_trajectory_suite` and remains a
  compatibility command.
- The exact report runner owns a fresh UUID directory and delegates evidence,
  projection, arithmetic, thresholds, and report hashing to existing kernel
  contracts.
- Candidate binding and measurement types already deserialize through Serde.
- `MetricThreshold` public fields can deserialize values that must still be checked
  through its constructor policy.
- The CLI creates its configured home before dispatching commands.

## Contracts and invariants

1. Kernel preflight validates the built-in dataset and candidate binding, requires
   exactly one measurement per canonical case, and validates every threshold plus
   unique threshold metric dimensions before creating `evaluation-runs`.
2. Invalid preflight input returns `Err`, creates no evaluation-run directory, and
   executes neither exact suite.
3. CLI input files must be non-empty and no larger than
   `MAX_EVALUATION_DATASET_BYTES`; oversized or malformed input fails before calling
   the report runner.
4. `--binding` contains one `CandidateBinding`; `--measurements` contains a JSON array
   of `EvaluationResourceMeasurement`; omitted `--thresholds` means an explicit empty
   threshold policy, while a supplied file contains a JSON array of `MetricThreshold`.
5. The CLI passes parsed values unchanged to the kernel. It does not repair,
   normalize, infer, time, or substitute evidence.
6. Success stdout is exactly one pretty JSON report. Diagnostic errors use the
   existing stderr/error exit path and must not echo input file contents.
7. A threshold-failing report is printed completely, then the process exits non-zero
   so automation can retain evidence and detect failure.
8. Existing `eval run`, other commands, report serialization, and kernel APIs remain
   compatible.

## State, failure, interruption, and recovery

- JSON reads are read-only and bounded before deserialization. No temporary input
  copy or credential access is added.
- The CLI's existing home creation remains; invalid inputs may create that empty home
  but must not create `evaluation-runs` or suite state.
- Once preflight passes, existing UUID ownership, interruption, partial-run, retry,
  and concurrency contracts govern execution. A crash can leave only the owned
  partial run and no printed report.
- JSON output is not atomically persisted by Optimus. Callers choosing shell
  redirection own destination atomicity and recovery.
- Concurrent CLI invocations use separate UUID run directories and share no report
  row or baseline mutation.

## Interface and compatibility

```text
optimus --home PATH eval report \
  --binding binding.json \
  --measurements measurements.json \
  [--thresholds thresholds.json]
```

No flags or output of `eval run` change. No database schema or migration changes.

## Acceptance criteria

- Invalid binding, measurement identity, threshold value, and duplicate threshold
  dimension each fail before `evaluation-runs` exists.
- A real CLI invocation with valid JSON inputs exits zero and stdout deserializes as
  a passing ten-sample `EvaluationReportV1` with the exact binding and resource
  means.
- Omitting thresholds produces an empty policy; supplying thresholds preserves them.
- Malformed and oversized JSON exit non-zero without suite state or input-content
  disclosure.
- A valid but failing threshold prints a complete report and exits non-zero.
- Legacy `eval run --json` remains executable.
- Focused, canonical, Engineering Memory, exact-scope, and detached-tree gates pass.

## Execution plan and ledger

### Slice 1 — mutation-free typed preflight

- **Outcome:** reject caller-contract errors before any evaluation fixture state.
- **Dependencies:** existing binding, measurement, and threshold validators.
- **RED:** assert invalid binding and malformed measurement/threshold policies leave
  no `evaluation-runs`; current runner creates and executes first.
- **GREEN:** factor exact measurement and threshold-policy validation, invoke all
  preflight checks before UUID ownership creation, and reuse them in projection and
  report construction.
- **Refactor:** centralize validation without duplicating report arithmetic.
- **Verification:** selected evaluation contracts and strict focused Clippy.
- **Complete when:** every caller-contract failure is mutation-free.
- **Observed evidence:** RED rejected an invalid binding only after creating and
  executing an evaluation run. GREEN moved binding, exact measurement identity,
  threshold value, and duplicate-dimension checks before UUID ownership; all four
  failures left no `evaluation-runs`, and evaluation contracts passed 18/18.

### Slice 2 — bounded CLI report command

- **Outcome:** operators can produce the exact report through the real binary.
- **Dependencies:** Slice 1 and existing CLI dispatch.
- **RED:** binary integration test fails because `eval report` is not a recognized
  subcommand.
- **GREEN:** add the command, bounded generic JSON reader, kernel invocation, JSON
  output, and threshold-failure exit.
- **Refactor:** share input parsing only where it reduces duplicate policy.
- **Verification:** CLI binary success, malformed/oversized input, failing threshold,
  and legacy `eval run`; kernel evaluation contracts and strict CLI Clippy.
- **Complete when:** stdout/exit/state behavior matches every acceptance criterion.
- **Observed evidence:** RED failed because Clap rejected `eval report`. GREEN
  produced a deserializable passing report with supplied and omitted threshold
  policies, printed a complete failing-threshold report before non-zero exit,
  rejected malformed and oversized JSON without suite state or content echo, and
  preserved legacy `eval run --json`; CLI binary acceptance passed 4/4.

## Final verification

- CLI binary integration, evaluation, integrity, and trace contracts.
- Engineering Memory tests, generation, strict validation, and currentness.
- `cargo fmt --all --check`.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- `cargo test --workspace --all-features`.
- `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps`.
- Exact diff/path review and detached staged-tree focused verification.

## Prohibited actions

- Do not infer, fabricate, or silently default binding, measurement, or threshold
  values.
- Do not print input contents in errors or automatically persist/accept a baseline.
- Do not remove or repurpose legacy `eval run`.
- Do not manually edit generated Engineering Memory JSON.
- Do not create a branch or pull request, install dependencies, deploy, release,
  publish, access credentials, or modify unrelated paths.

## Assumptions and unresolved work

- **Reasonable inference:** explicit JSON evidence is the narrowest useful operator
  seam while measurement provenance remains externally owned.
- **Unresolved:** binding generation and provenance-bound live resource measurement
  remain separate future milestones.
