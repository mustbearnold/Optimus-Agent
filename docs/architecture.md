---
doc_id: architecture-system-overview
doc_type: explanation
plane: current
status: current
authority: canonical
summary: This document describes the repository as it exists now. The historical blueprint in optimus-exceeds-hermes.md remains useful product direction, but it contains planned components and is not proof of implementation.
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: architecture
owns:
  - Cargo.toml
  - crates/optimus-kernel/src/lib.rs
  - crates/optimus-runtime/src/lib.rs
  - crates/optimus-packs/src/lib.rs
  - crates/optimus-memory/src/lib.rs
  - crates/optimus-store/src/lib.rs
  - crates/optimus-graph/src/lib.rs
  - crates/optimus-skills/src/lib.rs
  - apps/optimus-cli/src/main.rs
  - apps/optimus-desktop/src/main.rs
  - specs/004-runtime-effects/spec.md
watches:
  - apps/optimus-cli/src/**
  - apps/optimus-desktop/src/**
  - crates/*/src/**
covers:
  - Cargo.toml
  - crates/optimus-kernel/src/lib.rs
  - crates/optimus-runtime/src/lib.rs
  - crates/optimus-packs/src/lib.rs
  - crates/optimus-memory/src/lib.rs
  - crates/optimus-store/src/lib.rs
  - crates/optimus-graph/src/lib.rs
  - crates/optimus-skills/src/lib.rs
  - apps/optimus-cli/src/main.rs
  - apps/optimus-desktop/src/main.rs
depends_on:
  - docs/decisions/0017-engineering-memory-separation.md
  - docs/decisions/0020-work-graph-integrity-and-loopback-security.md
  - docs/decisions/0026-separate-development-and-runtime-agents.md
  - docs/decisions/0031-safe-project-work-loop.md
  - docs/decisions/0032-engineering-memory-compact-lenses.md
validated_by:
  - crates/optimus-kernel/tests/kernel_turn.rs
  - crates/optimus-runtime/tests/cancellation.rs
  - apps/optimus-cli/tests/gateway_http.rs
  - apps/optimus-desktop/e2e/03-runtime-and-sessions.spec.js
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

## Instruction planes

**Confirmed current behaviour:** Optimus has two deliberately separate root
instruction surfaces:

| Surface | Audience | Runtime loading |
|---|---|---|
| `AGENTS.md` | Humans and coding agents developing Optimus | Never injected into product chat |
| `OPTIMUS_AGENTS.md` | Installed Optimus product sessions | Embedded by `optimus-kernel` |

Development requests about autonomy, orchestration, model/reasoning selection,
VCS, testing, or reporting remain in the development plane. They do not alter
product prompts, permission defaults, routing, or approval behaviour unless the
user explicitly requests a product/runtime change.

`crates/optimus-kernel/src/system_prompt.rs` constructs the product system
message from `OPTIMUS_AGENTS.md` and has regression coverage excluding the
development-only body. ADR-0026 owns this boundary. A selected third-party
project may contribute task-local project instructions; those remain distinct
from both Optimus root surfaces.

## Current topology

**Confirmed current behaviour**

```text
CLI, legacy Wry Desktop, or Tauri React workbench
        (bounded preload -> authenticated loopback Rust host)
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
| `apps/optimus-desktop` | Confirmed current behaviour | Rust host (`--host-only`) for the Tauri shell + legacy Wry/Tao shell (WebKitGTK / WebView2), frozen IPC registry, bounded worker queues, inline legacy UI, and loopback HTTP. |
| `apps/optimus-tauri` | Confirmed current behaviour | **Default installed** React shell; Tauri commands bridge the frozen IPC registry, bounded chat streams with cancellation, window chrome, and native folder selection; embeds the built workbench. |
| `apps/optimus-ui` | Confirmed current behaviour | React 19 workbench; typed `DesktopMethod` transport (Tauri bridge); multi-folder presentation with Rust scope authority; Browser surface drives the kernel `browser_*` effector. |
| `crates/optimus-kernel` | Confirmed current behaviour | Provider-agnostic turn loop, strict tool dispatch, sessions, execution manifests, credential protection, canonical routing, browser/search effectors, and filesystem sandbox. Re-exports agent/workflow/artifacts/ops for surfaces. |
| `crates/optimus-agent` | Confirmed current behaviour | Versioned specialist descriptors, immutable registry, durable invocation/cancellation/retry/terminal ledger, effect provenance links. |
| `crates/optimus-workflow` | Confirmed current behaviour | Workflow definitions/registry, durable DAG `WorkflowRunStore`, built-in specialist verticals and registered-definition executor. |
| `crates/optimus-artifacts` | Confirmed current behaviour | Content-addressed handoff/workbench artifact store under `{home}/artifacts`. |
| `crates/optimus-ops` | Confirmed current behaviour | Operator services: durable local gateway delivery authority and cron schedule store. Kernel re-exports for surface convenience; does not own the turn loop. |
| `crates/optimus-eval` | Confirmed current behaviour | Offline integrity/trajectory harnesses, versioned evaluation reports/baselines, and zero-effect fixture replay. Depends on kernel; kernel does not depend on eval. |
| `crates/optimus-browser` | Confirmed current behaviour | Optional CDP browser backend for agent tools and the workbench Browser surface. |
| `crates/optimus-packs` | Confirmed current behaviour | Canonical pack/tool descriptors, provider-visible input schemas, tool policy/invocation identity, availability, validation, and schema-token budgets. |
| `crates/optimus-runtime` | Confirmed current behaviour | Durable ordered jobs, effect intents/receipts, bounded command execution, exact-action SmartDeny approvals, cancellation, crash recovery, output capture, and leased ordered campaigns. |
| `crates/optimus-graph` | Confirmed current behaviour | Job/node/effect domain and state-transition helpers. |
| `crates/optimus-store` | Confirmed current behaviour | Versioned SQLite jobs, nodes, exact-action approval decisions, cancellation requests, effect attempts, atomic transitions, quarantine state, and ordered append-only events. |
| `crates/optimus-memory` | Confirmed current behaviour | SQLite evidence-native claim ledger, bitemporal correction, scoped recall, non-authorizing FTS5 free-text recall with per-hit standing, conflict sets, injected monotonic clock, sensitivity/allowed-use gates, retention, tombstone/privacy erase, sanitized audit events, and evidence packets. |
| `crates/optimus-skills` | Confirmed current behaviour | SQLite versioned procedural-skill registry with closed permissions, outcome counts, promotion, pinning, and deprecation. |

## Control plane and orchestration

**Confirmed current behaviour:** the CLI and desktop select Auto or an explicit
model provider, resolve Auto once to a concrete provider/model, create or resume
a `Kernel`, and call `Kernel::turn` or
`turn_with_sink` or the cancellable variant. Desktop IPC methods are frozen in a single method/domain
registry and dispatched to small domain modules.

**Confirmed current behaviour:** an `approval_required` chat lifecycle event
carries an exact runtime binding. The React transcript may submit approve or
deny through `chat_approval_resolve`; desktop validates every identity field and
Rust settles the effect, lifecycle receipt, turn, and execution manifest before
the UI reloads the canonical session projection. This settlement does not issue
a second model-provider request.

**Confirmed current behaviour:** `CampaignStore` runs an ordered list of
`WriteFile` or `RunCommand` steps. Each campaign step becomes a Work Graph job,
so crash recovery and SmartDeny apply. Exact live owner/token/generation leases
fence concurrent runners. Campaigns can end `succeeded`, `failed`, `cancelled`,
or `awaiting_approval`.

**Confirmed current behaviour:** versioned agent and workflow contract
substrates and immutable registries exist in `optimus-agent` and
`optimus-workflow` (re-exported by the kernel). They are library boundaries, not
a new control-plane process. Built-in specialists are registered via those peels
(see Agent execution). Campaign steps remain deterministic effect specifications,
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
at a bounded timeout, and stdout/stderr capture is capped. On Linux each command
runs under systemd-run + bwrap with a `CommandFsEnvelope` profile (default:
workspace-only writable FS). Cancellation, timeout, normal root exit, and guard
drop terminate the owned unit/process tree and verify it is empty. On Windows
commands launch suspended, enter a private kill-on-close Job Object before
resume, and settlement verifies the Job has zero active processes.

**Confirmed current behaviour:** jobs, nodes, campaign steps, and campaigns have
typed `cancelled` outcomes. Cancellation requests are durable and idempotent;
pending work terminalizes atomically, running commands observe the request and
terminate and reap their platform-owned process tree before cancellation is
finalized, and
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

**Confirmed current behaviour:** the turn loop emits and durably records monotonic
integer-millisecond timing events for the turn, each model step, first provider
response, and each tool call. Terminal success, failure, and cancellation include
one turn-finished timing event. Successful desktop responses expose total,
first-response, aggregate model, and aggregate executed-tool durations.

**Confirmed current behaviour:** canonical agent IDs/versions, descriptors,
typed bounded requests/results, context/evidence references, budgets, tool sets,
permission envelopes, immutable registration, durable invocation events,
cancellation, retry lineage, and exact runtime-effect provenance links are
implemented. Descriptor and request validation use canonical available
`ToolId`s and exact permission ceilings.

**Confirmed current behaviour:** two built-in specialists are registered via
`open_seeded_agent_registry`:
`workspace_writer@1.0.0` (`write_file`) and `workspace_reader@1.0.0` (`read_file`).
Three immutable workflows are seeded: `write_file_handoff@1.0.0`,
`read_file_handoff@1.0.0`, and the two-node DAG
`write_then_read_handoff@1.0.0`. Execution goes through durable
`WorkflowRunStore` (`workflow-runs.db`): run lease, per-node projections, child
invocation links, and exactly one run terminal. Writers still use Work Graph
`WriteFile` under SmartDeny (optional auto-grant), link effect provenance, and
publish content-addressed handoff artifacts. Readers publish handoff artifacts
without host mutation. Parent run cancel fans out to child invocations/jobs and
blocks new children on terminal parents. CLI: `optimus vertical list`,
`write-file`, `read-file`, `write-then-read`.

**Unknown or unresolved behaviour:** there is no model-chosen specialist router,
parallel multi-ready-node execution, command/shell specialist, or MCP agents.
The agent contract still does not bypass runtime SmartDeny or filesystem
confinement. **Confirmed (P12):** commands use `CommandFsEnvelope` (default
Linux confined workspace-only RW; Windows Job Object residual product-visible;
`UnrestrictedHost` explicit break-glass).

## Tool system

**Confirmed current behaviour:** `optimus-packs::ToolDesc` is the canonical
implemented tool contract. It owns stable ID, description, provider input and
output schemas, policy and invocation identity, replay class, retry,
idempotency, timeout ownership, cancellation, observability declarations,
availability, pack ownership, and schema-token cost. Available tool calls are validated against the exact set
advertised for that model step, including non-empty unique call IDs, before any
sibling effect runs.

**Confirmed current behaviour:** provider responses have a hard ceiling of 64 tool
calls before effects and a default execution budget of eight calls per model step.
Valid overflow calls receive typed suppressed outcomes and force the next request
to advertise no tools. Exact normalized repeated `web_search`, `memory_recall`, and
`skill_resolve` calls are suppressed after their first execution in a turn; mutable
and context-sensitive tools are not semantic-deduplicated.

**Confirmed current behaviour:** available tools are `read_file`, `write_file`,
`terminal`, `web_search`, `memory_recall`, `skill_resolve`, `activate_pack`,
`browser_navigate`, `browser_click`, and `browser_snapshot`. Other catalog items
are explicit unavailable placeholders and are not advertised to models.

**Confirmed current behaviour:** `write_file` and `terminal` route through
durable jobs. `terminal` pauses under SmartDeny until a separate grant bound to its exact
job/node/SHA-256 effect identity. Browser tools use an HTTP text/link effector,
not CDP. `read_file`
uses the filesystem sandbox and denies secret basenames.

## Domain modularity (P13 / ADR-0036)

**Confirmed current behaviour:** domain ownership is single-catalog and
plane-separated (grade **S+++** in architecture-marks):

| Plane | Owner | Must not |
|---|---|---|
| Tool identity | `optimus-packs::ToolDesc` / `ToolId` / `ToolInvocation` | Second catalog in kernel or surfaces |
| Session transcript | `SessionStore` | Authorize host effects |
| Semantic memory | `optimus-memory` | `ActionAuthorize` / live capability grants |
| Procedural skills | `optimus-skills` | Expand closed permissions; grant wrong effect class |
| Work Graph jobs | store / graph / runtime | Own chat UI schema |
| Engineering Memory | repo docs / EM scripts | Runtime authorization |

Kernel dispatch resolves only `packs.resolve_loaded_tool` then matches on
`ToolInvocation`. Skill grants are class-scoped (`FsWorkspace` → writes,
`Terminal` → commands). Gates: `scripts/gates/check-domain-modularity.py` and
`cargo test -p optimus-kernel --test domain_modularity`.

**Confirmed current behaviour:** project sessions load canonical roots from the
Rust-owned project authority store. Reads use the authorized root set. Writes
and commands persist the primary workspace hash, are high-risk under SmartDeny,
and reopen the exact matching authorized root when an approval is granted.

**Confirmed current behaviour:** tool streams use stable run/call/event IDs and
explicit lifecycle phases. Each transition is stored before delivery in an
ordered execution event table. Desktop session reload removes provider protocol
messages and attaches those events to the owning assistant turn; React reduces
them by call identity and deduplicates reconnect delivery by event identity.

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
| `execution.db` | kernel/execution | Versioned execution manifests, exact model/tool hashes and outcomes, ordered full tool lifecycle events, monotonic duration fields, ordered timing events, terminal status, trace links, and replay classification. |
| `project-authority.json` | kernel/project authority | Versioned canonical project roots, primary-root selection, and consumed native selection grants; renderer project state is not authority. |
| `cron.db` | kernel/cron | Interval schedules, exact lease owner/generation/token/deadline state, and latest status. |
| `gateway/gateway.db` plus files | kernel/gateway | Authoritative message claims/attempts/terminal outbox JSON plus reconciled inbox/outbox/processed/failed adapter files. |
| caller-selected agent registry/invocation DBs | kernel/agent | Immutable descriptor versions plus accepted invocation projections, ordered events, retry lineage, cancellation, terminal results, and validated effect links. |
| caller-selected workflow registry DB | kernel/workflow | Immutable validated workflow definitions; not workflow execution state. |
| caller-selected replay DB | kernel/replay | Immutable content-addressed fixture bundles and one terminal replay report per bundle. |
| caller-selected trace DB | kernel/trace | Canonical spans, ordered events, and one terminal span outcome. |
| `routing.db` telemetry tables | kernel/telemetry | Provenance-bound provider/model outcome, latency, and cost observations. |
| caller-selected evaluation DB | kernel/evaluation | Immutable candidate-bound baseline reports. |
| `workspace/.optimus/browser_state.json` | kernel/browser | Last HTTP page and bounded navigation history. |
| renderer local storage | React presentation | Versioned pane geometry, theme/density, local project `rootPaths[]`/primary root, session-to-project assignment, pins, and expansion state. This is not runtime permission authority. |

**Confirmed current behaviour (P18):** process-local durability is multi-file
SQLite under one home. There is **no** distributed transaction across those DBs.
Operators use `optimus doctor` (schema inventory + Work Graph quarantine) and
`optimus doctor backup-list` / durability-and-backup.md (merged)
for the backup path set (including workflow/agent ledgers). Universal retention
and a single migration framework across all stores remain residual.

**Confirmed residual:** Campaigns and Work Graph jobs share `optimus.db`.
Cross-contract agent/session links reconcile committed identities; they are not
distributed transactions.

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
fields. Free-text recall adds a `claims_fts` FTS5 lexical index that supplies
candidate ids only: every candidate is re-read from `claims` and re-gated, and
each hit carries the bitemporal standing that ranks stale below current
(ADR-0072). There is no embedding, vector search, reranker, knowledge graph, or
GPU retrieval implementation in the workspace.

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
candidate. Auto is a request selector that chooses connected Codex, configured
OpenAI-compatible, or offline in fixed order; it is never persisted as the
provider/model that executed. Expiring Codex access without refresh capability
is not connected for this selection. CLI, desktop, cron, and gateway use the
same resolver. Provider-specific wire parsing remains in the adapters.

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

**Confirmed current behaviour:** the Tauri production renderer loads built
relative assets embedded in the shell binary. The Tauri bridge (`host_invoke`)
forwards typed methods to the Rust host, which owns the bearer token and every
durable effect; the shell adds bounded chat streams, window chrome, and native
folder selection. The renderer never receives `OPTIMUS_HTTP_TOKEN`.

**Confirmed current behaviour (P15):** IPC matrix gate requires host registry ⊇
React `DesktopMethod` (the renderer surface over the Tauri bridge); every host
method is renderer-callable or classified non-invoke/main-only. Critical
invokes include approvals, project scopes, sessions, fs, settings, `term_run`,
and `jobs_list`. `project_root_stage_native` stays main-only. See ADR-0038.

**Confirmed current behaviour:** the workbench Browser surface drives the
kernel `browser_*` effector (HTTP SSRF-safe, CDP when available). The
Electron-era `WebContentsView` preview and its native annotation mode are
retired with Electron; the agent browser tools own all renderer browser
activity and no shared cookies, history, or automation target is claimed
beyond the effector's own bounded state.

**Confirmed current behaviour:** the React project catalog can group several
folder paths under one local project identity and nominate one primary root.
Legacy single-path records migrate to `rootPaths[]`. New roots become runtime
authority only after a native picker stages a single-use token and Rust accepts
the canonical scope; renderer presentation state alone grants nothing.

**Confirmed current behaviour:** desktop HTTP mode is explicitly development-only
and requires a 32-character bearer token. Effectful POSTs additionally require
an exact loopback origin and CSRF header; wildcard CORS is disabled. The gateway
requires its own 32-character bearer token and validates any supplied browser
origin. Both surfaces cap request bodies, apply fixed-window request limits,
bound aggregate operations, omit home paths from health responses, and return
stable redacted errors while retaining local stderr diagnostics.

**Confirmed current behaviour:** SmartDeny treats `WriteFile`,
`ProjectWriteFile`, `RunCommand`, `ProjectRunCommand`, and `ProjectServe` as
high-risk. `AssertFileEquals` does not require approval.

**Confirmed current behaviour (ADR-0059):** the Standard broker lane permits
direct project work while recognised remote/network commands and command-string
shell forms ask; uncheckpointed project deletes also ask. Classification does
not prove an arbitrary binary lacks network or ambient-credential authority,
so those remain explicit blockers to a universal Standard fallback.

**Confirmed current behaviour (ADR-0060 foundation):** owned-localhost
capabilities require a coherent project/session/run/process-tree/socket
constraint envelope, which cannot ride an unrelated capability. The pure broker
does not establish liveness. The agent CDP backend is public-only unless
constructed with one exact numeric HTTP loopback origin, and it checks
navigation, intercepted requests, and post-click URLs. The HTTP backend follows
redirects manually and validates each target before connection. The runtime now
contains a default-inactive lease registry: a copied binding cannot become
authority without exact live membership, the same opaque execution context, current
generation/expiry, retained-listener liveness, and a non-serializable use
guard. Revocation removes membership before bounded use drain and process
cleanup. No production constructor can create the opaque listener proof yet;
no production path can mint the execution context either. The structured
issuer/owned-server lifecycle, timer-driven expiry, restart orphan cleanup, and
worker/service-worker target coverage remain absent. This is still a
fail-closed authority substrate rather than a shipped localhost product path.

**Confirmed current behaviour (P12):** approved commands use `CommandFsEnvelope`
(default confined): Linux bwrap binds the workspace read-write only (no full
root rw bind); `UnrestrictedHost` is explicit break-glass. See ADR-0035.

**Known residual (product-visible):** Windows command FS is Job Object process-
tree ownership under confined mode; `ConfinedNoNetwork` fail-closes on non-
Linux. Provider/OAuth TLS is adapter-local beyond shared browser/search egress.

## Events, observability, and replay

**Confirmed current behaviour:** Work Graph events have an ordered SQLite
sequence and optional job/node IDs. Model turns expose in-process text, tool,
and status stream events. Sessions, cron, campaigns, gateway, skills, and memory
also retain subsystem-specific state.

**Confirmed current behaviour (P14):** machine-readable local causal export
`optimus.causal.v1` (`optimus trace export` / `write_causal_export`) is
store-backed, versioned, and redacts the Optimus home path. It does not re-run
live providers and is not OTLP. Merge gate
`scripts/gates/check-observability-gate.py` covers integrity, causal/export tests,
and export API surface. See ADR-0037.

**Confirmed current behaviour:** versioned execution manifests and immutable,
bounded, content-addressed fixture bundles support zero-effect offline replay.
Planning binds exact source manifest, trace, policy, tool catalog, stage order,
fixture hashes, and terminal evidence. Input or fixture drift fails before later
stages and one immutable replay report records the terminal comparison.

**Confirmed current behaviour:** canonical trace/span identities support ordered
append-only events, one terminal span outcome, traced route decisions, and
immutable execution-manifest trace links. Versioned evaluation datasets retain
ten declared cases and produce deterministic candidate-bound metrics, thresholds,
reports, immutable baselines, and regression comparisons. Comparison permits a
changed source-tree identity only while dataset, contract, tool catalog, route
policy, provider/model, threshold policy, report hashes, and metric schema remain
compatible. Report construction, baseline acceptance/loading, and comparison
revalidate supported identities, exact metric dimensions and arithmetic, unique
threshold policy, failure/pass projection, and content hash before returning or
persisting evidence. Report construction also rejects observations without trace
evidence when the matched dataset case declares that trace is required.

**Confirmed current behaviour:** production kernel turns create the execution
manifest and one parentless trace link atomically in the execution database.
Successful results expose that exact context; interrupted turns reuse it after
validating manifest identity and running status. Missing, malformed, mismatched,
or already-terminal resume evidence fails before model or tool execution. The
execution link does not claim that a corresponding `TraceStore` span exists.

**Confirmed current behaviour:** a public offline integrity executor exercises
the six required memory, SmartDeny, routing, cancellation/fencing, and gateway
cases against isolated local run state. It requires run-directory ownership
before execution, matches policy-specific denial outcomes, and returns a complete
deterministic failed report when setup is unavailable. It does not execute an
approved command or access the network. Usable runs persist one evaluation-owned
root span per case with hashed evidence and terminal status, then return the exact
read-back trace context and deterministic replay class. Independent retries use
fresh trace identities and stable normalized semantics.

**Confirmed current behaviour:** a separate exact four-case offline trajectory
runner reloads each successful turn's execution evidence and returns exact
assistant text, canonical invoked tools, terminal status, replay classification,
and root trace. Missing or mismatched persisted evidence fails the case; failed
cases carry no typed success evidence.

**Confirmed current behaviour:** the exact Priority-2 report runner owns a fresh
run directory, executes the four trajectory and six integrity cases, projects
their typed evidence in canonical dataset order, and returns one deterministic
candidate-bound report. Per-case latency and cost are mandatory explicit inputs;
they are not inferred from wall time or silently defaulted. Equal inputs yield
equal report bytes while run and trace identities remain fresh.

**Confirmed current behaviour:** `optimus eval report` reads candidate binding,
per-case measurements, and optional thresholds from separate bounded JSON files.
Typed policies are preflighted before evaluation run state. Success and threshold
failure both print the complete report; threshold failure exits non-zero. The
legacy four-case `eval run` command remains available.

**Confirmed current behaviour:**
`python scripts/tools/engineering_memory.py binding` emits the only context accepted by
the exact offline runner: the current canonical source-tree identity, canonical
evaluation/tool/routing source hashes, and fixed `offline/offline-scripted`
provider/model identity. The runner rejects context drift before creating run state.

**Confirmed current behaviour:** `optimus eval compare` reads two bounded exact
reports, invokes the canonical candidate-aware comparator, and prints one comparison
without creating the configured home. A valid regression is comparison evidence,
not an implicit release gate; invalid or incompatible reports fail without output.

**Confirmed bounded behaviour:** desktop stream delivery loss requests the same
cooperative token used by active providers and tool-loop boundaries. A turn can
still commit a durable effect and then fail before the session transcript is
saved.

**Confirmed current behaviour:** operators can reconstruct a turn from durable
stores via `load_causal_turn` / `optimus trace show` using a root trace id,
manifest id, or turn id. Security/policy fences map to a closed
`SecurityDenialCode` vocabulary when classifiable. Offline integrity + causal +
export surface tests are the observability gate
(`scripts/gates/check-observability-gate.py`, P14).

**Unknown or unresolved behaviour:** there is no OpenTelemetry/OTLP export (local
`optimus.causal.v1` export exists — ADR-0037), live security-denial event stream,
token accounting, artifact publication lineage, GPU/fallback telemetry, or a
transaction spanning trace, route, execution, runtime, agent, workflow, and
session stores. Fixture replay does not rerun live providers or external
effects. Logs remain non-authoritative.

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

1. **Partial product:** two built-in specialists and a registered-definition DAG
   runner exist; no model-chosen specialist router or open MCP agents.
2. **Partial product:** DAG executor runs registered built-in definitions with a
   closed specialist dispatch table; not a universal executor for arbitrary
   third-party definitions beyond registry validation.
3. **Partially implemented:** cancellation remains owner-specific.
4. **Confirmed contract, unresolved product:** metadata declarations do not create universal runtime cancellation/retry.
5. **Partially implemented:** policy and telemetry routing exist; evaluation-driven routing does not.
6. **Confirmed bounded behaviour:** fixture replay and local causal traces exist; live-effect replay and distributed tracing do not.
7. **Unknown/unresolved:** provenance and artifact publishing contracts.
8. **Confirmed (P12) / residual:** file effects use `cap-std`; approved commands
   use `CommandFsEnvelope` (Linux Confined = workspace-only RW; Windows Confined
   = Job Object process-tree residual; `ConfinedNoNetwork` fail-closed non-Linux;
   `UnrestrictedHost` explicit break-glass). See ADR-0035.
9. **Confirmed bounded behaviour:** Work Graph terminal uniqueness and campaign,
   cron, and gateway owner/generation/token/deadline fencing are implemented;
   external exactly-once delivery remains unresolved.
10. **Confirmed current behaviour (S+++ Phase 1B):** if durable effect links
    exist without matching tool transcript messages, session open injects
    deterministic repaired tool messages from the links and persists them.
11. **Resolved (P16):** duplicate ADR number `0016` aliased as **ADR-0016-A**
    (tool contract) and **ADR-0016-B** (FS sandbox); historical file names kept.
12. **Residual (owned by P16 banners / ongoing):** blueprint and historical
    phase notes may mix plan vs current; readers use status banners and the
    Confirmed/Planned/Unknown legend. Do not rewrite history to hide priors.
13. **Program:** architecture quality marks live in
    [architecture-marks.md](runbooks/architecture-marks.md). Foundation Phases 0–5:
    s-plus-trust-spine.md (atticked) (done). S+++ climb
    **P10–P19 done** — all architecture marks **S+++** (board:
    s-plus-plus-plus-review-2026-07-25.md (atticked);
    history: s-plus-plus-plus-program.md (atticked)).
    **Closed product program:** product-complete-program.md (atticked)
    (program P20–P29 **PRODUCT-COMPLETE** with residuals); historical task record
    full-app-microtasks.md (atticked). Current roadmap:
    current/roadmap.md (see specs/BACKLOG.md); named phase programs are
    historical implementation records unless that roadmap promotes them.
    Operator gate matrix: release-and-parity-gates.md (merged).
    Durability backup/doctor: durability-and-backup.md (merged).


## Daily use (desktop)

## Honest status

**Still not a full Hermes replacement** (no messenger gateway, cron UI, browser automation, skill editor).

**Now usable for local daily chat** if Codex OAuth is imported:

| Capability | Status |
|---|---|
| Real multi-turn sessions (SQLite) | yes |
| Sidebar = real sessions list | yes |
| Resume prior session | yes |
| Live **Codex** chat (SSE OAuth) | yes (default provider) |
| OpenAI-compatible API key chat | yes |
| Offline echo / memory demo | yes |
| Import Codex from Hermes (read-only) | yes (button + CLI) |
| Non-blocking chat (UI thread) | yes (worker thread) |
| Light/dark theme | yes |
| Terminal/tools in loop | yes when model calls tools |
| Browser effector | stub only |
| Gateway / Telegram / cron UI | no |
| Streaming tokens to UI | no (full turn then paint) |

## Run

```bash
cargo run -p optimus-desktop
```

1. Click **Import Codex** (or `optimus auth codex import-hermes` with same home)
2. **New session**
3. Provider = **gpt-5.4 · Codex**
4. Chat

Home: `%LOCALAPPDATA%/optimus`

## Build

`cargo build -p optimus-desktop` — green after this slice.


## Streaming (desktop)

## What shipped

End-to-end **token streaming** for daily chat:

| Layer | Behavior |
|---|---|
| `StreamEvent` | `TextDelta` · `ToolStatus` · `Status` |
| `ModelProvider::complete_streaming` | default one-shot; overrides stream |
| `ScriptedModel` | ~12-char chunks (UI/Playwright) |
| `CodexOAuthModel` | live SSE line reader → delta sink |
| `Kernel::turn_with_sink` | forwards model + tool events |
| HTTP | `POST /api/chat/stream` (SSE) |
| WebView | `chat_stream` IPC + `__optimusStream` pushes |
| UI | progressive bubble + caret while streaming |

## Verification

```text
cargo test --workspace -- --test-threads=1   # all green
cd apps/optimus-desktop && npx playwright test
  7 passed (3.6s)
```

Includes:
- Enter streams offline reply progressively
- SSE endpoint emits `delta` then `done`

## Run

```bash
# native window (streams via WebView IPC)
cargo run -p optimus-desktop

# Playwright / browser
cargo run -p optimus-desktop -- --http 8787
cd apps/optimus-desktop && npx playwright test
```

## Daily-use status (updated)

| Need | Status |
|---|---|
| Multi-turn sessions | yes |
| Sidebar sessions | yes |
| Enter-to-send | yes (Playwright) |
| Live Codex OAuth | yes |
| **Streaming tokens** | **yes** |
| HTTP e2e harness | yes |
| Gateway / cron / browser agent | no |

Still not a full Hermes OS — but local chat is now usable with progressive replies.


## Durability and backup

Date: 2026-07-25  
Planes: program **P18** · mark **Durability / crash safety** · delivery **PR #28**

## Scope of architecture Durability S+++

**In scope (Confirmed process-local / local SQLite):**

- Work Graph jobs/nodes/effects in `optimus.db` (exactly one terminal outcome;
  crash-resume; quarantine on corrupt projection).
- Campaign plans/leases in the same `optimus.db` (schema versioned).
- Session transcripts + effect links in `sessions.db` with repair-on-open when a
  durable effect link outlives a tool message.
- Local gateway delivery authority (`gateway/gateway.db` + adapter dirs) and
  cron leases (`cron.db`) as **local** fencing — not off-box exactly-once.
- Memory / skills / execution DBs as independent SQLite files under the same home.

**Out of scope for this mark (explicit residual):**

- External messaging **exactly-once** across third-party networks (Telegram,
  etc.). Local leases/claims remain Confirmed; cross-host delivery is not
  claimed S+++.
- A single distributed transaction spanning all home DBs.

## Backup set

Prefer copying the **entire Optimus home** while writers are stopped.

Minimum path set (also emitted by `optimus doctor backup-list`):

| Relative path | Role |
|---|---|
| `optimus.db` (+ `-wal`/`-shm` if present) | Work Graph + campaigns |
| `sessions.db` (+ wal/shm) | Transcripts + effect links |
| `memory.db` (+ wal/shm) | MetaMemory claims |
| `skills.db` (+ wal/shm) | Skills registry |
| `execution.db` (+ wal/shm) | Execution manifests / tool lifecycle |
| `cron.db` (+ wal/shm) | Cron schedules and leases |
| `gateway/gateway.db` (+ wal/shm) | Gateway claims/attempts |
| `gateway/inbox`, `gateway/outbox`, `gateway/processed`, `gateway/failed` | Adapter file queues |
| `routing.db` (+ wal/shm) | Routing telemetry |
| `settings.json` | Product settings (not secrets) |
| `workflow-runs.db` (+ wal/shm) | Durable workflow run ledger |
| `agent-invocations.db` (+ wal/shm) | Agent invocation ledger |
| `workflow-registry.db` (+ wal/shm) | Workflow definition registry |
| `agent-registry.db` (+ wal/shm) | Agent descriptor registry |
| `project-authority.json` | Project root authority |
| `artifacts/` | Content-addressed blobs |

### Cold backup procedure

1. Stop Optimus CLI, desktop host, gateway serve, and cron runners using the home.
2. `optimus --home <HOME> doctor backup-list` — confirm present paths.
3. Copy the home directory (or every present path from backup-list) to immutable storage.
4. Record product version: `optimus version --json`.

### Restore procedure

1. Stop writers.
2. Replace the home directory (or restore listed files in place).
3. `optimus --home <HOME> doctor` — expect schema versions OK and quarantine empty
   (or investigate quarantined jobs before resume).
4. `optimus --home <HOME> resume-all` only after doctor is clean for intended work.

## Doctor commands

```bash
# Multi-DB schema inventory + quarantine
optimus --home .optimus doctor
optimus --home .optimus doctor --json

# Backup path set
optimus --home .optimus doctor backup-list
optimus --home .optimus doctor backup-list --json
```

Doctor is **read-only** (never creates or migrates DBs). It exits non-zero when
schema skew, open/inspect failures, or quarantined jobs are reported.

## Crash / resume operator notes

- Running nodes require recovery: `optimus resume` / `resume-all` calls
  `recover_crashed_running` before resume.
- Prepared `RunCommand` attempts that crash mid-flight become **ambiguous** and
  are never blindly replayed.
- Session open repairs missing tool messages from durable effect links
  (deterministic JSON with `"repaired": true`).

## Related

- s-plus-plus-plus-p18-verification.md (atticked)
- [system-overview.md](architecture.md) state table
- Program phase P18 in s-plus-plus-plus-program.md (atticked)


## Optimus vs Hermes (measured)

> **Documentary status (P16, updated 2026-07-27): SUPERSEDED** by
> north-star-2026-07.md (atticked) via the
> [#59 wayfinder map](https://github.com/mustbearnold/Optimus-Agent/issues/59).
> Historical **blueprint / mission prose** — evidence of what was once
> intended, not a statement of truth. The "strictly better than Hermes"
> success criteria below were retired by
> [#63](https://github.com/mustbearnold/Optimus-Agent/issues/63); Hermes is no
> longer the yardstick. **Do not grade as Confirmed current behaviour.** For
> live topology and grades use [system-overview.md](architecture.md) and
> [architecture-marks.md](runbooks/architecture-marks.md).

**Mission:** Rebuild the personal agent category so Optimus exceeds Hermes Agent on *every* axis that matters in production: reliability, learning quality, memory integrity, cost, latency, security, multi-agent durability, desktop UX, Ubuntu-first quality, cross-platform discipline, evalability, and long-horizon autonomy — without sacrificing Hermes’ genuine strengths (provider freedom, gateway breadth, skills loop, cache discipline).

This is not a Hermes fork with a coat of paint. It is a greenfield architecture that **imports Hermes product lessons and rejects Hermes structural debt**.

---

## 0. North star

**Optimus is a durable operator runtime with a measured learning loop.**

- Hermes optimizes for: *self-improving single agent + many surfaces*.
- Optimus optimizes for: *verified work completion + compounding capability under budget, with evidence-native memory and crash-safe multi-agent campaigns*.

Success definition (must all be true):

1. **Parity-plus product surface** — everything Hermes users rely on daily (chat, tools, skills, cron, gateway, desktop, MCP, profiles) works at least as well.
2. **Strictly better closed loop** — skills and memory only promote when outcome metrics improve; bad skills cannot accumulate silently.
3. **Strictly better memory** — bitemporal evidence store is native, not a plugin afterthought; recalled content never becomes action authority.
4. **Strictly better economics** — progressive context loading + measured cache policy + tool-schema budgets cut tokens 2–4× on long sessions vs Hermes defaults.
5. **Strictly better durability** — jobs, subagents, and campaigns survive process death; resume is first-class.
6. **Strictly better security** — deny-by-default capabilities, skill sandboxing, provenance-bound authority, packaging integrity.
7. **Strictly better Windows** — native first-class host, not POSIX port with scars.
8. **Strictly better proof** — every capability has a hard real benchmark (agent trajectory suites, not only unit mocks).

---

## 1. What Hermes gets right (keep / surpass)

| Hermes strength | Why it matters | Optimus stance |
|---|---|---|
| Cache-stable system prompt | Biggest cost lever on long sessions | Keep invariant; make **progressive loading** compatible with caching via staged cache breakpoints |
| Skills as procedural memory | Compounding value over time | Keep; add **outcome-gated promotion**, tests, versioning, rollback |
| Provider-agnostic core | Avoid vendor lock-in | Keep; stronger adapter contract + capability matrix per model |
| Multi-surface same identity | CLI/gateway/desktop feel like one agent | Keep; one **Kernel**, many thin surfaces |
| Profiles isolation | Multi-persona / multi-tenant local | Keep; stronger tenant principal model |
| Cron + webhooks + kanban | Real operator durability hooks | Keep; unify under one **Job/Campaign runtime** |
| Narrow core tool waist (intent) | Every core tool taxes every call | Enforce harder than Hermes actually does today |
| Plugin/MCP edges | Extensibility without core bloat | First-class capability packs with signed manifests |

---

## 2. Where Hermes loses (and Optimus attacks)

### 2.1 Structural debt

Observed live tree shape (2026-07-18 local install):

- `run_agent.py` ~6.5k LOC
- `cli.py` ~15.8k LOC
- `gateway/run.py` ~21.9k LOC

These are accretion god-modules. They force every change through high-conflict files, make invariants hard to test, and couple product surface to agent loop.

**Optimus rule:** no module > ~800 LOC without a forced split. Core loop files are pure state machines with injectable ports.

### 2.2 Learning loop is unmeasured

Hermes creates skills after complex tasks. Quality is model-dependent. Skill explosion and mediocre skills are known failure modes. Curator is mostly inactivity hygiene, not outcome science.

**Optimus:** every skill is a versioned artifact with preconditions, postconditions, optional executable checks, and rolling success stats. Promotion to “always-load candidates” requires measured improvement or human pin.

### 2.3 Memory is too thin + too dangerous if thickened naively

Hermes core memory is intentionally tiny (~2.2k / ~1.4k chars) and frozen for cache. Deep memory is pluggable (Honcho, Mem0, etc.) and easy to get wrong (vector RAG as truth, last-write-wins, authority laundering).

**Optimus:** ships MetaMemory-class substrate natively:

- immutable experience ledger
- bitemporal claims (valid time + transaction time)
- procedural memory separate from semantic claims
- evidence packets on recall (never bare blobs)
- **recalled content is DATA, never instruction or capability**
- action requires live capability tokens, not remembered preference

### 2.4 Delegation is not durable

Hermes `delegate_task` is process-local. Parent exit kills children. Cron is durable but separate. Users experience “long mission” fragility.

**Optimus:** one **Durable Work Graph** for turns, tools, subagents, cron, and multi-day campaigns. Process is replaceable; graph is not.

### 2.5 Context tax

Hermes sends large tool schemas + skill catalogs + static guidance every turn. Progressive loading has been proposed but is not the architecture.

**Optimus:** **Capability Router** with:

- tiny always-on tool waist (file/terminal/web/memory/job/clarify)
- demand-loaded capability packs (browser, computer-use, home, office, …)
- skill *index* always available; skill *body* loaded on hit
- cache breakpoints designed so pack activation is an explicit, rare event (or new turn segment), not silent mid-prefix mutation

### 2.6 Security posture

Public analysis and user reports: powerful defaults, skill creation risks, container approval edge cases, “allow-all” feel. Packaging integrity for sidecars is a recurring class of bugs in adjacent desktop agent work.

**Optimus defaults:**

- capability-based sandbox (not YOLO culture)
- skills cannot expand privileges
- outbound network allowlists per profile
- signed skill/plugin manifests
- Windows package: pinned runtimes, full license closure, hash-verified sidecars

### 2.7 Dual-runtime desktop pain

Hermes desktop = Electron shell over Python agent. Two ecosystems, two update stories, Windows quirks multiply.

**Optimus desktop primary:** Tauri 2 + Rust kernel services + React UI (Heracles-class lessons), conversation-first like Codex desktop — progressive disclosure of tools/terminal/files, not IDE cosplay. CLI remains first-class for headless/VPS.

### 2.8 Windows second-class residue

Even with native Windows support, Hermes carries POSIX assumptions (test runner, PTY, path, env scrubbing).

**Optimus:** Windows x64 is tier-0 CI. Linux/macOS tier-0 equally. No feature merges without green on both families for that surface.

### 2.9 Reliability of long autonomous runs

Field reports: crons break, gateway breaks, fix-one-break-three, token burn, weak models thrash.

**Optimus:** supervisor tree (s6-like semantics even on Windows via a native supervisor), health probes, auto-restart with backoff, deterministic replay of failed tool segments, budget circuit breakers.

---

## 3. Architecture — deep modules at clean seams

### 3.1 Layer cake

```
┌─────────────────────────────────────────────────────────────┐
│ Surfaces: CLI · TUI · Desktop (Tauri) · Gateway · ACP · API │
├─────────────────────────────────────────────────────────────┤
│ Orchestration API (session, turn, job, approval, stream)    │
├─────────────────────────────────────────────────────────────┤
│ Kernel                                                      │
│  ├─ Conversation FSM                                        │
│  ├─ Context Assembler (cache tiers)                         │
│  ├─ Model Router + Provider Adapters                        │
│  ├─ Capability Router (tools/skills packs)                  │
│  ├─ Policy Engine (approvals, sandbox, budgets)             │
│  └─ Learning Controller (skill/memory promotion)            │
├─────────────────────────────────────────────────────────────┤
│ Durable Runtime                                             │
│  ├─ Work Graph (turns, tools, children, cron, campaigns)    │
│  ├─ Event Log (append-only)                                 │
│  ├─ Supervisor / Process Manager                            │
│  └─ Checkpoint & Rollback                                   │
├─────────────────────────────────────────────────────────────┤
│ Cognitive Stores                                            │
│  ├─ Core Pin (tiny, curated, cache-stable)                  │
│  ├─ MetaMemory (ledger + bitemporal + projections)          │
│  ├─ Session Transcript (FTS + scroll API)                   │
│  ├─ Skills Registry (versioned procedures)                  │
│  └─ Artifacts (files, screenshots, builds)                  │
├─────────────────────────────────────────────────────────────┤
│ Effectors                                                   │
│  Terminal · Files · Browser · Computer-Use · MCP · Net      │
└─────────────────────────────────────────────────────────────┘
```

### 3.2 Deep module interfaces (small surface, large behavior)

#### `Kernel::turn(input) -> TurnResult`

Caller knows: messages in, stream events out, approvals may pause, budgets enforced.  
Hidden: provider retries, tool dispatch, compression, memory writes, skill suggestions.

#### `WorkGraph::spawn(spec) -> JobId` / `resume(job_id)`

Caller knows: durable id, status machine, delivery target.  
Hidden: process placement, retries, child isolation, crash recovery.

#### `MetaMemory::recall(query, purpose) -> EvidencePacket`

Caller knows: purpose enum (`inform`, `constraint`, `procedure_lookup`, …).  
Fails closed on `action_authorize` purpose.  
Hidden: hybrid retrieval, conflicts, temporal views.

#### `Skills::resolve(task_signal) -> SkillHit[]` then `load(id, version)`

Index is cheap and cache-stable; bodies are demand-loaded and spilled out of hot context after use summaries.

#### `Policy::authorize(action, context) -> Decision`

Unified gate for shell, net, file write, browser, messaging send, skill install, memory export.

#### `ContextAssembler::build(snapshot) -> PromptLayers`

Explicit layers:

1. **L0 Immutable identity** (soul, safety, schema waist) — max cache life  
2. **L1 Session constants** (profile, cwd policy, enabled pack ids)  
3. **L2 Working set** (goal, todos, open errors) — may refresh on segment boundaries  
4. **L3 Ephemeral** (tool results, recalls, skill bodies) — never poison L0/L1

### 3.3 Language / runtime split (pragmatic)

| Layer | Language | Why |
|---|---|---|
| Kernel + Work Graph + Policy + Supervisor + MetaMemory engine | **Rust** | Crash isolation, concurrency, Windows service quality, single binary sidecars, auditability |
| Provider adapters + high-level tool orchestration glue | **Rust** primary; thin Python optional host for scientific/user scripts | One runtime for production path |
| Surfaces UI | **React/TS** (Tauri desktop, web dashboard) | Fast iteration, Heracles/Codex-class UX |
| User code execution sandbox | Isolated subprocess (Python/Node/etc. as tools, not core) | Don’t put the agent brain in the same process users can wreck |
| Skills content | Markdown + optional WASM/scripts with capability manifests | Portable procedural memory |

**Why not pure Python like Hermes?**  
Python is excellent for research loops and plugins, terrible as the long-lived Windows supervisor + multi-agent process fabric once you care about durability and memory safety. Optimus can still *run* Python tools and offer a Python SDK; the **kernel is not Python**.

**Migration bridge:** Optimus speaks Hermes-compatible session import + skill import + OpenAI-compatible tool loop so users can claw-migrate and hermes-migrate.

---

## 4. Exceeding Hermes axis-by-axis

### 4.1 Agent loop

**Hermes:** classic tool loop in a mega-module; compression when near limit; max turns config.

**Optimus:**

- Explicit **Conversation FSM**: `Idle → Reasoning → ToolDispatch → AwaitApproval → Compressing → HandedOff → Done/Failed`
- **Segmented turns**: tool bursts can finalize a “segment” and re-open with preserved L0/L1 cache
- **Steer/interrupt** as first-class events (Hermes has these; Optimus makes them durable in the work graph)
- **Verification phase** optional but default for coding/ops goals: after claim-done, run declared checks before user-facing success
- **Circuit breakers**: repeated identical tool failure, budget burn rate, thrash detector → degrade to plan/ask, not infinite loops

### 4.2 Tools & capabilities

**Hermes:** 60–80+ tools; toolsets; MCP; browser; computer-use; etc. Schema cost is real.

**Optimus Capability Packs:**

| Pack | Examples | Load mode |
|---|---|---|
| `core` | read/write/search/patch, terminal, process, web_search/extract, clarify, todo, memory, jobs | Always |
| `browser` | CDP/harness browser | On demand |
| `desktop` | computer-use, UI automation | On demand + elevated policy |
| `media` | vision, imagegen, TTS/STT, video | On demand |
| `office` | docs/sheets/pptx | On demand |
| `social` | X search, messaging send | On demand + stricter policy |
| `devex` | git/gh deep tools, kanban, PR workflows | On demand |
| `mcp:*` | per-server generated pack | On demand, allowlisted tools |

Pack activation rules:

- Model requests pack via `need_capability(pack)` tool (tiny schema)
- Or router heuristic from user utterance
- Activation is a **segment boundary** event (logged, user-visible, cache-aware)
- Hard cap on concurrent loaded packs per session

### 4.3 Skills (procedural memory 2.0)

Hermes skills are markdown procedures the model may load and patch.

Optimus skill object:

```yaml
id: windows-rust-lnk1104
version: 3
status: candidate | proven | pinned | deprecated
preconditions: ["os=windows", "toolchain=msvc"]
steps: ...
checks:
  - type: command
    run: "cargo test -q"
    expect_exit: 0
metrics:
  uses: 12
  successes: 10
  avg_tokens: 48000
  last_verified: 2026-07-18
provenance:
  created_from_job: job_...
  parent_skills: []
permissions_required: [terminal, fs_workspace]
```

Rules that beat Hermes:

1. **Create as candidate only** after complex success
2. **Auto-prove** by replaying checks on next similar task or synthetic fixture
3. **Promote to proven** only after N successes or human pin
4. **Never** let skill text grant new permissions
5. **Curator is metric-driven**, not only TTL
6. Compatibility with agentskills.io + Hermes skill import

### 4.4 Memory (MetaMemory-native)

Exceed Hermes four-layer story by making the deep substrate *correct*:

1. **Core Pin** — tiny USER/AGENT pins, cache-stable, curated (Hermes-like size discipline retained on purpose)
2. **Working Memory** — goal stack, constraints, open errors (session-local, durable)
3. **Ledger** — append-only events (messages, tools, decisions, outcomes)
4. **Episodic** — trajectories with attempts/outcomes
5. **Semantic bitemporal claims** — “user prefers X” with valid-time, corrections, conflicts preserved
6. **Procedural** — skills registry
7. **Artifacts** — files with manifests
8. **Meta-memory** — which recalls helped/harmed; retrieval feedback loop

Recall returns **EvidencePacket**:

- current vs historical vs transition
- conflicts explicit
- citations to ledger ids
- trust/authority fields
- abstain when insufficient

Security invariants (non-negotiable):

- origin-bound authority
- evidence ≠ instruction
- no durable action capability in memory rows
- scope filters before top-k
- privacy erasure across all projections
- no destructive summarization as sole evidence

Hardware note (user constraint class): default local embeddings / rerank must fit **RTX 5070 12GB** with headroom for concurrent agent work; heavy models optional, not required.

### 4.5 Multi-agent & long-horizon work

**Hermes gaps:** leaf/orchestrator delegation, kanban board, cron — three systems users must mentally unify; children not durable.

**Optimus Work Graph unifies:**

- interactive turns
- background tool processes
- subagents
- cron ticks
- multi-day campaigns
- human approval nodes

Properties:

- every node has idempotency key, budget, policy snapshot, parent, delivery
- crash → supervisor restarts worker → graph resumes
- subagent contexts are isolated stores with explicit handoff artifacts (not “summary vibes only”)
- parent can await, poll, or subscribe
- **no nested spawn bomb** without quota; depth and fanout are hard-metered
- campaign templates: “repo completion”, “research watch”, “release train”

### 4.6 Scheduling & gateway

Keep Hermes platform breadth ambition, but:

- gateway adapters are **plugins with contract tests** and chaos suites
- one message bus; adapters are pure I/O
- backpressure and per-chat queues (no thundering herd tool storms)
- home-channel delivery is transactional with the work graph
- pairing/auth is capability tokens, not ambient trust forever

### 4.7 Desktop UX (beat Electron Hermes + match Codex feel)

Primary UX principles (aligned with Heracles/Codex desktop taste):

- conversation-first
- compact status (model, cost, job health, sponsor/earnings slot if needed)
- progressive disclosure: terminal/files/browser as drawers, not permanent IDE chrome
- live job/subagent watch without stealing focus
- mid-turn steer that feels instant
- native notifications only for approvals / true blockers
- Windows installer: MSI/NSIS with **complete** license + native dependency closure

### 4.8 Cost & performance

Targets vs Hermes default long-coding session:

| Metric | Hermes baseline class | Optimus target |
|---|---|---|
| Tool schema tokens/turn | all enabled tools | core + ≤2 packs |
| Skill body tokens | easy over-injection | index always; body on hit; spill after |
| Cache hit rate | high if disciplined | ≥ Hermes on L0/L1; pack switches are rare segments |
| Useless tool thrash | common on weak models | thrash detector + tool-result hashing |
| Aux model use | ad hoc | explicit tiers: extract/classify local small; reason frontier |

Additional levers:

- result spill to disk with handles (Hermes-like) but typed handles the model must re-fetch intentionally
- provider prompt-cache breakpoint API awareness (Anthropic/OpenAI/xAI as available)
- local small models for routing, skill suggest, memory extract on GPU-class machines

### 4.9 Security & safety

Default profile: **smart-deny**, not smart-allow.

- filesystem jail per workspace + explicit escapes
- network policy per profile (default: no arbitrary egress from code sandbox)
- high-risk tools always approval-gated (even in “yolo” only if profile explicitly `unrestricted` and local single-user)
- skills/plugins: signed manifests, permission declarations, no ambient admin
- secret redaction on by default and **not** toggleable by the model mid-session (Hermes lesson — keep)
- prompt-injection scanning on project files and web/tool output; untrusted content fenced
- audit log exportable and hash-chained (tamper-evident; optional external anchor)

### 4.10 Eval, benchmarks, self-improvement science

Hermes learns; Optimus **measures learning**.

Built-in eval harness:

1. **Trajectory suite** — frozen tasks with graders (file state, tests pass, HTTP contracts)
2. **Memory suite** — LongMemEval-class + bitemporal correction probes + poisoning tests
3. **Skill regression** — promoted skills must keep passing fixtures
4. **Gateway chaos** — disconnects, duplicate webhooks, partial sends
5. **Windows GUI / computer-use** — real apps first (TrueCUA philosophy): installed apps, DOM/CDP/a11y before pixels
6. **Cost suite** — tokens and wall time budgets as first-class scores
7. **Security suite** — authority laundering, path traversal, skill privilege escape

Learning loop only writes *proven* improvements into default load paths. Everything else stays candidate or personal pin.

### 4.11 Developer experience & codebase health

- deep modules, deletion test, interface = test surface
- contract tests at every seam (provider, tool pack, memory, gateway adapter)
- `optimus doctor` stronger than hermes doctor: reproduces common break classes
- schema-first config (versioned migrations, no silent defaults that flip live↔demo)
- single writer conventions for worktrees; campaign locking
- docs as executable examples

---

## 5. Product surfaces (parity map)

| Surface | Hermes | Optimus |
|---|---|---|
| CLI chat | yes | yes, thinner, faster start |
| Ink/TUI | yes | yes or repl-first; not a second monolith |
| Desktop | Electron | **Tauri 2 native** primary |
| Gateway 20+ platforms | yes | yes, adapter SDK + cert suite |
| ACP / IDE | yes | yes |
| Dashboard | yes | yes, job/memory/skill observability first |
| OpenAI-compatible proxy | yes | yes |
| Profiles | yes | yes + org/tenancy hooks |
| Cron | yes | Work Graph schedules |
| Kanban multi-agent | yes | Campaign board on Work Graph |
| MCP client/server | yes | yes, pack-gated |
| Plugins | yes | signed capability packs |
| RL / datagen hooks | Nous-specific | optional research pack, not core waist |

---

## 6. Data & identity model

- **Principal**: user / agent / profile / service
- **Workspace**: directory + policy + secrets scope
- **Session**: interactive conversation
- **Job**: durable work unit (may outlive session)
- **Campaign**: graph of jobs toward a goal
- **Artifact**: content-addressed outputs
- **MemoryEntity**: typed, scoped, bitemporal
- **SkillVersion**: immutable version; pointer moves

Session search remains free FTS (Hermes session_search lesson: don’t tax aux LLM for scrollback).

---

## 7. Migration & coexistence

Day-one importers:

- Hermes `state.db` sessions → Optimus transcript ledger
- Hermes skills directories → candidate skills (not auto-proven)
- Hermes MEMORY.md/USER.md → core pin + semantic claims with low confidence until confirmed
- OpenClaw/Claude/Codex project files (AGENTS.md, CLAUDE.md) — load as workspace constitution

Coexistence: Optimus can run beside Hermes; different home dir (`OPTIMUS_HOME`). No destructive takeover.

---

## 8. Delivery plan (build order that cannot lie)

### Phase 0 — Spine (2–3 weeks)

Vertical slice only:

- Rust kernel turn loop with one provider
- core tool waist (fs + terminal) on Windows + Linux
- sqlite event log + session resume
- CLI surface
- golden trajectory: “create repo file, run test, pass”

**Exit:** crash process mid-tool; resume; finish task.

### Phase 1 — Durable jobs + policy (2 weeks)

- Work Graph + supervisor
- approvals
- budgets / circuit breakers
- checkpoints

**Exit:** kill -9 during multi-step job; resume from last committed node.

### Phase 2 — MetaMemory MVP (3–4 weeks)

- ledger + claims + recall evidence packets
- core pin integration cache-safe
- correction, conflict, forget
- adversarial security probes green

**Exit:** bitemporal “prefers X → prefers Y” and poisoning tests pass.

### Phase 3 — Skills 2.0 + learning controller (2–3 weeks)

- import Hermes skills
- candidate/proven lifecycle
- curator metrics
- skill permissions enforced

**Exit:** skill improves graded task tokens/success vs baseline; bad skill does not promote.

### Phase 4 — Capability packs + browser/desktop (3 weeks)

- pack loader + schema budget
- browser pack
- computer-use pack (real app benchmarks)

**Exit:** schema tokens/turn ≤ target; browser task suite pass rate ≥ Hermes baseline on same model.

### Phase 5 — Gateway + multi-platform (ongoing)

- adapter SDK
- Telegram/Discord/Slack first
- chaos tests

### Phase 6 — Desktop Tauri (parallel after Phase 1)

- conversation-first UI
- job watch
- approval UX
- packaging integrity (pinned runtimes, licenses)

### Phase 7 — Eval harness & public benchmarks (continuous from Phase 0)

- every phase adds graded tasks
- weekly regression gate blocks release

### Phase 8 — Breadth pack explosion

Only after waist is stable: office, home automation, media, RL, etc.

---

## 9. Explicit non-goals (for v1)

- Becoming a multi-tenant SaaS cloud agent OS on day one
- Replacing all IDEs
- Unlimited autonomous self-modification of kernel code in production profiles
- Claiming “AGI” or unbounded self-improvement without graders
- Shipping 20 messaging platforms before Work Graph resume is boringly reliable

---

## 10. Competitive one-liner matrix

| Axis | Hermes | Optimus |
|---|---|---|
| Core identity | Self-improving personal agent | Verified durable operator with measured learning |
| Brain host | Python monolith accretion | Rust kernel + thin surfaces |
| Memory | Small pins + plugins | MetaMemory-native evidence store |
| Skills | Create/patch freely | Outcome-gated versions + permissions |
| Multi-agent | Soft delegation + kanban | Unified durable Work Graph |
| Context | Cache-stable but heavy schemas | Cache tiers + progressive packs |
| Desktop | Electron + Python | Tauri conversation-first |
| Security default | Powerful/permissive leaning | Capability deny-by-default |
| Proof | Tests + doctor | Trajectory/memory/security/cost gates |
| Windows | Supported | Tier-0 equal citizen |

---

## 11. First decisions to lock before code floods in

1. **Kernel language:** Rust (recommended) vs Python rewrite discipline  
2. **Desktop:** Tauri-first vs CLI-only MVP then UI  
3. **Memory:** embed MetaMemory in-process vs separate memory daemon  
4. **Compatibility:** hard requirement to import Hermes state on week 1?  
5. **Default approval posture:** smart-deny vs Hermes-like smart  
6. **Model default:** provider-agnostic empty vs batteries-included OAuth path  
7. **Scope of v1 gateway:** none / one platform / three platforms  

---

## 12. Recommendation (label + rationale)

**Recommendation: Build Optimus as a Rust-kernel, MetaMemory-native, Work-Graph durable operator with Hermes skill/session import and Tauri conversation-first desktop — not a Hermes fork.**

Rationale: Hermes’ product insight (learning loop + multi-surface personal agent) is correct, but its Python accretion core, unmeasured skill promotion, thin/plugin memory, non-durable delegation, and schema-taxed context are structural ceilings. Exceeding “in every way” requires changing the waist, not polishing the shell.

---

## 13. Immediate next actions

1. Grill/lock the seven decisions in §11.  
2. Write ADR-0001 (kernel + work graph) and ADR-0002 (memory invariants).  
3. Scaffold monorepo: `crates/optimus-kernel`, `crates/optimus-memory`, `apps/cli`, `apps/desktop`, `packs/core`, `evals/`.  
4. Implement Phase 0 spine with crash-resume golden test on Windows.  
5. Import a sample Hermes skill pack as *candidates* and prove the promotion gate before building more features.

---

*Document status: architecture blueprint for empty project tree `E:\Projects\Optimus Agent` as of 2026-07-18. Not an implementation claim.*


## Versioning

**Status:** active, fail-closed  
**Optimus product version:** `0.1.0`  
**Tracked Hermes target:** `0.19.0` at upstream revision `8967e73e`  
**Verified Hermes parity version:** none

## Why there are two versions

Optimus has an independent product version and a separate Hermes parity version.
They answer different questions:

- **Optimus product version** is normal SemVer from `Cargo.toml`.
- **Hermes target version** is the exact Hermes release currently being audited.
- **Hermes parity version** is `null` until every gate in this document passes for one immutable Optimus revision.

A normal Optimus development release may use any honest independent version. If
its three-part numeric SemVer core equals the tracked Hermes number, the release
check refuses it unless the Hermes parity claim is verified. Prerelease or build
suffixes do not disguise that collision. This prevents an accidental or
marketing-only numerical match.

Example while work is incomplete:

```text
Optimus Agent 0.1.0
Hermes target: 0.19.0
Hermes parity: unverified
Frozen Hermes feature contracts: 2063
```

## Non-negotiable parity invariant

Optimus may claim `Hermes parity: X.Y.Z` only when the exact candidate:

1. implements or strictly exceeds **every** feature contract frozen from Hermes
   `X.Y.Z`;
2. has executable, revision-bound evidence for each feature contract;
3. has no `missing` or `partial` row in the human parity rollup;
4. matches or beats Hermes success rate and deterministic quality score;
5. matches or beats Hermes p50 and p95 wall latency and time-to-first-token;
6. matches or beats Hermes cost per successful task and peak resident memory;
7. passes the required comparison scenarios on the same machine, model,
   provider, permissions, and paired randomized task order;
8. uses fresh evidence from a clean, immutable Optimus revision; and
9. has a completed audit against the official Hermes documentation.

There are **no feature waivers**. An equivalent Optimus design is allowed, but
it must prove the same user-visible outcome and edge behavior. A missing Hermes
feature cannot be traded for an unrelated Optimus advantage.

## Sources of truth

| File | Purpose |
|---|---|
| `docs/architecture/optimus-version.json` | Version target, claim, release rules, and benchmark thresholds |
| `docs/architecture/hermes-baselines/hermes-0.19.0.json` | Frozen machine inventory for Hermes 0.19.0 |
| `docs/architecture/hermes-manual-capabilities.json` | Non-CLI product capabilities curated from official docs/source |
| `docs/architecture/hermes-feature-evidence.json` | Per-feature Optimus evidence bound to a commit |
| `docs/architecture/hermes-performance-evidence.json` | Raw paired benchmark samples and protocol provenance |
| `docs/architecture/parity-capability-ledger.json` | Human-readable capability rollup and ownership |
| `scripts/tools/optimus_version.py` | Capture, validation, status, release, and promotion gate |
| `scripts/gates/check-parity-ledger.py` | Rollup validation plus version-system integrity check |

Executable evidence outranks prose. Architecture documents are not parity
proof unless a claim also names a passing trajectory and an existing evidence
artifact.

## Frozen Hermes inventory

The v0.19.0 baseline contains **2,063 distinct contracts** and has SHA-256:

```text
cafbcf313b4fbd7885b4df9b888a2539885d8d62ec55e6df1cf88dc0e66cf725
```

It inventories:

- recursively discovered CLI commands and options;
- slash commands, aliases, and subcommands;
- toolsets and statically registered tools;
- provider catalog entries;
- bundled messaging platforms; and
- non-CLI capabilities from the official product surface.

The source capture is tied to official commit `8967e73e`, not to the locally
modified Hermes checkout. Normalized ID collisions are retained as independent,
deterministically suffixed contracts; capture never drops one silently. MCP
server tool names are intentionally dynamic and unbounded, so the frozen
contract covers MCP client/server behavior rather than arbitrary third-party
runtime names.

The machine capture has zero warnings. The separate official-documentation
inventory audit remains `pending`, so parity is blocked even if someone were to
populate evidence prematurely.

## Per-feature evidence contract

`hermes-feature-evidence.json` maps each frozen feature ID to a claim. A passing
claim has this shape:

```json
{
  "cli.command.example": {
    "status": "verified",
    "evidence": ["path/to/current/test-or-report"],
    "trajectory": "cargo:package/test-name",
    "verified_at": "2026-07-23T12:00:00Z",
    "optimus_revision": "40-character-git-commit-sha"
  }
}
```

Rules:

- Every baseline ID must be present and `verified`.
- Evidence paths must exist.
- A named executable trajectory is mandatory.
- Evidence older than 30 days does not pass.
- All feature claims must refer to the same clean Optimus revision.
- Unknown IDs are schema errors, not ignored extensions.

`missing`, `partial`, `not-applicable`, `waived`, and prose-only evidence never
pass the parity gate.

## Comparative performance contract

The performance report stores raw paired samples. It does not accept manually
entered aggregate claims. Every required scenario needs at least 30 paired
samples across at least three distinct seeds:

1. cold start;
2. single-turn response;
3. multi-tool turn;
4. long session;
5. session resume;
6. scheduled job;
7. browser task; and
8. delegated task.

Each sample contains `hermes` and `optimus` records with `success`, a
reproducible `quality_score`, and the metrics required by that scenario.
The gate recomputes all statistics.

Hard thresholds:

| Axis | Requirement |
|---|---|
| Success rate | Optimus ≥ Hermes |
| Deterministic quality | Optimus ≥ Hermes |
| Wall time p50 and p95 | Optimus / Hermes ≤ 1.0 |
| TTFT p50 and p95 | Optimus / Hermes ≤ 1.0 |
| Cost per successful task | Optimus / Hermes ≤ 1.0 |
| Peak RSS p50 and p95 | Optimus / Hermes ≤ 1.0 |

The report must also affirm same machine, same model, same provider, same tool
permissions, and randomized paired order. It must hash the dataset, deterministic
grader, benchmark harness, Hermes binary, and Optimus binary, identify the
machine/provider/model, and record each sample's case ID, seed, and execution
order. Both `hermes-first` and `optimus-first` samples are required. Evidence is
valid for 30 days and must target the exact Hermes baseline and Optimus commit.

## Commands

```bash
# Human and machine-readable status
python3 scripts/tools/optimus_version.py status
python3 scripts/tools/optimus_version.py status --json

# Structural integrity; incomplete parity is reported but is not an error
python3 scripts/tools/optimus_version.py validate

# Strict full-parity gate; expected to fail until all work is complete
python3 scripts/tools/optimus_version.py gate

# Release preflight. Development versions pass; false matching claims fail.
python3 scripts/tools/optimus_version.py release-check

# Existing rollup plus version-system integrity
python3 scripts/gates/check-parity-ledger.py

# Architecture S+++ claim hygiene (not Hermes product parity)
python3 scripts/gates/check-architecture-marks.py

# Record parity only after all blockers are gone
python3 scripts/tools/optimus_version.py promote --reviewer "reviewer identity"

# Built CLI status
optimus version
optimus version --json
```

Both `scripts/rebuild-install-relaunch.sh` and
`scripts/rebuild-install-relaunch.ps1` run `release-check` before build/binary
selection, then run it again and revalidate both selected binary versions
immediately before stopping or replacing an installed application. Their
`VERSION.txt` and `install-meta.json` record the target, parity value, claim
status, and frozen feature count.

## Capturing a clean Hermes baseline

Never capture from a dirty or locally patched Hermes tree. Use an exact detached
worktree and the installed Hermes virtualenv only as the dependency runtime:

```bash
source_repo="$HOME/.hermes/hermes-agent"
clean_source="$(mktemp -d /tmp/optimus-hermes-0.19.0-XXXXXX)"
git -C "$source_repo" worktree add --detach "$clean_source" 8967e73e
python3 scripts/tools/optimus_version.py capture-hermes \
  --hermes-source "$clean_source" \
  --hermes-python "$source_repo/venv/bin/python"
git -C "$source_repo" worktree remove --force "$clean_source"
```

Capture updates the baseline hash in the version manifest and both evidence
files. Existing evidence is therefore invalidated whenever the baseline bytes
change.

## When Hermes publishes a new version

1. Update `hermes_target` to the new exact version, release date, and upstream
   revision.
2. Reset `parity_claim` to `unverified` with null metadata.
3. Capture a clean baseline from that exact revision.
4. Re-audit the official docs and mark the inventory audit complete only after
   resolving every discrepancy.
5. Add evidence for every new or changed feature contract.
6. Re-run all paired comparison scenarios on one immutable Optimus revision.
7. Run `validate`, the repository test suites, `gate`, and `release-check`.
8. Use `promote` only after the gate has no error or blocker.

A previously verified older Hermes parity version may remain historical, but it
must not be presented as parity with the newly tracked release.

## Current honest status

Optimus `0.1.0` tracks Hermes `0.19.0`, but parity is **unverified**:

- feature contracts verified: `0 / 2063` under the new strict per-feature schema;
- rollup rows below parity: `37 / 51`;
- required performance scenarios passing: `0 / 8`;
- official-documentation inventory audit: pending.

This is intentional. The version system exists to prevent the number from
advancing ahead of the product and evidence.


## Release and parity gates

Date: 2026-07-25  
Planes: program **P17** · grade mark **Release / parity gating** · delivery **PR #27**

**Status:** Confirmed operator contract for merge hygiene vs product release.  
Architecture **S+++** for this mark does **not** require full Hermes parity
(2,063 feature contracts). It requires that gates remain **fail-closed**, that
operators know which command is merge-safe vs release-blocking, and that grade
claims cannot greenwash missing phase evidence.

## Two version questions (do not collapse)

| Question | Answer lives in | Green means |
|---|---|---|
| May I ship an ordinary Optimus development build? | `optimus_version.py release-check` | Product SemVer is honest and does not falsely equal Hermes without a verified claim |
| May I claim Hermes parity `X.Y.Z`? | `optimus_version.py gate` + evidence corpus | Every frozen feature, rollup row, and performance scenario is proven on a clean revision |

Full policy: the merged Versioning section above.

## Gate matrix

| Gate | Command | Pre-merge (PR / local) | Pre-release (ship binary / install) | Pre-parity-claim | Notes |
|---|---|:---:|:---:|:---:|---|
| Version structural integrity | `python3 scripts/tools/optimus_version.py validate` | ✅ | ✅ | ✅ | Incomplete parity is reported; structural errors fail |
| Development release honesty | `python3 scripts/tools/optimus_version.py release-check` | ✅ | ✅ (required by installer) | ✅ | Passes independent SemVer; blocks numeric Hermes collision without verified claim |
| Strict Hermes parity | `python3 scripts/tools/optimus_version.py gate` | ❌ (expected red until complete) | ❌ unless claiming parity | ✅ required | Fail-closed; **not** an architecture S+++ blocker |
| Parity ledger rollup | `python3 scripts/gates/check-parity-ledger.py` | ✅ | ✅ | ✅ | Evidence paths must exist; `parity`/`win` need trajectory |
| Architecture marks claim hygiene | `python3 scripts/gates/check-architecture-marks.py` | ✅ | ✅ | optional | Fails if a mark is graded **S+++** without done phase / required paths |
| Observability | `python3 scripts/gates/check-observability-gate.py` | ✅ when touching kernel/runtime/packs/eval | recommended | optional | Cargo integrity + causal/export surface |
| Desktop IPC matrix | `python3 scripts/gates/check-desktop-ipc-matrix.py` | ✅ when touching desktop/tauri/ui | recommended | optional | Host ⊇ React = Tauri classification |
| Domain modularity | `python3 scripts/gates/check-domain-modularity.py` | ✅ when touching packs/kernel/store | recommended | optional | Single `ToolDesc` catalog / plane separation |
| Crate layers | `python3 scripts/gates/check-crate-layers.py` | ✅ when touching crate graph | recommended | optional | Control-plane peel deps |
| Engineering Memory | `python3 scripts/tools/engineering_memory.py check` (+ `generate` / `validate` when stale) | ✅ | ✅ | optional | Generated maps must not be hand-edited |
| Runtime / pack hold suites | `cargo test -p optimus-runtime` / `optimus-kernel` / `optimus-packs` as touched | ✅ scoped | full workspace before major ship | optional | See program hold suites |
| Installer re-gate | `scripts/rebuild-install-relaunch.*` | n/a | ✅ | n/a | Runs `release-check` before binary selection and again before replace |

Legend: ✅ expected green for that class of change · ❌ not required (and often red by design).

## What architecture S+++ does **not** require

- Full Hermes feature inventory green (`gate` PASS).
- Every parity-ledger row at `parity` or `win`.
- Performance scenario suite complete.
- Marketing claim that Optimus “is” Hermes `X.Y.Z`.

Those remain **product parity** work under `program:parity` / version promote /
Track Z after product-complete. The Release mark grades the **gate system**
(fail-closed scripts, docs, claim hygiene), not product completeness.

**Sources of truth (do not collapse):**

| Question | Authority |
|---|---|
| Architecture mark exits / hold | [architecture-marks.md](runbooks/architecture-marks.md); history s-plus-plus-plus-program.md (atticked) (P10–P19 done) |
| Daily-app phase exits → PRODUCT-COMPLETE | product-complete-program.md (atticked) (program P20–P29) + ledger |
| Merge vs ship vs Hermes claim | this matrix (`release-check` vs `gate`) |

## Operator quick paths

### Before opening / merging a PR

```bash
python3 scripts/tools/optimus_version.py release-check
python3 scripts/gates/check-parity-ledger.py
python3 scripts/gates/check-architecture-marks.py
python3 scripts/tools/engineering_memory.py check
# plus dimension gates for the files you touched
```

### Before install / ship of a development build

```bash
python3 scripts/tools/optimus_version.py release-check
python3 scripts/gates/check-parity-ledger.py
# installer scripts re-run release-check around binary selection
```

### Only when claiming Hermes parity

```bash
python3 scripts/tools/optimus_version.py validate
python3 scripts/gates/check-parity-ledger.py
python3 scripts/tools/optimus_version.py gate          # must PASS
python3 scripts/tools/optimus_version.py release-check
python3 scripts/tools/optimus_version.py promote --reviewer "…"
```

## Sources of truth

| Artifact | Role |
|---|---|
| `docs/architecture/optimus-version.json` | Product version, Hermes target, claim, release rules |
| `docs/architecture/parity-capability-ledger.json` | Human rollup (51 rows); not the 2,063-feature gate |
| `docs/architecture/architecture-marks.md` | Architecture quality grades (S+++ climb) |
| `docs/plans/s-plus-plus-plus-program.md` | Phase exit criteria; P17 owns this matrix |
| `scripts/tools/optimus_version.py` | Capture / validate / gate / release-check / promote |
| `scripts/gates/check-parity-ledger.py` | Rollup + version integrity |
| `scripts/gates/check-architecture-marks.py` | S+++ claim ↔ phase done / path existence |

## Related verification

- s-plus-plus-plus-p17-verification.md (atticked)
- sota-scorecard (parity planning rollup, not architecture grades; merged above)


## SOTA scorecard

Updated: 2026-07-28 · thesis-axis re-key (north-star C-criteria); 13/50 runnable trajectories, unclassified pinned at 37 shrink-only; projects.scope+updater+pty+native-cua partial

**Status banner:** This scorecard is a **parity/planning rollup**, not the
architecture quality grade sheet. For modular architecture grades (S+++ climb)
see [architecture-marks.md](runbooks/architecture-marks.md). For current topology and
Confirmed behaviour see [system-overview.md](architecture.md).

**Default product shell (Confirmed):** Tauri + React over Rust host
(exclusively — no Electron, no Wry rollback since 2026-08-05, spec-012).
Do not read “tao+wry Windows desktop shell” below as the default
install path.

**Source of truth:** `docs/architecture/parity-capability-ledger.json`  
**Validator:** `python scripts/gates/check-parity-ledger.py`  
**Rule:** executable current-repository evidence outranks architecture blueprints and historical phase prose. A `parity` or `win` row requires an existing evidence path; every row's trajectory is either runnable (`cargo:`/`playwright:`, resolved to a real target by the validator) or pinned on the validator's shrink-only unclassified list.
**Release-version gate:** `docs/architecture/optimus-version.json` plus `python scripts/tools/optimus_version.py gate`. The 50 rows below are a planning rollup, not sufficient for a product-level Hermes parity claim. The strict v0.19.0 baseline contains 2,063 per-feature contracts.

## Current ledger summary

| State | Count | Meaning |
|---|---:|---|
| **win** | 4 | Current executable evidence demonstrates a structural advantage over Hermes |
| **parity** | 41 | A bounded Hermes-equivalent capability has current executable evidence |
| **partial** | 4 | Useful implementation exists, but the Hermes behavior/surface is incomplete |
| **missing** | 1 | No complete executable path exists yet |
| **total** | 50 | Capability rows tracked by the executable ledger |

## Defensible wins

- Crash-resumable Work Graph effects
- Evidence-fenced bitemporal MetaMemory
- Outcome-gated, permission-closed Skills lifecycle
- Durable SmartDeny approval model

These are narrow evidence-backed wins, not a claim that the complete product is already superior.

## Implemented parity slices

- OpenAI-compatible provider client
- Codex OAuth Responses provider
- Streaming desktop chat
- Durable session reopen
- Electron + React default desktop shell (Wry legacy optional)
- Sandboxed Files list/read
- Bounded terminal job stream
- Sequential durable write/command campaigns
- Deterministic offline eval suite
- Store-backed causal reconstruction + local export (`optimus.causal.v1`)
- Fail-closed tool ads↔handler registry + progressive pack schema budget (program P21)
- Files mutate under SmartDeny (mkdir/rename/delete/patch + write) (program P22)
- Coordinated preview + agent browser under ADR-0040 (not shared CDP session) (program P23)
- Web search versioned extract + provenance URL (program P23)
- Annotation gallery + explicit Add to prompt (program P23)
- HTTP browser SSRF without CDP (program P23)
- Thinking blocks separate from assistant text + timed tool lifecycle cards (program P24)
- Session FTS, archive/unarchive, durable pins + sort (program P24)
- Memory FTS: free-text claim recall (`memory_search`) with per-hit standing/provenance, no new dependency (ADR-0072)
- Artifacts gallery, filters, export + bulk zip (program P25)
- Cron create/pause/resume/remove/history workbench (program P25)
- Skills/memory/packs consoles + redacted logs + command palette (program P26)
- Gateway outbox receipts, ambiguous-send recovery, mock Telegram adapter, messaging UI (program P28; external EO residual)
- Provider catalog + ordered failover, pack-gated MCP mock, signed packs (program P27)
- Product ship path: Electron install default, doctor shell/isolation/gateway/packs, ADR-0043 no auto-updater (program P29)
- S7: profile homes, leased child agents, CUA pack scaffold, Hermes importers
- Track Z: offline comparative runner, surface/media/breadth scaffolds, Discord/Slack mock adapters

## Material partials

- Installed native paint/accessibility (`desktop.native-cua`): the PF-00 installed-app CUA baseline is not committed, so the row carries no evidence. Playwright covers paint/layout supplementally and does not substitute for installed-app proof (see `skills/optimus-native-ui-testing`). Regenerate PF-00 to a tracked path to restore the parity claim.
- Project isolation honesty (configured vs enforced) with concurrent multi-project mutate lease residual
- Release updater: no in-app signed auto-update channel (ADR-0043); reinstall script is the upgrade path
- Terminal PTY: Linux multi-tab session store scaffold; full interactive I/O residual

## Leading product losses

1. Hermes strict parity gate (2063 contracts + performance scenarios) still unverified
2. Live multi-tab ConPTY I/O product UI
3. Live computer-use effectors under heavy approval
4. Live Discord/Slack bot transports (mock enqueue only)

## Current architecture truth

- **Default installed desktop:** Electron + React workbench over Rust `optimus-desktop --host-only` (ADR-0028). Not Tauri.
- **Legacy rollback:** tao + wry native shell (WebKitGTK / WebView2) via install “Legacy Wry” action.
- Native Wry IPC: ADR-0014 custom-protocol path; host HTTP mode is a test / Electron transport path.
- Browser: agent `browser_*` effector (HTTP SSRF-safe; CDP when available) is separate from the Electron sandboxed preview `WebContentsView`.
- Artifacts: content-addressed store under `{home}/artifacts` with gallery/filters/export under `exports/` (program P25)
- Campaigns: sequential WriteFile/RunCommand plus leased child-agent coordinator (S7)
- Gateway: SQLite authority + config-gated live Telegram long-poll (`optimus gateway telegram run`) + Telegram mock + Discord/Slack mock enqueue (live Discord/Slack residual)
- Retrieval: two SQLite FTS5 lexical indexes (`sessions_fts`, `claims_fts`); the claim index narrows only — every hit is re-authorized against `claims` and labelled with its bitemporal standing (ADR-0072). No vector, embedding, graph, reranking, or GPU index.
- Capabilities: PRODUCT-COMPLETE + S7/Track Z scaffolds; Hermes gate not claimed
- Architecture quality marks: [architecture-marks.md](runbooks/architecture-marks.md) (S+++ program)

## Baseline commands of record

```bash
just verify
```

`just verify` runs every gate above through `scripts/verify.sh`, the single
source of truth shared by the justfile, managed land, humans, and coding agents.
Narrower tiers: `just gates` · `just check` · `just test` ·
`just ui`.

The Hermes parity gate is deliberately excluded from `just verify` because it is
fail-closed by design; run it with `just parity`.

PF-00 baseline evidence: **absent**. The installed-app CUA baseline has never
been committed, which is why `desktop.native-cua` is `partial`. Regenerate it to
a tracked path (not gitignored `local/`) to restore the parity claim.

## Honest statement

Optimus has evidence-backed architectural wins and broad product/ecosystem scaffolds. It is **not yet Hermes-strict-parity**: the ledger currently contains 4 partial and 1 missing capability row (packs.breadth re-marked by ADR-0068: breadth claimed through refusing scaffolds was cosmetics, and the scaffolds are gone), but Hermes feature-contract and performance gates remain unverified (optimus-native claims only for a small first batch). The Hermes parity version therefore remains `null`; it cannot become `0.19.0` until the full inventory, comparative, security, cost, durability, packaging, and native-platform gates pass.
