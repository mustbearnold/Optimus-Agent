---
knowledge_type: architecture
status: current
covers:
  - Cargo.toml
  - apps/optimus-cli/src/**
  - apps/optimus-desktop/src/**
  - crates/optimus-store/src/**
  - crates/optimus-graph/src/**
  - crates/optimus-runtime/src/**
  - crates/optimus-memory/src/**
  - crates/optimus-skills/src/**
  - crates/optimus-packs/src/**
  - crates/optimus-kernel/src/**
depends_on:
  - docs/decisions/**
validated_by:
  - crates/**/tests/**
  - apps/optimus-cli/tests/**
  - apps/optimus-desktop/e2e/**
last_verified_commit: 09fddbc1b60a6b37f9f80680988ea5036a9b8eec
---

# Optimus Agent system overview

This document describes the repository as it exists now. The historical
blueprint in `optimus-exceeds-hermes.md` remains useful product direction, but
it contains planned components and is not proof of implementation.

## Status legend

- **Confirmed current behaviour** — observed in source or tests.
- **Inferred behaviour** — a bounded architectural interpretation.
- **Planned behaviour** — a target without a complete implementation.
- **Unknown or unresolved behaviour** — evidence or a settled contract is
  missing.

## Current topology

**Confirmed current behaviour**

```text
CLI or Desktop (native Wry IPC; loopback HTTP only for dev/tests)
                         |
                         v
          provider selection at each surface
                         |
                         v
       optimus-kernel::Kernel turn/tool loop
          |          |          |          |
          v          v          v          v
       packs       memory      skills    sessions + effect links
          |                                  |
          v                                  v
 canonical ToolDesc                    SQLite transcript
          |
          v
 optimus-runtime durable jobs ----> optimus-graph state machine
          |                                  |
          +--------------------------> optimus-store SQLite ledger
```

**Inferred behaviour:** the kernel is the current control-plane waist because
both implemented user surfaces construct it and it assembles providers, packs,
memory, skills, sessions, and durable effects. There is no separately
implemented `optimus-control-plane` or `optimus-orchestrator` package.

## Applications and packages

| Component | State | Current responsibility |
|---|---|---|
| `apps/optimus-cli` | Confirmed current behaviour | CLI for jobs, approvals, skills, packs, chat, sessions, auth, cron, browser, gateway, evals, and campaigns. It also hosts a loopback webhook gateway. |
| `apps/optimus-desktop` | Confirmed current behaviour | Windows-first Wry/Tao desktop shell, native IPC, bounded worker queues, inline HTML/JS/CSS UI, and loopback HTTP test mode. |
| `crates/optimus-kernel` | Confirmed current behaviour | Provider-agnostic turn loop, strict tool dispatch, typed agent/workflow contracts and registries, durable agent invocation ledger, sessions, execution manifests, credential protection, canonical routing, cron, gateway, browser/search effectors, filesystem sandbox, and offline eval harnesses. |
| `crates/optimus-packs` | Confirmed current behaviour | Canonical pack/tool descriptors, provider-visible input schemas, tool policy/invocation identity, availability, validation, and schema-token budgets. |
| `crates/optimus-runtime` | Confirmed current behaviour | Durable ordered jobs, effect intents/receipts, bounded command execution, exact-action SmartDeny approvals, cancellation, crash recovery, output capture, and leased ordered campaigns. |
| `crates/optimus-graph` | Confirmed current behaviour | Job/node/effect domain and state-transition helpers. |
| `crates/optimus-store` | Confirmed current behaviour | Versioned SQLite jobs, nodes, exact-action approval decisions, cancellation requests, effect attempts, atomic transitions, quarantine state, and ordered append-only events. |
| `crates/optimus-memory` | Confirmed current behaviour | SQLite evidence-native claim ledger, bitemporal correction, scoped recall, conflict sets, injected monotonic clock, sensitivity/allowed-use gates, retention, tombstone/privacy erase, sanitized audit events, and evidence packets. |
| `crates/optimus-skills` | Confirmed current behaviour | SQLite versioned procedural-skill registry with closed permissions, outcome counts, promotion, pinning, and deprecation. |

## Control plane and orchestration

**Confirmed current behaviour:** the CLI and desktop directly select a model
provider, create or resume a `Kernel`, and call `Kernel::turn` or
`turn_with_sink` or the cancellable variant. Desktop IPC methods are frozen in a single method/domain
registry and dispatched to small domain modules.

**Confirmed current behaviour:** `CampaignStore` runs an ordered list of
`WriteFile` or `RunCommand` steps. Each campaign step becomes a Work Graph job,
so crash recovery and SmartDeny apply. Exact live owner/token/generation leases
fence concurrent runners. Campaigns can end `succeeded`, `failed`, `cancelled`,
or `awaiting_approval`.

**Confirmed current behaviour:** versioned agent and workflow contract
substrates and immutable registries exist in `optimus-kernel`. They are library
boundaries, not a new control-plane process. No built-in specialist definition
is registered, and campaign steps remain deterministic effect specifications,
not specialist-agent invocations.

**Planned behaviour:** a dedicated control plane, specialist routing, parallel
child hierarchy, and general workflow executor remain targets. The implemented
schemas, invocation ledger, and adapters must not be described as those
executors.

## Workflow runtime and terminal outcomes

**Confirmed current behaviour:** jobs contain one or more ordered nodes and have
step/failure/time budgets. A running node recovered after process death becomes
`interrupted`, never silently `succeeded`. `run_all` stops on success, failure,
approval wait, or a non-runnable/error condition. Commands are killed and reaped
at a bounded timeout, and stdout/stderr capture is capped. On Windows commands
launch suspended, enter a private kill-on-close Job Object before resume, and
settlement verifies the Job has zero active processes.

**Confirmed current behaviour:** jobs, nodes, campaign steps, and campaigns have
typed `cancelled` outcomes. Cancellation requests are durable and idempotent;
pending work terminalizes atomically, running commands observe the request and
terminate/reap their Windows process tree before cancellation is finalized, and
campaign cancellation propagates to created jobs and uncreated steps. A storage
unique terminal slot enforces exactly one terminal event across repeated cancel,
resume, run, recovery, and recomputation.

**Confirmed current behaviour:** the kernel has a typed cooperative cancellation
token and cancellable turn/provider seam. A provider can observe cancellation
during an active call; Codex SSE checks at bounded read intervals after the
response stream opens. A cancellation-aware sink is additive to the legacy sink
API. Desktop native event-loop closure and HTTP event-channel full/disconnected
results request cancellation through that seam; later stream callbacks are
suppressed and existing session/execution terminal stores settle cancellation.
Explicit desktop Stop uses a one-shot bridge handle. HTTP cancellation aborts
only the owning fetch; native mode uses a bounded process-local exact-ID token
registry registered before queue admission and removed after terminal settlement.

**Confirmed current behaviour:** agent invocations have durable cancellation
requests, cooperative token synchronization, retry lineage with new identities,
and one storage-enforced terminal result. General workflow definitions require
explicit cancellation/retry/timeout/approval/rollback and exact terminal
declarations; owner adapters state unsupported capabilities rather than
inventing them.

**Unknown or unresolved behaviour:** synchronous `ureq` connection/write cannot
be force-aborted, and cancellation has no general future child-agent hierarchy.
Native cancellation acknowledgement means the cooperative token was signalled;
the persisted stream terminal outcome remains authoritative in a completion race.

**Unknown or unresolved behaviour:** workflow retry policies are declarations,
not a universal retry scheduler. Work Graph interruption recovery, subsystem
leases, Codex adapter retry, and agent retry lineage retain owner-specific
semantics.

## Agent execution

**Confirmed current behaviour:** `ModelProvider` is a synchronous provider
adapter interface. `ScriptedModel` is a deterministic test/offline adapter.
`Kernel::turn_with_sink` and `turn_with_controlled_sink` loop over model responses
and canonical tools until a non-empty final assistant response, cancellation, or
a bounded error.

**Confirmed current behaviour:** canonical agent IDs/versions, descriptors,
typed bounded requests/results, context/evidence references, budgets, tool sets,
permission envelopes, immutable registration, durable invocation events,
cancellation, retry lineage, and exact runtime-effect provenance links are
implemented. Descriptor and request validation use canonical available
`ToolId`s and exact permission ceilings.

**Unknown or unresolved behaviour:** there are no built-in specialist-agent
definitions, specialist router, parallel scheduler, or child hierarchy. The
agent contract does not bypass runtime SmartDeny or filesystem confinement.

## Tool system

**Confirmed current behaviour:** `optimus-packs::ToolDesc` is the canonical
implemented tool contract. It owns stable ID, description, provider input and
output schemas, policy and invocation identity, replay class, retry,
idempotency, timeout ownership, cancellation, observability declarations,
availability, pack ownership, and schema-token cost. Available tool calls are validated against the exact set
advertised for that model step, including non-empty unique call IDs, before any
sibling effect runs.

**Confirmed current behaviour:** available tools are `read_file`, `write_file`,
`terminal`, `web_search`, `memory_recall`, `skill_resolve`, `activate_pack`,
`browser_navigate`, `browser_click`, and `browser_snapshot`. Other catalog items
are explicit unavailable placeholders and are not advertised to models.

**Confirmed current behaviour:** `write_file` and `terminal` route through
durable jobs. `terminal` pauses under SmartDeny until a separate grant bound to its exact
job/node/SHA-256 effect identity. Browser tools use an HTTP text/link effector,
not CDP. `read_file`
uses the filesystem sandbox and denies secret basenames.

**Unknown or unresolved behaviour:** owner-specific runtime paths do not yet
implement universal cooperative cancellation or retries merely because the
descriptor declares their support boundary.

## State and persistence

**Confirmed current behaviour:** state is split across these stores under an
Optimus home:

| Store | Owner | Contents |
|---|---|---|
| `optimus.db` | store/runtime | Jobs, nodes, approvals, ordered events, campaign plans, schema metadata, and campaign projection caches. |
| `memory.db` | memory | Evidence ledger and bitemporal claims. |
| `skills.db` | skills | Skill versions, permissions, outcomes, and events. |
| `sessions.db` | kernel/session | Session title, loaded pack names, serialized messages, and hash-only durable effect causal links. |
| `cron.db` | kernel/cron | Interval schedules, exact lease owner/generation/token/deadline state, and latest status. |
| `gateway/gateway.db` plus files | kernel/gateway | Authoritative message claims/attempts/terminal outbox JSON plus reconciled inbox/outbox/processed/failed adapter files. |
| caller-selected agent registry/invocation DBs | kernel/agent | Immutable descriptor versions plus accepted invocation projections, ordered events, retry lineage, cancellation, terminal results, and validated effect links. |
| caller-selected workflow registry DB | kernel/workflow | Immutable validated workflow definitions; not workflow execution state. |
| caller-selected replay DB | kernel/replay | Immutable content-addressed fixture bundles and one terminal replay report per bundle. |
| caller-selected trace DB | kernel/trace | Canonical spans, ordered events, and one terminal span outcome. |
| `routing.db` telemetry tables | kernel/telemetry | Provenance-bound provider/model outcome, latency, and cost observations. |
| caller-selected evaluation DB | kernel/evaluation | Immutable candidate-bound baseline reports. |
| `workspace/.optimus/browser_state.json` | kernel/browser | Last HTTP page and bounded navigation history. |

**Unknown or unresolved behaviour:** the remaining stores do not share a
transaction, migration framework, backup policy, or universal retention
contract. Campaigns and Work Graph jobs are the exception: both live in
`optimus.db`. Cross-contract agent/session links reconcile committed identities;
they are not distributed transactions.

**Confirmed current behaviour:** complete Work Graph job creation and later
projection/event transitions are transactional. Legacy partial state is
diagnosed and quarantined before execution. Terminal-event uniqueness is
enforced in storage. Effect intent is durable before I/O; `WriteFile` uses
temporary replacement and receipts, while a crashed command attempt is marked
ambiguous and cannot be blindly replayed.

**Confirmed current behaviour:** cron due work is claimed transactionally and
stale owners cannot complete after expiry, takeover, disable, or release.
Gateway message UUIDs are ingested idempotently; exact leased attempts own one
terminal outcome and deterministic outbox/archive materialization is reconciled
without rerunning the turn. Durable session tool messages are persisted with an
exact job/node/attempt/effect hash and receipt hash in the same session
transaction before the next model step.

## Memory and retrieval

**Confirmed current behaviour:** runtime MetaMemory is separate from sessions,
skills, gateway state, and this Engineering Memory. Claim writes derive trust
from origin and cap it by the authenticated write context. Recall is scoped by
tenant/user/project before selection, supports valid-time and transaction-time
views, detects conflicting objects, returns citations, and rejects
`ActionAuthorize`.

**Confirmed current behaviour:** current recall is deterministic SQLite
filtering and ordering by scope, optional exact subject/predicate, and temporal
fields. There is no embedding, vector search, reranker, knowledge graph, or GPU
retrieval implementation in the workspace.

**Confirmed current behaviour:** memory default transaction and event times use
an injected UTC monotonic clock. Sensitivity and allowed-use filters apply before
recall limiting; correction preserves sensitivity; retention, tombstone, and
privacy erase are idempotent scoped transitions with sanitized audit records.

## Model routing

**Confirmed current behaviour:** a canonical typed route resolver validates
provider/model ownership, required capabilities, local-only privacy, cost
budget, and explicitly bounded fallback before optional fresh telemetry
filtering/ranking, then persists route decisions. Telemetry is tied to exact
route provider/model/trace identity and cannot authorize a statically denied
candidate. CLI,
desktop, cron, and gateway use the same resolver. Provider-specific wire parsing
remains in the adapters.

**Confirmed current behaviour:** Codex retries once after an HTTP failure with
system plus last-user messages and no reasoning effort. This provider-local
retry is distinct from cross-provider fallback.

**Known routing debt:** normalized reasoning effort and fast mode are sent by the
Codex adapter; the OpenAI-compatible request mapper does not transmit them.

**Unknown or unresolved behaviour:** token accounting, live billing integration,
automatic runtime-failure fallback, evaluation-report-driven selection, and
local-model/GPU adapters are not implemented.

## Security and approvals

**Confirmed current behaviour:** SmartDeny is the default runtime policy and
`RunCommand` is the only current Work Graph effect classified high-risk.
Approval decisions are durable and bound to exact job, node, and SHA-256 effect
identity, with actor, creation time, expiry, denial, and revocation metadata.
They do not transfer to changed effects or later nodes. Skills cannot expand
their declared permissions; a skill can grant a terminal action approval only
if it declares `Terminal`.

**Confirmed current behaviour:** `FsRoots` reads are rooted, canonicalized,
symlink/prefix checked, and secret-name denied. Runtime `WriteFile` and
`AssertFileEquals` accept only normal path components and resolve through a
retained `cap-std` workspace directory capability. Root replacement and linked
Windows junction/Unix symlink ancestors cannot redirect built-in effects. Kernel
and runtime share one case-insensitive secret-basename policy. Browser HTTP
navigation rejects non-HTTP(S),
loopback, private, link-local, local, and metadata targets before and after
redirects.

**Confirmed current behaviour:** campaign persistence rejects malformed scalar or
step JSON fields and validates a migrated expected step count plus contiguous
indices before any runtime effect. Missing or partially reassigned steps cannot
silently shorten the executable plan. Campaign schema v4 has transactional
migrations, future-version rejection, read-only legacy import, diagnostics, and
deterministic projection repair plus fenced owner leases. Campaign status is
derived from Work Graph jobs in the same SQLite database.

**Confirmed current behaviour:** native desktop uses Wry IPC on a custom origin.
HTTP mode and the webhook gateway bind to `127.0.0.1`.

**Confirmed current behaviour:** desktop HTTP mode is explicitly development-only
and requires a 32-character bearer token. Effectful POSTs additionally require
an exact loopback origin and CSRF header; wildcard CORS is disabled. The gateway
requires its own 32-character bearer token and validates any supplied browser
origin. Both surfaces cap request bodies, apply fixed-window request limits,
bound aggregate operations, omit home paths from health responses, and return
stable redacted errors while retaining local stderr diagnostics.

**Known security boundary:** `WriteFile` is not currently high-risk under
SmartDeny, and an approved arbitrary command is not filesystem-sandboxed by the
built-in file-effect capability.

## Events, observability, and replay

**Confirmed current behaviour:** Work Graph events have an ordered SQLite
sequence and optional job/node IDs. Model turns expose in-process text, tool,
and status stream events. Sessions, cron, campaigns, gateway, skills, and memory
also retain subsystem-specific state.

**Confirmed current behaviour:** versioned execution manifests and immutable,
bounded, content-addressed fixture bundles support zero-effect offline replay.
Planning binds exact source manifest, trace, policy, tool catalog, stage order,
fixture hashes, and terminal evidence. Input or fixture drift fails before later
stages and one immutable replay report records the terminal comparison.

**Confirmed current behaviour:** canonical trace/span identities support ordered
append-only events, one terminal span outcome, traced route decisions, and
immutable execution-manifest trace links. Versioned evaluation datasets retain
ten declared cases and produce deterministic candidate-bound metrics, thresholds,
reports, immutable baselines, and regression comparisons.

**Confirmed bounded behaviour:** desktop stream delivery loss requests the same
cooperative token used by active providers and tool-loop boundaries. A turn can
still commit a durable effect and then fail before the session transcript is
saved.

**Unknown or unresolved behaviour:** there is no OpenTelemetry export,
structured security-denial stream, token accounting, artifact publication
lineage, GPU/fallback telemetry, or transaction spanning trace, route,
execution, runtime, agent, workflow, and session stores. Fixture replay does not
rerun live providers or external effects. Logs are not a complete source of
operational truth.

## GPU and CPU fallback

**Confirmed current behaviour:** no CUDA, GPU crate, embedding backend, vector
index, reranker, or local-model runtime is implemented. Core functionality is
CPU-only and does not require CUDA.

**Planned behaviour:** GPU adapters may accelerate embedding similarity,
batching, reranking, and local utility inference when benchmarks justify them.
Each adapter must remain replaceable and have correctness-tested CPU fallback.
RTX 5070 12 GB is a development constraint, not permission to make GPU
availability mandatory.

## Current architectural debt and open decisions

1. **Confirmed contract, unresolved product:** no built-in specialists or router.
2. **Confirmed contract, unresolved product:** no universal workflow executor.
3. **Partially implemented:** cancellation remains owner-specific.
4. **Confirmed contract, unresolved product:** metadata declarations do not create universal runtime cancellation/retry.
5. **Partially implemented:** policy and telemetry routing exist; evaluation-driven routing does not.
6. **Confirmed bounded behaviour:** fixture replay and local causal traces exist; live-effect replay and distributed tracing do not.
7. **Unknown/unresolved:** provenance and artifact publishing contracts.
8. **Known boundary:** approved arbitrary child processes are not governed by
   the built-in file-effect directory capability.
9. **Confirmed bounded behaviour:** Work Graph terminal uniqueness and campaign,
   cron, and gateway owner/generation/token/deadline fencing are implemented;
   external exactly-once delivery remains unresolved.
10. **Known debt:** durable effects and session transcript persistence can diverge.
11. **Known debt:** duplicate ADR number `0016`.
12. **Known debt:** existing blueprint and phase notes mix future targets with
    historical/current claims; they require gradual labeling, not deletion.
