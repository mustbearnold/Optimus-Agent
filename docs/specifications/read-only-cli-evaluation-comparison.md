---
knowledge_type: specification
status: historical
covers:
  - apps/optimus-cli/src/main.rs
  - apps/optimus-cli/tests/eval_compare.rs
  - scripts/engineering_memory.py
depends_on:
  - docs/specifications/candidate-aware-evaluation-comparison.md
  - docs/specifications/authoritative-offline-candidate-binding.md
validated_by:
  - apps/optimus-cli/tests/eval_compare.rs
last_verified_commit: null
---

# Read-only CLI evaluation comparison

Date: `2026-07-20`

## Problem and outcome

The kernel can compare two valid `EvaluationReportV1` documents, including reports
from different source trees, but the CLI exposes only report execution. Operators
must write custom code to use the existing comparison authority.

The observable outcome is a bounded `optimus eval compare` operation that accepts
baseline and candidate report JSON, invokes `compare_evaluation_reports`, and prints
exactly one `EvaluationComparison` JSON document without creating or changing
`--home` or either input.

## Repository truth

### Observed facts

- `compare_evaluation_reports` verifies both report hashes, dataset and non-source
  context, threshold policy, and metric-key equality before classification.
- Source-tree identity alone may differ.
- Quality metrics increase toward improvement; latency and cost decrease toward
  improvement.
- `EvaluationComparison` contains report hashes and deterministic
  improved/equal/regressed metric lists.
- `read_bounded_json` already limits each CLI JSON input to one MiB.
- `run` currently creates `--home` before command dispatch, including commands that
  need no state.

### Reasonable inference

A valid comparison containing regressions is evidence, not an accepted gate policy.
The CLI should return success after printing it. Existing report thresholds remain
the only implemented evaluation gate.

### Unresolved assumptions

None. Baseline acceptance and release policy are explicitly excluded.

## Scope

- Add `optimus eval compare --baseline REPORT --candidate REPORT`.
- Read each report through the existing one-MiB bounded JSON loader.
- Invoke the canonical kernel comparator without reimplementing validation or metric
  direction.
- Serialize exactly one pretty JSON `EvaluationComparison` on success.
- Dispatch comparison before generic `--home` initialization.
- Add compiled-binary success and failure coverage.
- Refresh current authority and generated Engineering Memory.

## Non-scope

- Baseline storage, acceptance, replacement, or migration.
- New thresholds, regression tolerances, or non-zero exit policy for valid
  regressions.
- Report generation, live providers, routing, CI/release gates, or desktop UI.
- Schema or dependency changes.

## Contracts and invariants

1. Baseline and candidate inputs are independently limited to
   `MAX_EVALUATION_DATASET_BYTES` and must deserialize as `EvaluationReportV1`.
2. The command calls `compare_evaluation_reports`; CLI code does not duplicate or
   weaken report verification, context compatibility, or metric direction.
3. Success stdout contains exactly one complete `EvaluationComparison` JSON document
   and returns zero even when `regressed` is non-empty.
4. Invalid JSON, oversized input, invalid report hashes/arithmetic, context drift,
   threshold drift, or metric drift returns non-zero and emits no plausible
   comparison JSON on stdout.
5. Comparison creates no `--home`, database, run directory, baseline, or output file.
6. Input files are read-only and remain byte-identical.
7. Equal valid input bytes produce equal output bytes regardless of `--home`.
8. Existing `eval run`, `eval report`, binding generation, and kernel APIs remain
   compatible.

## State, interface, and compatibility

```text
optimus --home PATH eval compare --baseline baseline.json --candidate candidate.json
```

Comparison must be handled before the current generic `create_dir_all(&cli.home)`.
The normal stateful command path remains unchanged.

## Failure, interruption, and concurrency

The operation has no durable writes, leases, or rollback. Failure or interruption
leaves inputs and `--home` unchanged. Concurrent comparisons share no state and are
deterministic. A retry is safe with unchanged evidence.

## Execution ledger

### Slice 1 — bounded mutation-free comparison command

- **Outcome:** two compatible exact reports produce one canonical comparison through
  the compiled CLI while invalid evidence produces none.
- **Dependencies:** existing report loader, comparator, and JSON types.
- **RED:** compiled-binary test invokes `eval compare`; Clap rejects the absent
  subcommand.
- **GREEN:** add the command, pre-home read-only dispatch, bounded loading, canonical
  comparison, and exact JSON serialization.
- **Refactor:** isolate read-only dispatch only if needed to keep stateful command
  initialization unchanged.
- **Verification:** success with distinct source trees and a real regression;
  malformed, oversized, tampered, and context-drifted inputs; input-byte and absent
  home assertions; complete CLI test and strict Clippy surfaces.
- **Complete when:** every success/failure and mutation boundary is observed through
  the compiled binary.
- **Observed evidence:** the valid test first failed because Clap rejected the
  absent subcommand. GREEN compared different source trees with one latency
  regression, returned success, produced byte-identical JSON across two absent
  homes, preserved both inputs, and created no home. Boundary coverage rejected
  malformed input, independently oversized inputs, a stale report hash, and route
  context drift with empty stdout and no mutation. Comparison tests passed 2/2,
  report compatibility 4/4, evaluation contracts 19/19, and strict focused Clippy.

## Acceptance criteria

- Distinct source-tree reports compare successfully when fixed context matches.
- Output hashes identify the exact two reports and metric classes are correct.
- A valid regression remains a successful comparison operation.
- Invalid or incompatible evidence emits no comparison JSON.
- Each input is bounded independently.
- Inputs remain unchanged and a missing `--home` remains missing.
- Existing report and legacy evaluation commands remain green.

## Final verification

- Focused compiled-binary comparison and report integration tests.
- Evaluation contract regression tests.
- Engineering Memory tests, generation, strict validation, and currentness.
- Workspace format, strict Clippy, all-feature tests, and strict rustdoc once on final
  bytes.
- Exact diff, generated-artifact, protected-path, detached staged-tree, post-commit,
  and independent remote verification.

## Prohibited actions

- Do not mutate or accept baselines during comparison.
- Do not add implicit regression tolerance or gate policy.
- Do not weaken report verification or fabricate report/metric evidence.
- Do not create state for a read-only comparison.
- Do not manually edit generated Engineering Memory JSON.
- Do not add dependencies, create a branch or pull request, deploy, release, publish,
  access credentials, rewrite history, or modify unrelated paths.
- One verified commit and push to `origin/main` are authorized after all gates pass.
