---
knowledge_type: specification
status: historical
covers:
  - crates/optimus-eval/src/eval.rs
  - crates/optimus-eval/tests/integrity_integration.rs
  - docs/architecture/system-overview.md
  - docs/maps/observability-and-evaluations.md
  - scripts/engineering_memory.py
  - scripts/test_engineering_memory.py
---

# Persisted integrity evaluation traces

**Date:** 2026-07-20

## Problem and outcome

**Observed fact:** the Priority-2 dataset requires trace evidence for all ten
cases. The four trajectory cases return persisted execution roots, while the six
integrity cases return only pass/fail text. The report builder now rejects a
required case without trace presence.

**Intended outcome:** every executed integrity case owns one persisted root span
whose lifecycle starts before the subsystem check, records bounded hashed evidence,
and terminalizes with the case outcome. Successful results expose coherent trace,
terminal, and replay evidence. Setup failures that cannot establish trace storage
remain explicit untraced failures.

## Scope

- Persist one `TraceStore` root span per required integrity case in the isolated
  run directory.
- Start spans before the owned subsystem checks and settle them exactly once.
- Append one bounded evidence event containing only the digest of the public case
  evidence.
- Expose exact trace context, typed terminal status, and deterministic replay class
  through integrity observations/results.
- Verify persisted span identity, subject, status, event ordering, retry isolation,
  and unusable-home failure semantics.
- Update observability authority and generated Engineering Memory.

## Non-scope

- Converting the ten cases into `EvaluationObservation` or producing
  `EvaluationReportV1`.
- Adding child spans inside memory, runtime, routing, agent, gateway, or workflow
  subsystems.
- Claiming distributed tracing, OpenTelemetry export, or transactionality across
  subsystem databases.
- Changing the six integrity checks, dataset, metrics, thresholds, baselines, or
  comparisons.
- Live providers, network evaluation, release gates, latency, or cost collection.

## Authoritative existing behaviour

- `run_offline_integrity_suite` creates a fresh UUID-named run directory and
  executes exactly six local cases in canonical order.
- Run-directory creation failure returns six deterministic failed results without
  mutating the obstructing path.
- `TraceStore` persists parentless and child spans, ordered events, and exactly-once
  terminal settlement in SQLite.
- `EvalCaseResult` already has optional typed terminal, replay, and trace fields.
- Integrity checks are deterministic local fixtures; expected denials are passing
  evaluation outcomes and do not execute approved commands or access the network.

## Contracts and invariants

1. A usable integrity run owns one `integrity-traces.db` under its unique run
   directory.
2. Exactly six parentless root spans are created with subsystem `evaluation` and
   subject equal to the canonical case ID.
3. Each root exists before its owned subsystem check begins. The cooperative
   cancellation and stale-completion cases may share one underlying check, but
   both roots must exist before that check starts.
4. Each span receives one `Evidence` event whose payload is the SHA-256 digest of
   the bounded public evidence string; raw subsystem state or secrets are not
   copied into trace storage.
5. A passing case settles `SpanStatus::Succeeded`; a failed check settles
   `SpanStatus::Failed`. Settlement appends the existing terminal event.
6. Returned trace context must equal the persisted parentless context. Returned
   terminal status is `Succeeded` for a passing case and `Failed` for a failed
   executed case. Replay classification is `Deterministic`.
7. `evaluate_integrity_observations` rejects inconsistent typed evidence: a trace
   without terminal/replay fields, typed fields without a trace, a passing case
   without succeeded status, a failed case without failed status, or a
   non-deterministic replay class.
8. Setup failures before trace storage exists carry `None` for trace, terminal,
   and replay evidence; they may not fabricate identities.
9. Case order, evidence text, pass/fail counts, and subsystem effects remain
   unchanged.
10. Independent retries have fresh run and trace identities. After removing those
    identities, their semantic result bytes remain equal.

## State, interruption, failure, and recovery

- Each run writes only inside its new owned run directory. Existing runs are
  immutable historical evidence and are never resumed, overwritten, or cleaned.
- A case function returning an expected error still reaches traced settlement as a
  failed evaluation case.
- Failure to open trace storage, create a span, append evidence, settle, or read
  back the persisted span aborts the suite with `Err`; no successful report is
  returned. Already-written evidence remains for diagnosis.
- A process crash or panic may leave a running span and no report. A retry uses a
  new run directory and cannot settle or mutate the abandoned span.
- `TraceStore::settle` owns atomic status/terminal-event mutation. This milestone
  adds no cross-database transaction and makes no such claim.
- Concurrent suite invocations use distinct UUID run directories and databases;
  no shared lease or row ownership exists.
- Unusable-home retries preserve the existing deterministic six-failure response
  and the obstructing file bytes.

## Interface and compatibility

- `IntegrityObservation` gains optional trace, terminal, and replay fields.
  Existing serialized inputs omitting them deserialize as `None`.
- Exact executor success now populates those fields. Generic untraced failure
  observations remain representable only as an all-`None` evidence group.
- `EvalReport` JSON gains populated optional fields for successful integrity runs;
  fresh trace UUIDs make raw successful reports intentionally identity-unique.
- No database schema migration is needed; each run creates a new trace database
  using the existing schema.

## Acceptance criteria

- The exact executor returns six passing canonical cases with parentless trace,
  succeeded terminal status, and deterministic replay evidence.
- Reopening the run trace database proves six matching succeeded spans, each with
  ordered `Evidence` then `Terminal` events.
- Two retries produce distinct trace/span IDs but identical semantic results after
  trace identities are removed.
- A deliberately failed executed observation is persisted as a failed span and
  exposes failed terminal status without claiming a passing case.
- Inconsistent typed integrity observations are rejected.
- Unusable-home execution remains six stable untraced failures and preserves the
  blocking file.
- Focused, canonical, Engineering Memory, exact-scope, and detached-tree gates
  pass.

## Execution plan and ledger

### Slice 1 — persisted per-case trace lifecycle

- **Outcome:** each executed integrity result references a real terminal root span.
- **Dependencies:** existing isolated run ownership and `TraceStore` contracts.
- **RED:** extend the exact executor integration test to require a returned root
  context and reopened succeeded span/events; compilation fails because integrity
  results do not carry trace contexts and no trace database exists.
- **GREEN:** add trace begin/evidence/settle/readback helpers, wrap all six checks,
  and forward contexts into results.
- **Refactor:** centralize single-case begin/finalize logic; explicitly pre-begin
  the two roots around the shared cancellation check.
- **Verification:** selected integration test and trace contracts.
- **Complete when:** six exact persisted roots and ordered events are independently
  reopened and matched to returned contexts.
- **Observed evidence:** RED failed on the first missing result context. GREEN
  independently reopened six parentless succeeded roots, each with ordered
  `Evidence` then `Terminal` events and a subject matching its canonical case ID.

### Slice 2 — coherent typed outcomes and retry/failure semantics

- **Outcome:** integrity evidence is directly usable for terminal/replay/trace
  projection without fabricated values.
- **Dependencies:** Slice 1 terminal spans.
- **RED:** require succeeded/deterministic typed values, reject an inconsistent
  observation, and require retry identities to differ while normalized semantics
  remain equal; current integrity results expose `None` and raw report equality
  assumes no identities.
- **GREEN:** add optional typed fields to `IntegrityObservation`, validate their
  all-or-none/status/replay coherence, and project them to `EvalCaseResult`.
- **Refactor:** centralize typed failure construction for pre-trace setup failure.
- **Verification:** full integrity integration and focused strict Clippy.
- **Complete when:** success, executed failure, retry isolation, inconsistency, and
  unusable-home contracts pass.
- **Observed evidence:** RED failed because `IntegrityObservation` lacked terminal
  and replay fields. GREEN rejected typed outcomes without a trace, returned
  succeeded/deterministic evidence for all six exact cases, persisted a forced
  failed case as a failed span, produced fresh retry identities with equal
  normalized semantics, rejected both partial typed evidence and untraced passing
  observations, and kept unusable-home failures untraced and stable.

## Final verification

- Integrity integration, trace contracts, evaluation contracts, and affected unit
  tests.
- Engineering Memory tests, generation, strict validation, and currentness.
- `cargo fmt --all --check`.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- `cargo test --workspace --all-features`.
- `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps`.
- Exact diff/path review and detached staged-tree focused verification.

## Prohibited actions

- Do not attach random UUIDs without persisted spans.
- Do not expose raw secret or subsystem state as trace evidence.
- Do not treat expected security denial as execution failure.
- Do not claim cross-store atomicity, child-span coverage, or a complete ten-case
  report producer.
- Do not manually edit generated Engineering Memory JSON.
- Do not create a branch or pull request, install dependencies, deploy, release,
  publish, access credentials, or modify unrelated paths.

## Assumptions and unresolved work

- **Reasonable inference:** one evaluation-owned root per case is the narrowest
  honest trace contract until subsystems emit their own child spans.
- **Unresolved:** the future observation producer must measure or define honest
  latency/cost semantics before generating one complete report.
