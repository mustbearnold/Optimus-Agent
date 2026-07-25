---
knowledge_type: architecture
status: current
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
  - docs/maps/repository-and-ownership.md
  - docs/maps/memory-and-retrieval.md
  - docs/maps/model-routing.md
  - docs/maps/security-and-approvals.md
  - docs/maps/observability-and-evaluations.md
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

## Current topology

**Confirmed current behaviour**

```text
CLI, legacy Wry Desktop, or Electron React workbench
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
| `apps/optimus-desktop` | Confirmed current behaviour | Rust host (`--host-only`) for Electron + legacy Wry/Tao shell (WebKitGTK / WebView2), frozen IPC registry, bounded worker queues, inline legacy UI, and loopback HTTP. |
| `apps/optimus-electron` | Confirmed current behaviour | **Default installed** React shell; main authenticates host IPC, context-isolated preload, chat AbortControllers, sandboxed preview `WebContentsView`, bounded annotations; `OPTIMUS_ELECTRON_UI=legacy` and Wry install action remain rollback. |
| `apps/optimus-ui` | Confirmed current behaviour | React 19 workbench; typed `DesktopMethod` transport matching Electron allowlist; multi-folder presentation with Rust scope authority; Preview browser distinct from agent `browser_*` tools. |
| `crates/optimus-kernel` | Confirmed current behaviour | Provider-agnostic turn loop, strict tool dispatch, sessions, execution manifests, credential protection, canonical routing, browser/search effectors, and filesystem sandbox. Re-exports agent/workflow/artifacts/ops for surfaces. |
| `crates/optimus-agent` | Confirmed current behaviour | Versioned specialist descriptors, immutable registry, durable invocation/cancellation/retry/terminal ledger, effect provenance links. |
| `crates/optimus-workflow` | Confirmed current behaviour | Workflow definitions/registry, durable DAG `WorkflowRunStore`, built-in specialist verticals and registered-definition executor. |
| `crates/optimus-artifacts` | Confirmed current behaviour | Content-addressed handoff/workbench artifact store under `{home}/artifacts`. |
| `crates/optimus-ops` | Confirmed current behaviour | Operator services: durable local gateway delivery authority and cron schedule store. Kernel re-exports for surface convenience; does not own the turn loop. |
| `crates/optimus-eval` | Confirmed current behaviour | Offline integrity/trajectory harnesses, versioned evaluation reports/baselines, and zero-effect fixture replay. Depends on kernel; kernel does not depend on eval. |
| `crates/optimus-browser` | Confirmed current behaviour | Optional CDP browser backend for agent tools; not the Electron Preview `WebContentsView`. |
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
`Terminal` → commands). Gates: `scripts/check-domain-modularity.py` and
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

**Confirmed current behaviour:** the Electron production renderer loads built
relative assets from `optimus-app://ui/`. Its context-isolated preload exposes a
bounded method/chat/Browser/window contract; the Rust bearer token remains in
Electron main. Main validates that calls originate from the owning Optimus
renderer, caps serialized requests, allowlists method names, and permits one
foreground SSE stream.

**Confirmed current behaviour (P15):** IPC matrix gate requires host registry ⊇
Electron `DESKTOP_METHODS` = React `DesktopMethod`; every host method is
invoke-allowlisted or classified non-invoke/main-only. Critical invokes include
approvals, project scopes, sessions, fs, settings, `term_run`, and `jobs_list`.
`project_root_stage_native` stays main-only. See ADR-0038.

**Confirmed current behaviour:** the user-facing Electron preview is a
main-owned sandboxed `WebContentsView` (`nodeIntegration: false`,
`contextIsolation: true`, `sandbox: true`, separate partition). It accepts HTTPS
and loopback HTTP, denies remote insecure HTTP, privileged/non-web schemes,
permissions, downloads, and new windows, and is physically aligned to a
React-measured content hole. This preview is not the Rust agent `browser_*`
effector and no shared cookies, history, or automation target is claimed.

**Confirmed current behaviour:** explicit native annotation mode captures one
clicked element and returns bounded URL/title/tag/role/label/text/rectangle
context; it consumes the click and supports Escape, surface-change, and timeout
cancellation. It does not return HTML or selectors. Renderer overlays suspend
the child view before Settings, project-source management, or task UI appears,
then restore it at settled bounds after close.

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
`ProjectWriteFile`, `RunCommand`, and `ProjectRunCommand` as high-risk.
`AssertFileEquals` does not require approval.

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
`scripts/check-observability-gate.py` covers integrity, causal/export tests,
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
`python scripts/engineering_memory.py binding` emits the only context accepted by
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
(`scripts/check-observability-gate.py`, P14).

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
    [architecture-marks.md](./architecture-marks.md). Foundation Phases 0–5:
    [s-plus-trust-spine.md](../plans/s-plus-trust-spine.md) (done). S+++ climb
    **P10–P16 done** (Doc **S+++**); active next: **P17** release/parity in
    [s-plus-plus-plus-program.md](../plans/s-plus-plus-plus-program.md).
