---
knowledge_type: observability-evaluation-map
status: current
covers:
  - crates/optimus-store/src/**
  - crates/optimus-graph/src/**
  - crates/optimus-runtime/src/**
  - crates/optimus-kernel/src/eval.rs
  - crates/optimus-kernel/src/lib.rs
  - apps/optimus-desktop/src/server.rs
  - apps/optimus-desktop/e2e/**
depends_on:
  - docs/decisions/0001-kernel-and-work-graph.md
  - docs/decisions/0006-trajectory-evals.md
validated_by:
  - crates/**/tests/**
  - apps/optimus-cli/tests/**
  - apps/optimus-desktop/e2e/**
last_verified_commit: null
---

# Observability, replay, and evaluation map

## Structured runtime evidence

| Surface | State | What is queryable |
|---|---|---|
| Work Graph event ledger | Confirmed current behaviour | Ordered integer sequence, optional job/node IDs, event kind, JSON payload, database timestamp. |
| Job/node projections | Confirmed current behaviour | Status, budgets, effect JSON, durable attempts/receipts, cancellation requests, exact-action approval decisions, steps, and bounded command output. |
| Kernel turn sink | Confirmed current behaviour | In-process text delta, tool started/finished, status, final, and error events. |
| Sessions | Confirmed current behaviour | Serialized transcript, loaded packs, and normalized tool-call → job/node/effect-attempt/effect/receipt-hash links. |
| Campaign/cron/gateway | Confirmed current behaviour | Campaign and cron leases; gateway message claims, generation/token/deadline state, attempt history, terminal outbox JSON, and reconciled files. |
| Memory/skills | Confirmed current behaviour | Dedicated event/evidence tables and outcome records. |
| Desktop logs | Confirmed current behaviour | Process stderr and browser console messages; not durable operational truth. |

**Partially implemented behaviour:** event order is strong inside a single Work
Graph database, but subsystem events do not share a trace, transaction, clock,
or total order.

**Confirmed current behaviour:** accepted Work Graph projection transitions and
their events commit atomically. Storage also reserves one terminal-event slot;
legacy partial projections are quarantined rather than executed.

## Current event/terminal coverage

**Confirmed current behaviour:** tests cover job creation, ordered node
execution, crash ambiguity, exact approval waits/resume/expiry/revocation,
cancellation and Job Object active-process-zero proof, cooperative model
cancellation, command output capture, campaign/cron/gateway lease fencing,
gateway crash reconciliation, session effect causality, timeouts, queue overload,
and stream cancellation on backpressure.

**Unknown or unresolved behaviour:** no global terminal-outcome record combines
workflow, agent, tool, model, artifact, approval, cost, and error data. Work Graph
job terminal uniqueness is established, but no universal cross-subsystem outcome
envelope exists.

**Unknown or unresolved behaviour:** HTTP/native stream delivery failure does not
propagate cancellation into the running turn. Event loss and execution lifetime
are therefore decoupled.

## Replay

**Confirmed current behaviour:** persisted effect JSON plus ordered job events
support inspection and bounded resume. `ScriptedModel` lets tests replay a fixed
model trajectory.

**Unknown or unresolved behaviour:** Optimus does not retain a complete replay
envelope containing workflow/agent/prompt/tool/model versions, model parameters,
input hashes, provider responses, artifact hashes, external
fixtures, timing, and cost. The current system must not claim exact replay.

**Confirmed current behaviour:** exact approval actors/times/effect hashes and
effect-attempt intent/outcome receipts are retained, and interrupted commands are
classified ambiguous rather than replayed. These records improve diagnosis but
do not make arbitrary external effects exactly replayable.

Suggested future stage labels are **planned behaviour**:
`deterministic`, `replayable_with_fixture`, `externally_non_deterministic`,
`model_non_deterministic`, and `destructive`.

## Evaluation coverage

**Confirmed current behaviour:** the built-in offline trajectory suite has four
cases: echo, memory recall, pack activation, and durable file writing. It checks
expected canonical tool IDs and an assistant-text substring.

**Confirmed current behaviour:** Rust unit/integration suites cover state
machines, policies, budgets, filesystem and browser boundaries, provider
parsing, sessions, memory, skills, cron, gateway, and campaigns. Desktop
Playwright tests cover bootstrap, shell/composer behavior, session/runtime
interactions, capabilities/tools, drag, and browser UI contracts.

**Partially implemented behaviour:** these are tests plus a small trajectory
harness, not a general evaluation framework. Results do not currently record
model/prompt/workflow/tool versions, quality/cost/latency dimensions, or baseline
comparisons.

## Missing observability

The following are **unknown or unresolved behaviour**:

- cross-subsystem trace, workflow-run, agent-invocation, tool-call, model-call,
  artifact, approval-request, and memory-write IDs;
- model tokens, cost, latency, retries, fallback reasons, and cache telemetry;
- retrieval candidate/rank evidence;
- security-denial and policy-decision records across all boundaries;
- artifact lineage and source provenance from input to publish;
- CPU/GPU utilization and fallback reason;
- OpenTelemetry export, retention, sampling, and redaction policy;
- production dashboards, SLOs, alerts, and incident correlation;
- reconciliation for failed turns that never reached a durable tool result;
- external-channel delivery acknowledgements, dead-letter attempts, and
  duplicate-delivery records beyond the local transactional outbox.

## Missing evaluation dimensions

| Dimension | Current state |
|---|---|
| Agent routing and ownership | Missing; no specialist-agent system. |
| Workflow completion/retry/cancel | Partial job/campaign tests; no general workflow evals. |
| Tool correctness/security | Strong focused tests for implemented subset; no canonical output-schema conformance. |
| Retrieval precision/recall | Missing. |
| Memory temporal/trust correctness | Unit/integration coverage; no benchmark metrics. |
| Source grounding/citations | Memory citation structure tested; end-to-end factual grounding eval missing. |
| Browser reliability | HTTP effector and UI tests exist; no real-browser CDP reliability benchmark. |
| Cost/latency | Missing. |
| Deterministic replay | Missing beyond scripted trajectories. |
| GPU versus CPU | Not applicable until a GPU adapter exists. |

## Planned evaluation gate

**Planned behaviour:** a prompt, workflow, tool, model-routing, retrieval, or
memory change becomes default only after a versioned baseline comparison. The
result must separate quality, reliability, cost, latency, security, and human
correction rather than collapsing them into one score.
