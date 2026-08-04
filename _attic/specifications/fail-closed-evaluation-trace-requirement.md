---
doc_id: specifications-fail-closed-evaluation-trace-requirement
doc_type: history
plane: history
status: historical
authority: historical
summary: Historical record for Fail-closed evaluation trace requirement; retained for provenance and excluded from default retrieval.
reviewed_on: 2026-07-31
review_by: never
knowledge_type: specification
covers:
  - crates/optimus-eval/src/evaluation.rs
  - crates/optimus-eval/tests/evaluation_contracts.rs
  - docs/architecture/system-overview.md
  - docs/maps/observability-and-evaluations.md
  - scripts/engineering_memory.py
  - scripts/test_engineering_memory.py
---

# Fail-closed evaluation trace requirement

**Date:** 2026-07-20

## Problem and outcome

**Observed fact:** every case in the built-in Priority-2 dataset declares
`trace_required: true`, but `EvaluationObservation` cannot represent trace
presence and `build_evaluation_report` does not enforce the declaration.
Candidate-bound reports can therefore be built from observations with no trace.

**Intended outcome:** observations explicitly state whether trace evidence is
present, and report construction rejects any observation that omits trace evidence
when its case contract requires it.

## Scope

- Add explicit trace-presence evidence to `EvaluationObservation`.
- Enforce each case's `trace_required` contract during report construction.
- Update all focused fixtures to declare trace presence honestly.
- Cover missing required trace, JSON omission, and optional-trace compatibility.
- Update evaluation authority and generated Engineering Memory.

## Non-scope

- Storing trace IDs or span IDs in `EvaluationReportV1`.
- Producing observations from trajectory or integrity executors.
- Changing the Priority-2 dataset's ten cases or trace requirements.
- Adding a trace metric, changing thresholds, or bumping the unchanged report
  output schema.
- Defining integrity-case trace production, `TraceStore` lifecycle, or child spans.
- Latency/cost collection, baseline delivery gates, or live-provider evaluation.

## Authoritative existing behaviour

- `EvaluationCaseContract.trace_required` is versioned dataset authority.
- `EvaluationDataset::validate` already validates case identity and provenance.
- `build_evaluation_report` fail-closes on observation count, identity, duplicate
  cases, tool-count consistency, arithmetic overflow, and threshold inconsistency.
- `EvaluationObservation` is report-construction input, not persisted baseline or
  report output.
- Existing reports and baselines do not serialize per-case observations.

## Contracts and invariants

1. `EvaluationObservation` has a required serialized boolean `trace_present`.
2. Omitted `trace_present` fails deserialization; absence may not silently default
   to either value.
3. If a case declares `trace_required: true`, its observation must declare
   `trace_present: true` or report construction fails before metric calculation.
4. If a case declares `trace_required: false`, either trace-presence value is
   accepted; optional evidence is not discarded or treated as an error.
5. Trace presence is evidence validity, not a quality metric. It does not alter
   metric numerators, denominators, thresholds, report hashes, or output schema.
6. Existing case identity, exact-text, tool, terminal, replay, latency, cost,
   binding, baseline, and comparison contracts remain unchanged.
7. The builder must use the matched case contract; order cannot substitute for
   case identity.

## State, interface, compatibility, and recovery

- No database, filesystem, baseline, or report schema changes occur.
- Rust callers constructing observations must provide the new field.
- Serialized observations missing the field become invalid by design. This is the
  fail-closed compatibility boundary; unknown extra fields retain existing Serde
  behaviour.
- A rejected build returns no report and performs no baseline mutation.
- Retrying with the same observations plus truthful trace presence is deterministic
  and follows the existing report hash contract.
- No concurrency, ownership, crash-recovery, or rollback mechanism changes because
  report construction is in-memory and side-effect free.

## Acceptance criteria

- A required-trace observation with `trace_present: false` is rejected.
- A serialized observation omitting `trace_present` is rejected.
- All ten Priority-2 observations with `trace_present: true` still produce the
  existing deterministic report and metrics.
- A valid custom case with `trace_required: false` accepts
  `trace_present: false`.
- Focused, canonical, Engineering Memory, exact-scope, and detached-tree gates
  pass.

## Execution plan and ledger

### Slice 1 — enforce declared trace requirements

- **Outcome:** untraced required observations cannot enter a report.
- **Dependencies:** existing dataset validation and observation identity matching.
- **RED:** a focused test sets one observation's absent `trace_present` field to
  false and expects rejection; compilation fails because the field does not exist.
- **GREEN:** add the required field and validate it against the identity-matched
  case contract before metrics are computed.
- **Refactor:** carry the matched case contract instead of a tool-count-only map;
  update the shared observation fixture once.
- **Verification:** selected rejection test, full evaluation contracts, strict
  focused Clippy, and JSON omission/optional-trace acceptance.
- **Complete when:** required missing trace fails, optional missing trace succeeds,
  and all existing report/baseline contracts remain green.
- **Observed evidence:** RED failed because `EvaluationObservation` had no
  `trace_present` field. GREEN rejected a false required trace before metrics;
  boundary checks rejected JSON omission, accepted false for an optional-trace
  case, and retained the identical report and hash for optional presence.

## Final verification

- Full evaluation contracts and affected kernel tests.
- Engineering Memory tests, generation, strict validation, and currentness.
- `cargo fmt --all --check`.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- `cargo test --workspace --all-features`.
- `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps`.
- Exact diff/path review and detached staged-tree focused verification.

## Prohibited actions

- Do not treat an observation boolean as proof of a `TraceStore` span or child
  span.
- Do not fabricate trace presence in production observation producers.
- Do not weaken dataset, binding, metric, baseline, or comparison validation.
- Do not manually edit generated Engineering Memory JSON.
- Do not create a branch or pull request, install dependencies, deploy, release,
  publish, access credentials, or modify unrelated paths.

## Assumptions and unresolved work

- **Reasonable inference:** trace-presence validation belongs at report construction
  because that is the existing fail-closed evidence boundary.
- **Unresolved:** integrity-case trace production must be implemented before one
  honest ten-case report can be produced from the executable suites.
