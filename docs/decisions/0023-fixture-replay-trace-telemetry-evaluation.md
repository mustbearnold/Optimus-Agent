---
doc_id: decisions-0023-fixture-replay-trace-telemetry-evaluation
doc_type: decision
plane: decision
status: current
authority: record
summary: Decision record for ADR-0023: Fixture-only replay, causal traces, routing telemetry, and versioned evaluation, including its context, consequences, and current documentary status.
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: decision
covers:
  - crates/optimus-eval/src/replay.rs
  - crates/optimus-kernel/src/trace.rs
  - crates/optimus-kernel/src/telemetry.rs
  - crates/optimus-kernel/src/routing.rs
  - crates/optimus-eval/src/evaluation.rs
  - crates/optimus-kernel/src/execution.rs
  - crates/optimus-packs/src/lib.rs
  - crates/optimus-eval/tests/replay_contracts.rs
  - crates/optimus-kernel/tests/trace_contracts.rs
  - crates/optimus-kernel/tests/routing_telemetry.rs
  - crates/optimus-eval/tests/evaluation_contracts.rs
depends_on:
  - docs/decisions/0021-owned-execution-and-causal-delivery.md
  - docs/decisions/0022-versioned-agent-and-workflow-contracts.md
  - docs/contracts/high-risk-contracts.md
validated_by:
  - crates/optimus-eval/tests/replay_contracts.rs
  - crates/optimus-kernel/tests/trace_contracts.rs
  - crates/optimus-kernel/tests/routing_telemetry.rs
  - crates/optimus-eval/tests/evaluation_contracts.rs
  - crates/optimus-packs/tests/packs_budget.rs
last_verified_commit: 09fddbc1b60a6b37f9f80680988ea5036a9b8eec
---

# ADR-0023: Fixture-only replay, causal traces, routing telemetry, and versioned evaluation

- **Status:** Accepted
- **Date:** 2026-07-20

## Context

Execution manifests classified model/tool stages and retained hashes, but Optimus
could not retain bounded fixtures or execute an offline replay. Cross-subsystem
correlation relied on unrelated identifiers. Tool operational metadata was
partly duplicated outside `ToolDesc`. Routing used static policy only.
Evaluations used ad hoc substring/tool checks without candidate binding,
thresholds, or immutable baselines. C-13 therefore remained partial.

## Decision

1. Replay evidence is a versioned immutable bundle linked to one terminal source
   manifest. Fixture identity is the SHA-256 of bounded bytes.
2. Replay planning verifies source, trace, policy, tool catalog, stage order,
   fixture completeness, content hashes, and terminal evidence before execution.
3. The replay executor receives only validated replay-store data. It has no
   provider, network, process, runtime, approval, or writable-workspace handle.
4. Replay compares exact fixture/input hashes and appends one immutable report.
   It never rewrites the source execution or bundle.
5. Live provider/external/destructive work is not rerun. An accepted fixture may
   permit offline comparison, but does not prove the live service was reproduced.
6. `TraceId`, `SpanId`, and `TraceContext` form the canonical local causal
   vocabulary. Trace spans/events are append-only and settle once.
7. Route decisions and execution manifests may bind exact trace contexts.
   These are independently stored causal references, not a distributed transaction.
8. `ToolDesc` owns retry, idempotency, timeout ownership, cancellation support,
   and observability declarations derived from canonical invocation identity.
9. Telemetry observations are append-only and must match an existing route's
   provider/model/trace identity. Aggregates use bounded checked integer math.
10. Static privacy/capability/budget policy runs before telemetry. Telemetry can
    filter/rank only already-approved candidates, only under explicit freshness,
    sample, success, latency, fallback, and missing-evidence policy.
11. Evaluation datasets/cases/reports/baselines are versioned. Reports bind exact
    dataset, source tree, contract, tool catalog, route policy, provider, and model
    hashes and contain deterministic checked metrics and explicit thresholds.
12. Baselines are immutable reports. Comparisons are deterministic and never
    silently alter threshold policy.

## Alternatives considered

### Rerun every historical effect

Rejected. Repeating process, network, browser mutation, or filesystem-write
stages can duplicate irreversible effects and cannot prove equivalence.

### Store fixtures as loose files

Rejected. Loose files make atomic metadata/blob insertion, bounded reads,
immutability, and corruption detection harder to enforce.

### Use distributed tracing infrastructure

Rejected for this tranche. Optimus is local and SQLite-authoritative; an external
collector would add availability and trust boundaries without fixing causal IDs.

### Let telemetry choose any provider

Rejected. Operational evidence is not authorization and cannot override privacy,
capability, budget, or explicit fallback policy.

### Floating-point metrics and mutable “latest” baselines

Rejected. Checked integer/rational metrics and immutable report hashes are easier
to reproduce, review, and compare across machines.

## Reasons

The chosen contracts close replay and evaluation gaps using deterministic code,
content-addressed evidence, bounded stores, and explicit policy. They preserve
existing subsystem authority, avoid effect duplication, and remain CPU-first.

## Consequences

- Offline fixture replay can be independently verified and corrupted evidence
  fails before later stages.
- Trace links improve causal inspection without granting permissions or claiming
  cross-store atomicity.
- Tool metadata has one canonical descriptor owner.
- Routing can use fresh measured evidence without weakening static policy.
- Evaluation output is candidate-bound and byte-deterministic for equal inputs.
- Callers must explicitly create/bind traces and record telemetry; old untraced
  APIs remain valid but provide less evidence.

## Risks

- A fixture can faithfully reproduce recorded bytes while the historical remote
  service had additional unobserved behavior.
- SQLite stores remain independently authoritative; crashes can leave a valid
  record in one store before its causal link is written elsewhere.
- Tool declarations describe supported contracts; owner-specific runtime paths
  must continue proving cancellation/timeout behavior.
- Telemetry can be sparse or stale; missing evidence behavior must remain explicit.
- Evaluation metrics cover declared offline properties, not general factual truth.

## Evaluation evidence

- Replay tests cover validation, atomic insertion, corruption, reopen,
  source/trace/policy drift, zero-effect execution, mismatch fail-fast, and one
  report per bundle.
- Trace tests cover parent/cross-trace/duplicate rejection, event order, terminal
  fencing, route identity, and execution-manifest binding.
- Packs tests mutate operational metadata and require catalog rejection.
- Telemetry tests cover route provenance, integer aggregates, thresholds, and
  explicit healthy fallback after static capability filtering.
- Evaluation tests cover exact ten-case datasets, bounded JSON, deterministic
  report bytes/hash, checked metrics/thresholds, immutable baselines, and
  deterministic regression classification.

## Conditions for reconsideration

Reconsider when local model inference, distributed execution, or external trace
export is implemented. Any extension must retain exact identities, bounded data,
permission isolation, one terminal outcome, CPU fallback, and executable tests.

## Relevant code

- `crates/optimus-eval/src/replay.rs`
- `crates/optimus-kernel/src/trace.rs`
- `crates/optimus-kernel/src/telemetry.rs`
- `crates/optimus-kernel/src/routing.rs`
- `crates/optimus-eval/src/evaluation.rs`
- `crates/optimus-kernel/src/execution.rs`
- `crates/optimus-packs/src/lib.rs`

## Relevant tests

- `crates/optimus-eval/tests/replay_contracts.rs`
- `crates/optimus-kernel/tests/trace_contracts.rs`
- `crates/optimus-kernel/tests/routing_telemetry.rs`
- `crates/optimus-eval/tests/evaluation_contracts.rs`
- `crates/optimus-packs/tests/packs_budget.rs`
