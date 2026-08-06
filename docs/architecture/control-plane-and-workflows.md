---
doc_id: architecture-control-plane-workflows
doc_type: explanation
plane: current
status: current
authority: canonical
summary: Control plane, durable workflow runtime, terminal outcomes, and agent execution — current behaviour.
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: architecture
owns:
  - crates/optimus-workflow/src/lib.rs
  - crates/optimus-agent/src/lib.rs
  - crates/optimus-runtime/src/lib.rs
---

# Control plane, workflows, and agent execution

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
