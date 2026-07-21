---
knowledge_type: observability-evaluation-map
status: current
covers:
  - crates/optimus-store/src/**
  - crates/optimus-graph/src/**
  - crates/optimus-runtime/src/**
  - crates/optimus-kernel/src/eval.rs
  - crates/optimus-kernel/src/execution.rs
  - crates/optimus-kernel/src/agent.rs
  - crates/optimus-kernel/src/workflow.rs
  - crates/optimus-kernel/src/lib.rs
  - apps/optimus-desktop/src/server.rs
  - apps/optimus-desktop/src/native_workers.rs
  - apps/optimus-desktop/src/ipc/chat.rs
  - apps/optimus-desktop/src/bridge.rs
  - apps/optimus-desktop/src/main.rs
  - apps/optimus-desktop/ui/app.js
  - apps/optimus-desktop/e2e/**
depends_on:
  - docs/decisions/0001-kernel-and-work-graph.md
  - docs/decisions/0006-trajectory-evals.md
validated_by:
  - crates/**/tests/**
  - apps/optimus-cli/tests/**
  - apps/optimus-desktop/e2e/**
last_verified_commit: 09fddbc1b60a6b37f9f80680988ea5036a9b8eec
---

# Observability, replay, and evaluation map

## Structured runtime evidence

| Surface | State | What is queryable |
|---|---|---|
| Work Graph event ledger | Confirmed current behaviour | Ordered integer sequence, optional job/node IDs, event kind, JSON payload, database timestamp. |
| Job/node projections | Confirmed current behaviour | Status, budgets, effect JSON, durable attempts/receipts, cancellation requests, exact-action approval decisions, steps, and bounded command output. |
| Kernel turn sink | Confirmed current behaviour | In-process text delta, tool started/finished, status, final, and error events. |
| Sessions | Confirmed current behaviour | Serialized transcript, loaded packs, and normalized tool-call → job/node/effect-attempt/effect/receipt-hash links. |
| Execution manifests | Confirmed current behaviour | Versioned manifest identity, turn, provider/model, prompt/tool/policy/input hashes, atomic root trace link, terminal status, exact tool outcomes, replay classification, and non-replayability reasons. |
| Agent invocations | Confirmed current behaviour | Immutable descriptor version, accepted request, retry lineage, cancellation request, ordered events, one terminal result, and runtime-validated effect links. |
| Workflow registry/adapters | Confirmed current behaviour | Versioned validated definitions, exact terminal declarations, owner capability matrices, and fail-closed status mappings; not universal run state. |
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

**Partially implemented behaviour:** agent invocations and Work Graph jobs each
enforce one terminal result; sessions and execution manifests retain exact
causal/tool outcomes. No global record atomically combines workflow, agent,
tool, model, artifact, approval, cost, and error data across stores.

**Confirmed current behaviour:** desktop HTTP/native stream delivery failure
propagates through the kernel's cooperative token. HTTP full/disconnected
bounded channels and native event-loop closure stop further callback delivery;
the accepted session turn and execution manifest settle as cancelled. Terminal
transport notification after settlement remains best-effort. Explicit desktop
Stop is one-shot and local to the active composer stream: HTTP aborts its own
fetch, and native mode signals an exact active request ID from a bounded registry.

## Replay

**Confirmed current behaviour:** persisted effect JSON plus ordered job events
support inspection and bounded resume. `ScriptedModel` lets tests replay a fixed
model trajectory.

**Confirmed current behaviour:** versioned execution manifests retain provider,
model, prompt/tool/policy/input hashes, ordered canonical tool outcomes, and
explicit replay classification. Model calls remain honestly non-replayable;
deterministic/convergent tools may be fixture-replayable. Optimus does not retain
all provider responses and must not claim exact replay for arbitrary runs.

**Confirmed current behaviour:** immutable bounded replay bundles retain
content-addressed fixture bytes, source manifest/trace/dependency hashes,
ordered stages, and expected terminal evidence. Planning validates completeness
and drift. A zero-effect executor compares exact inputs/fixtures and appends one
terminal report; it never reruns a provider or external effect.

**Confirmed current behaviour:** canonical local trace/span stores retain parent
relationships, ordered bounded events, one terminal span outcome, traced route
decisions, and immutable execution-manifest links. No distributed transaction
or external exporter is claimed.

**Confirmed current behaviour:** every newly recorded production kernel turn has
one parentless execution trace link committed atomically with its manifest.
Successful turn results expose it, and interrupted-turn resume preserves it
exactly after fail-closed manifest/link preflight. This causal link does not by
itself assert a `TraceStore` span lifecycle or child-span propagation.

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
expected canonical tool IDs and an assistant-text substring. The exact public
runner additionally retains exact assistant output, canonical invoked-tool IDs,
terminal manifest status, persisted replay classification, and the root execution
trace. Failed cases expose no typed success evidence.

**Confirmed current behaviour:** the integrity evaluation requires exactly six
observed, evidence-bearing cases: sensitivity denial, SmartDeny approval,
route-policy denial, cooperative cancellation, stale-completion fencing, and
gateway dead letter. Missing, duplicate, or evidence-free observations fail the
evaluation contract. The public offline executor runs those real local subsystem
boundaries in an isolated per-run directory, matches policy-specific denial
outcomes, and returns six deterministic failed observations when run ownership
cannot be established. A usable run persists one parentless evaluation root per
case with hashed evidence and terminal settlement, then returns the read-back
context, terminal status, and deterministic replay class. Retry trace identities
are fresh while normalized case semantics remain stable.

**Confirmed current behaviour:** the versioned Priority-2 dataset represents
those six cases and four trajectories with exact case/tool/terminal/replay/trace
contracts and provenance. Deterministic reports bind dataset, source tree,
contract, tool catalog, route policy, provider, and model identities; checked
integer metrics, explicit thresholds, immutable baselines, and regression
comparisons fail closed on incompatible evidence. A baseline and candidate may
bind different source trees, but report hashes, dataset identity, non-source
binding context, threshold policy, and metric keys must remain exact. Construction,
baseline acceptance/loading, and comparison reject rehashed evidence with invalid
bindings, incomplete metrics, inconsistent arithmetic, duplicate thresholds, or
incorrect failure/pass projection before mutation or comparison.
Observation input must explicitly declare trace presence; report construction
rejects a missing trace whenever the identity-matched case requires one. Trace
presence is evidence validity rather than a scored metric.

**Confirmed current behaviour:** the exact Priority-2 offline runner creates a
fresh evaluation-owned run, executes both exact suites, projects all ten results
in dataset order, and returns one candidate-bound `EvaluationReportV1`. Text,
tool, terminal, replay, and trace fields derive from executor evidence. Latency and
cost must be supplied explicitly for every case; the runner neither fabricates
zero values nor introduces wall-clock nondeterminism. Equal semantic evidence and
resource inputs produce equal report bytes despite fresh run and trace identities.
The CLI exposes this path as `optimus eval report` using separate one-megabyte-
bounded JSON inputs for binding, measurements, and optional thresholds. Typed
caller contracts are preflighted before run mutation; a threshold-failing report
is printed before the command returns non-zero.
`python scripts/engineering_memory.py binding` derives the required binding from a
fresh canonical source traversal. The kernel independently derives its compiled
evaluation/tool/routing hashes and rejects provider, model, or context mismatch
before evaluation run ownership.
`optimus eval compare --baseline BASELINE --candidate CANDIDATE` exposes the
canonical comparator through independently one-megabyte-bounded report inputs.
It dispatches before home initialization, writes no state, and prints one complete
comparison even when metrics regress; invalid or incompatible evidence prints no
comparison JSON and fails.

**Confirmed current behaviour:** Rust unit/integration suites cover state
machines, policies, budgets, filesystem and browser boundaries, provider
parsing, sessions, memory, skills, cron, gateway, and campaigns. Desktop
Playwright tests cover bootstrap, shell/composer behavior, session/runtime
interactions, capabilities/tools, drag, and browser UI contracts.

**Partially implemented behaviour:** the exact ten-case report producer is not a
universal workflow runner or automatic release gate and does not establish factual
correctness beyond declared cases. Explicit resource measurements are
identity-checked but their external source is not independently proven.

## Missing observability

The following are **unknown or unresolved behaviour**:

- universal transactionally-coupled workflow-run/model-call/artifact correlation;
  every production kernel execution has a root link and selected routes are
  traced, while child spans and other owner IDs remain incomplete;
- model tokens, cache telemetry, and live billing integration;
- retrieval candidate/rank evidence;
- security-denial and policy-decision records across all boundaries;
- artifact lineage and source provenance from input to publish;
- CPU/GPU utilization and fallback reason;
- OpenTelemetry export, retention, sampling, and redaction policy;
- production dashboards, SLOs, alerts, and incident correlation;
- reconciliation for failed turns that never reached a durable tool result;
- delivery acknowledgements/dead-letter attempts exist for the local gateway;
  external-channel broker guarantees remain unresolved.

## Missing evaluation dimensions

| Dimension | Current state |
|---|---|
| Agent routing and ownership | Contract/invocation/effect-link tests exist; no built-in specialists or router. |
| Workflow completion/retry/cancel | General schema/adapter conformance plus cross-contract tests; no universal executor. |
| Tool correctness/security | Strong focused tests for implemented subset; no canonical output-schema conformance. |
| Retrieval precision/recall | Missing. |
| Memory temporal/trust correctness | Unit/integration coverage; no benchmark metrics. |
| Source grounding/citations | Memory citation structure tested; end-to-end factual grounding eval missing. |
| Browser reliability | HTTP effector and UI tests exist; no real-browser CDP reliability benchmark. |
| Cost/latency | Synthetic provenance-bound route observations and evaluation means exist; live billing/token integration is missing. |
| Deterministic replay | Bounded immutable fixture comparison exists; live providers/external effects are deliberately not rerun. |
| GPU versus CPU | Not applicable until a GPU adapter exists. |

## Planned evaluation gate

**Planned behaviour:** wire the candidate-aware versioned baseline contract into
default-change delivery for prompts, workflows, tools, model routing, retrieval,
and memory. The
result must separate quality, reliability, cost, latency, security, and human
correction rather than collapsing them into one score.
