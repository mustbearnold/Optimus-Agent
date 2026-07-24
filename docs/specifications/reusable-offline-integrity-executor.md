---
knowledge_type: specification
status: historical
covers:
  - crates/optimus-eval/src/eval.rs
  - crates/optimus-kernel/src/lib.rs
  - crates/optimus-eval/tests/integrity_integration.rs
  - docs/maps/observability-and-evaluations.md
  - scripts/engineering_memory.py
  - scripts/test_engineering_memory.py
depends_on:
  - docs/specifications/fail-closed-evaluation-report-validation.md
  - docs/decisions/0023-fixture-replay-trace-telemetry-evaluation.md
validated_by:
  - crates/optimus-eval/tests/integrity_integration.rs
last_verified_commit: c3aa28fb78a47a485dc1cb57d7066695f8d0d195
---

# Reusable offline integrity executor

Date: `2026-07-20`

## Problem and outcome

The six required integrity observations are executed only inside an integration
test and then passed to `evaluate_integrity_observations`. Production code can
validate supplied observations but cannot produce them. A complete ten-case
versioned report or CLI gate would therefore have to omit or fabricate evidence.

The outcome is a public kernel function that executes all six existing offline
integrity cases against real subsystem boundaries and returns the existing exact,
ordered `EvalReport`. The integration test consumes that function instead of
owning a second implementation.

## Repository truth

### Observed facts

- `REQUIRED_INTEGRITY_EVALS` defines six exact ordered case IDs.
- The current integration test executes memory clearance, SmartDeny, routing,
  agent cancellation/fencing, and gateway dead-letter behavior using local state.
- `evaluate_integrity_observations` already rejects missing, duplicate, or
  evidence-free cases.
- No reusable executor exists, and the four-case trajectory runner does not
  produce these observations.

### Inference

Moving the real execution seam into the kernel evaluation module is the smallest
prerequisite for honest complete report production. Each expected denial must be
matched by its policy-specific outcome rather than treating every error as a pass.

### Unresolved assumptions

None. Conversion from the six-case and four-case reports into
`EvaluationObservation` remains a separate milestone.

## Scope

- Add `run_offline_integrity_suite(home)` as a public kernel API.
- Execute the six exact required cases using existing subsystem contracts.
- Match sensitivity, approval, route-policy, cancellation, stale-completion, and
  dead-letter outcomes specifically.
- Return all six cases even when setup or execution for one case fails.
- Isolate each invocation under a unique run directory beneath the caller-owned
  home.
- Replace the integration-test-owned execution body with the reusable API.
- Prove successful retries and failed setup produce deterministic reports.
- Refresh current evaluation documentation and generated Engineering Memory.

## Non-scope

- Building `EvaluationReportV1` or combining the four trajectory cases.
- CLI, CI, release, routing, or deployment gates.
- Live providers, network requests, approved command execution, or external
  effects.
- New report, database, agent, workflow, or gateway schema versions.
- Cleanup, migration, or deletion of prior evaluation runs.

## Contracts and invariants

1. The report contains exactly `REQUIRED_INTEGRITY_EVALS` in canonical order.
2. Every case is produced by executing its real local subsystem boundary; no
   caller-supplied boolean can authorize a pass.
3. Sensitivity passes only for the expected `WriteDenied` clearance failure.
4. SmartDeny passes only for `NeedsApproval` on the exact created job/node and
   absence of the prohibited file; the command must never start.
5. Route policy passes only when the requested remote Codex route is rejected for
   local-only privacy.
6. Cooperative cancellation passes only when a durable request synchronizes the
   exact token and the token is cancelled.
7. Stale-completion fencing passes only when late success is rejected after that
   cancellation and the invocation is terminalized as cancelled.
8. Gateway dead-letter passes only after two retry outcomes, one dead-letter
   outcome, and exact terminal delivery state.
9. Setup or execution errors become failed cases with non-empty stable evidence;
   they never become passing denials and do not suppress other case records.
10. Each invocation uses `integrity-runs/<UUID>` under the supplied home. Run IDs
    and filesystem paths are not emitted in the report, so equal behavior yields
    equal serialized reports.
11. The executor performs no network request or process effect. SmartDeny must
    reject before process creation.
12. Existing `evaluate_integrity_observations` and `EvalReport` interfaces remain
    compatible.

## State, interruption, and recovery

- The caller owns the supplied home and all local SQLite/filesystem state below
  its unique run directory.
- Runs do not share mutable files, so concurrent or repeated calls cannot collide.
- A crash may leave one incomplete run directory; no external effect is possible
  and no automatic deletion or rollback occurs.
- Retry creates a new isolated run and never resumes or trusts partial evidence.
- Failed cases are terminal report observations, not retry authorization.
- The function does not mutate baseline storage or generated authority.

## Vertical execution ledger

### Slice 1 — reusable real execution

- **Outcome:** library callers execute and receive all six real integrity cases.
- **Dependencies:** current subsystem contracts and exact evaluation catalog.
- **RED:** change the integration contract to call the absent public executor;
  compilation fails on the missing API.
- **GREEN:** move the smallest real case orchestration into `eval.rs`, export it,
  and match expected typed outcomes.
- **Refactor:** remove obsolete test-owned orchestration/imports while retaining
  unrelated cross-contract helpers.
- **Verify:** exact integration test executes six passing cases in canonical order.
- **Complete when:** the test uses only the public executor for those six cases.

### Slice 2 — isolation and truthful failure

- **Outcome:** reruns are deterministic and unusable homes yield six failed,
  evidence-bearing cases rather than errors masquerading as passes.
- **Dependencies:** Slice 1 executor and unique run ownership.
- **Pre-change evidence:** isolated successful retries were already green because
  unique ownership was required by Slice 1; no artificial RED was manufactured.
- **RED:** an unusable home returned one passing stateless route case and five
  failures instead of one owned six-case run.
- **GREEN:** require run-directory ownership before executing any case and emit
  six stable setup failures when that precondition fails.
- **Refactor:** share case-result construction only where it removes duplication.
- **Verify:** focused integration tests, formatting, and focused strict Clippy.
- **Complete when:** two successful runs compare equal and two failed runs compare
  equal with all six cases failed and evidenced.

## Acceptance criteria

- Public API executes exactly six canonical cases and all pass on a usable home.
- The old integration test no longer implements those cases itself.
- Security denials are outcome-specific, not generic `is_err()` checks.
- Two calls under one home do not collide and return equal reports.
- An invalid home returns six ordered failures with stable non-empty evidence.
- No process file is created, network is not used, and no baseline is mutated.

## Verification

Focused:

```text
cargo test -p optimus-kernel --test integrity_integration offline_integrity_executor -- --nocapture
cargo test -p optimus-kernel --test integrity_integration
cargo clippy -p optimus-kernel --test integrity_integration -- -D warnings
```

Final:

```text
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps
python -m unittest scripts/test_engineering_memory.py -v
python scripts/engineering_memory.py generate
python scripts/engineering_memory.py validate --strict
python scripts/engineering_memory.py check
```

Then inspect exact paths, verify a config-neutral detached staged tree, commit once,
push `main`, fetch, and prove local/tracking/remote SHA agreement.

## Prohibited actions

- Do not treat arbitrary infrastructure errors as successful security denials.
- Do not execute an approved command, access the network, or fabricate evidence.
- Do not build the ten-case versioned report or CLI gate in this milestone.
- Do not delete prior run state, change schemas, or add dependencies.
- Do not create branches or pull requests, deploy, release, publish, access
  credentials, or modify unrelated files.
- One verified commit and push to `origin/main` are authorized after all gates
  pass.
