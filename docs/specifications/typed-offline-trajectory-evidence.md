---
knowledge_type: specification
status: historical
covers:
  - crates/optimus-kernel/src/eval.rs
  - crates/optimus-kernel/src/lib.rs
  - crates/optimus-kernel/tests/evaluation_contracts.rs
  - apps/optimus-cli/src/main.rs
  - docs/architecture/system-overview.md
  - docs/maps/observability-and-evaluations.md
  - scripts/engineering_memory.py
  - scripts/test_engineering_memory.py
---

# Typed offline trajectory evidence

**Date:** 2026-07-20

## Problem and outcome

**Observed fact:** the four built-in offline trajectories execute real kernel
turns, but `EvalCaseResult` retains only a summary boolean, diagnostic text,
formatted tool strings, and assistant text. It drops canonical invoked-tool IDs,
terminal execution status, replay classification, and the persisted root trace.

**Observed fact:** those typed values exist after a successful turn in
`TurnResult` and `ExecutionStore`. Without them, a future report producer would
have to infer or fabricate evidence.

**Intended outcome:** one exact public four-case runner returns the existing result
shape enriched with canonical tools and independently reloaded terminal, replay,
and trace evidence. Failed cases expose no typed success evidence.

## Scope

- Add optional typed evidence fields to `EvalCaseResult`.
- Reload and cross-check the completed turn's execution manifest, replay report,
  and trace link in `run_case`.
- Add `run_offline_trajectory_suite(home)` as the exact built-in four-case API.
- Route the CLI's built-in eval command through that API.
- Prove exact case order, expected assistant text, canonical tools, terminal status,
  fixture replay classification, and trace identity.
- Cover unusable-home failure without fabricated typed evidence.
- Update evaluation authority and generated Engineering Memory.

## Non-scope

- Producing `EvaluationObservation` or `EvaluationReportV1`.
- Defining unavailable latency/cost semantics.
- Changing the ten-case dataset, metrics, thresholds, baselines, or comparison.
- Claiming integrity-case trace coverage.
- Live providers, network evaluation, release/CI gates, or routing policy.
- `TraceStore` spans, child spans, or distributed tracing.

## Authoritative existing behaviour

- `builtin_suite` defines exactly four trajectories in canonical order.
- `run_suite` isolates cases under deterministic case directories and returns one
  result per input case, continuing after individual failures.
- Successful kernel turns atomically retain one execution root trace and expose it
  through `TurnResult`.
- `ExecutionStore` owns terminal manifest status and aggregate replay
  classification.
- The Priority-2 dataset expects successful, fixture-replayable trajectories with
  exact assistant text and canonical tool identities.

## Contracts and invariants

1. The exact runner executes only `builtin_suite()` in its declared order.
2. A successful case reloads the unique session turn and execution manifest from
   the case's isolated home.
3. The manifest must be terminal `Succeeded`.
4. The persisted trace must exist and equal `TurnResult.trace_context`.
5. Replay classification comes from `ExecutionStore::replay_report`, not a
   hardcoded expectation.
6. Canonical invoked tools come from `TurnResult.invoked_tools`; formatted
   `tool_trace` remains diagnostic only.
7. Assistant text remains the exact returned text.
8. Any missing, duplicate, mismatched, malformed, non-success, or unreadable
   persisted evidence makes `run_case` fail.
9. `run_suite` converts a failed case into a failed result with empty/`None` typed
   fields. It may not synthesize successful status, replay, tools, or trace.
10. Additive optional fields preserve existing JSON readers; absent fields default
    during deserialization.
11. No evaluation metric, baseline, report, runtime effect, permission, or policy
    semantics change.

## State, compatibility, failure, and recovery

- State remains the existing per-case session, execution, runtime, memory, and
  workspace data. No schema changes occur.
- Each case remains isolated. Partial failure in one case does not supply evidence
  to another.
- An unusable evaluation home yields four failed results, no typed success
  evidence, and no mutation of the obstructing caller path.
- Retrying with a usable fresh home creates independent trace identities while
  preserving canonical case/result semantics.
- This milestone does not clean up case directories or rewrite historical data.
- Existing `run_case`, `run_suite`, `EvalReport`, and CLI JSON remain compatible;
  results gain additive fields.

## Acceptance criteria

- The public exact runner returns four passing cases in canonical order.
- Every passing case has:
  - exact expected assistant text;
  - exact canonical invoked tools;
  - `ExecutionStatus::Succeeded`;
  - `ReplayClassification::FixtureReplayable`;
  - one parentless trace equal to persisted execution evidence.
- The file-writing case still uses its durable local effect and no network/provider
  access.
- An unusable home returns four failed cases with no typed success evidence and
  preserves the obstructing file bytes.
- Focused, canonical, Engineering Memory, exact-scope, and detached-tree gates
  pass.

## Execution plan and ledger

### Slice 1 — exact typed trajectory evidence

- **Outcome:** callers receive honest typed evidence for all four built-ins.
- **Dependencies:** atomic kernel-turn trace binding and existing replay reports.
- **RED:** a focused contract imports the absent exact runner and requires typed
  fields not present on `EvalCaseResult`.
- **GREEN:** add optional fields, reload/cross-check persisted evidence in
  `run_case`, expose the exact runner, and use it in the CLI.
- **Refactor:** centralize failed-case construction so every failure clears typed
  evidence.
- **Verification:** selected contract, full eval/evaluation tests, CLI compile, and
  focused strict Clippy.
- **Complete when:** success evidence is exact and unusable-home failures contain
  no fabricated typed values.
- **Observed evidence:** RED failed on the absent public runner. GREEN returned all
  four cases with exact dataset text/tools, successful terminal status, persisted
  fixture-replay classification, and parentless traces. The unusable-home contract
  returned four failures with empty/`None` typed fields and preserved the blocking
  file. The real CLI JSON path emitted all four typed records.

## Final verification

- Focused eval, evaluation, kernel-turn, replay, and CLI tests as applicable.
- Engineering Memory tests, generation, strict validation, and currentness.
- `cargo fmt --all --check`.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- `cargo test --workspace --all-features`.
- `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps`.
- Exact diff/path review and detached staged-tree focused verification.

## Prohibited actions

- Do not fabricate trace, replay, terminal, tool, latency, or cost evidence.
- Do not claim a complete ten-case report producer.
- Do not weaken exact tool, replay, cancellation, approval, or trace contracts.
- Do not manually edit generated Engineering Memory JSON.
- Do not create a branch or pull request, install dependencies, deploy, release,
  publish, access credentials, or modify unrelated paths.

## Assumptions and unresolved work

- **Reasonable inference:** additive optional evidence is the narrowest compatible
  bridge from the legacy harness to a typed producer.
- **Unresolved:** latency/cost availability and honest integrity-case trace
  semantics must be settled before producing one complete ten-case report.
