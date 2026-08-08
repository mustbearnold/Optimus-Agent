---
doc_id: spec-034-recursive-children
doc_type: reference
plane: work
status: current
authority: canonical
summary: Recursive children for Optimus. A parent kernel session spawns child kernel sessions with one typed task prompt. The spawn returns an admission handle at once. The children live in a durable registry with a depth limit, daemon-backed execution, cancellation, tombstones, and usage attribution. This is Phase 1a.3 of the best-agent roadmap.
reviewed_on: 2026-08-08
review_by: 2026-11-08
knowledge_type: specification
covers:
  - crates/optimus-workflow/src/children.rs
  - crates/optimus-workflow/src/specialist_vertical.rs
  - crates/optimus-kernel/src/session/children.rs
  - crates/optimus-kernel/src/execution.rs
  - crates/optimus-kernel/src/execution_schema.rs
  - crates/optimus-host/src/children.rs
  - crates/optimus-packs/src/invocation.rs
  - crates/optimus-packs/src/catalog.rs
  - apps/optimus-cli/src/children.rs
depends_on:
  - docs/decisions/0086-goals-are-session-scoped-budget-enforced-objectives.md
  - docs/decisions/0087-the-session-message-plane-is-a-durable-ops-store.md
  - specs/003-kernel-turns/spec.md
  - specs/005-agents-workflows/spec.md
  - specs/025-session-messaging/spec.md
  - specs/026-goals/spec.md
validated_by:
  - crates/optimus-workflow/tests/recursion.rs
  - crates/optimus-workflow/tests/attribution.rs
  - crates/optimus-runtime/tests/cancellation.rs
---

# Spec-034: Recursive children — child kernel sessions with usage attribution

Status: draft
Owner: optimus-agent-development (prompt-only owner)

## Revision table

| Round | Verdict | Findings | Fixes |
|---|---|---|---|
| 1 | DRAFT | Initial draft from best-agent roadmap v2 Phase 1a.3 (review 96/100). Roadmap text: "A child is a full kernel session with its own store context. The parent creates a child with one typed task prompt. The call returns an admission handle immediately. It never waits for the answer. The parent keeps a registry of direct children. The registry survives host restart. The child inherits provider, skills, and policy from the parent, or an explicit selection. Enforce a depth limit. The default is 1. Children run daemon-backed. They survive parent client detach. The parent can delete a child. Deletion writes a durable tombstone. Attribute child usage to the parent turn (Spec-034). Persist the attribution in the execution store. Show it in the context tree." | The registry lives in `sessions.db` (session-scoped state, ADR-0086). The attribution lives in `execution.db` (the execution store). The roadmap "process tree" wording maps to the child execution tree in v1. The tree covers the turn loop, the descendants, and the runtime effects. |
| 2 | REJECTED (round 1 gate) | B1: the Current-state claim "no execution engine calls these contracts" is false; the P10/ADR-0033 DAG executor in specialist_vertical.rs exercises AgentRequest today; roadmap point 1 has no R-number. B2: `Capability` mis-cited to spec-019; the class is ToolPolicy::Capability (approval-free, session-local); the "high-risk action" framing contradicts it. B3: re-adoption can double-apply effects; the turn loop is not resumable; no single-runner claim. B4: `deleted` as a status contradicts exactly-one-terminal (a terminal child moved to `deleted` enters a terminal state twice). N1-N9: citation precision (dispatch.rs:276-289), A1 determinism (offline_pace), A4 depth-raise, STE-100 (8 long sentences, 2 gerunds), runner-vs-tombstone race, law-12 migration wording, retry tie-in, packs_budget.rs citation, re-adoption attribution totality. | [R1-B1] The Current state section reports the DAG executor honestly. Roadmap point 1 disposes to the existing executor. A child is a kernel session, not an AgentRequest invocation. [R1-B2] R9 cites ToolPolicy::Capability (optimus-packs lib.rs:368; invocation.rs:182-188; ADR-0086:81-82). The spawn is approval-free and session-local. Child external effects stay SmartDeny-gated via the inherited effect policy. [R1-B3] Adoption re-runs only a child with no manifest. An interrupted child settles to failed with reason `crash_interrupted`. The serve health guard is the single-runner claim. A2 gains a crash-injection no-duplication assertion. [R1-B4] Deletion writes a `deleted_at` marker. The terminal outcome never changes. R8 now says "terminal event types at most once". [R1-N1..N9] Applied: re-cites, offline_pace in A1, depth-raise in A4, STE-100 pass, tombstone-race rule, law-12 migration wording, retry tie-in, packs_budget.rs cite, attribution totality. |
| 3 | REJECTED (round 2 gate) | B1: the tombstone/adoption race — a child deleted mid-run stays non-terminal and tombstoned; adoption admits it, the deleted row must not reactivate; the child ends with zero terminal outcomes. B2: the single-runner claim covers daemons only. A surface turn on a child session (CLI `--session`, chat_turn_cancellable) creates a second manifest. This falsifies "at most one manifest by construction". B3: the revision table claims the law-12 migration wording was applied; the body has none. N1: pin the exit-3 cite to serve.rs:126 (`EXIT_REFUSED`). N2: "No transition MAY enter" → "A transition MUST NOT enter a terminal state twice". N3: pin the adoption ordering — the runner records the manifest before the status leaves `spawned`. N4: STE-100 residuals (summary, R2 row, R5, R9, ceremony). N5: stale open questions 1 and 3 are resolved in the body. N6: state explicitly that `deleted` is a lifecycle event, not a terminal. | [R2-B1] Deletion serializes with the run. A delete on a non-terminal child requests cancellation and waits for the `cancelled` terminal before the tombstone write. Adoption excludes `deleted_at` rows. [R2-B2] The daemon is the only runner of a child turn in v1. A surface turn on a child session refuses with a diagnostic. The attribution row references the task manifest; the child has at most one manifest. [R2-B3] R2 and R8 now state the additive migration rule (law 12, like `session_goals`). The table claim matches the body. [R2-N1] serve.rs:126 pinned. [R2-N2] MUST NOT form. [R2-N3] Manifest-before-`running` ordering stated. [R2-N4] Long sentences split; summary trimmed; A-criteria keep the spec-025 length (accepted). [R2-N5] Stale questions removed. [R2-N6] `deleted` is a lifecycle event, not a terminal. |
| 4 | REJECTED (round 3 gate) | B1: the crash-window hole in deletion — the delete wait presupposes a live runner. A spawned child whose daemon died has none. Cancellation is silently lost. The delete can wait forever. N1: one sentence in the round-2 findings cell exceeds 20 words. | [R3-B1] The delete wait is bounded. A non-terminal child with no live runner settles at the delete call to `cancelled` with the reason `runner_lost`. The tombstone follows. Adoption skips terminal and tombstoned rows. The settle never double-terminals. A5 gains the daemon-dead branch. [R3-N1] The round-2 findings cell sentence is split. |
| 5 | REJECTED (round 4 gate) | B1: the cancel path has the same crash-window hole as the delete path. A cancelled child still re-runs after a daemon restart. The cancel record is only in the in-memory token. N1: the round-2 fixes cell sentence still exceeds 20 words. N2: the new R6 bounded-wait clause has a 24-word sentence. N3: the surface-turn sentence has 21 words. | [R4-B1] Cancellation is durable: the cancel call records a `cancel_requested` marker with the reason before the token cancel. The cancel call mirrors the delete bounded-wait rule: a non-terminal child with no live runner settles at the cancel call to `cancelled` with the reason `runner_lost`. Adoption settles a child with the marker to `cancelled` with the reason `cancel_requested` and never re-runs it. A2 and A4 gained the recovery branches. [R4-N1] The fixes cell sentence is split. [R4-N2] The R6 clause is split. [R4-N3] The surface-turn sentence is split. |
| 6 | APPROVED (round 5 gate) | None blocking. N1: the R2 registry-row sentence has 28 words; the reviewer quoted its split. | [R5-N1] Applied post-approval: the R2 registry-row sentence is split into short sentences, and the row gains the `cancel_requested` marker column (the body already required the marker in R6). |

## Purpose

A campaign is an ordered DAG of deterministic steps. It is not a
parent session that delegates one typed task to a full child session.
This spec defines the missing recursive surface. A parent session
spawns child kernel sessions. The spawn returns an admission handle at
once. The parent tracks the children in a durable registry. Each child
runs daemon-backed to exactly one terminal outcome. The parent
attributes the child usage to its own turn.

The roadmap names this phase "Spec-005 execution". The child registry
extends spec-005 R2: invocation, cancellation, retry, and terminal
outcomes stay durably ledged, now for the session-recursion surface.

## Current state (Confirmed behaviour)

- `crates/optimus-agent/src/lib.rs` holds the typed contracts:
  `AgentRequest`, `AgentResult`, `AgentDescriptor`, the immutable
  `AgentRegistry`, and the `AgentInvocationStore` (lib.rs:581). The
  registry validates a request with `validate_request` (lib.rs:407).
- `crates/optimus-workflow/src/specialist_vertical.rs` is the P10 /
  ADR-0033 DAG executor. It constructs `AgentRequest`
  (specialist_vertical.rs:830). It validates the request through the
  registry (`AgentInvocationStore::begin` → `validate_request`). It
  links the child run (specialist_vertical.rs:854) and settles
  `AgentResult` (specialist_vertical.rs:906, :943). Integration
  tests exercise it (`crates/optimus-kernel/tests/workflow_dag.rs`).
  No runtime entry point invokes it today.
- `crates/optimus-workflow/src/child_lease.rs` holds the
  `ChildLeaseCoordinator` seed (tests only, no production caller).
- `crates/optimus-workflow/src/workflow_run.rs` holds
  `WorkflowRunStore` with `WorkflowRunChild`, `link_child`, and
  `assert_can_begin_child`. This is the campaign DAG surface, not the
  session hierarchy.
- `Kernel::open_session(home, config, session_id)` (kernel lib.rs:454)
  creates a new session or resumes an existing one. Every session
  shares one home and its stores: `sessions.db`, `execution.db`,
  `memory.db`, `skills.db`, `optimus.db`, and the message store.
- The host daemon (`optimus serve`) runs a worker pool
  (host dispatch.rs:188). Each chat stream runs
  `chat_turn_cancellable(home, params, on_event, cancellation)`
  (host chat.rs:408). The call opens a kernel, routes the provider,
  and runs one turn loop with a `CancellationToken`. The turn loop is
  in-memory: a crash loses the run state. The terminal event arrives
  exactly once via a blocking send (host dispatch.rs:276-289).
- The serve health guard is the single-runner claim. A second `serve`
  on a healthily served home refuses with exit 3 (`EXIT_REFUSED`,
  host serve.rs:126; the call site is apps/optimus-cli/src/main.rs:1332-1335).
- `ExecutionStore` (kernel execution.rs:150) records one manifest
  per turn in `execution.db`. The manifest carries the session id,
  the provider, and the model. It carries the model-call usage:
  input, output, total, reasoning, and cached tokens
  (execution_schema.rs:30-44).
- The message plane (spec-025) established the durable-store
  pattern. The pattern has ordered events and unique-per-type
  enforcement (message_plane.rs:300). It has exactly-one-terminal
  discipline and the tool ceremony: a new pack, a `ToolInvocation`
  variant, a catalog entry, and a policy class.
- The Core pack schema budget is full. The budget test is
  `crates/optimus-packs/tests/packs_budget.rs`.
- No session hierarchy exists today. No tool can create a child
  session. No usage attribution exists across sessions.

## Requirements

### R1. Child admission

- A child MUST be a full kernel session with its own store context.
  The child session row and transcript live in `sessions.db`, like
  any other session (roadmap point 2).
- The parent MUST create a child with one typed task prompt. The task
  prompt becomes the first user message of the child transcript
  (roadmap point 3).
- The spawn call MUST return an admission handle immediately. The
  handle carries the child session id, the depth, the initial status,
  and the creation time. The spawn call MUST NOT wait for the answer
  (roadmap point 4).
- The admission handle MUST return only after the registry row is
  durable. A crash before the write returns an error, never a phantom
  child (roadmap point 5).
- The task prompt MUST be bounded: 1 to 64 KiB of text. The bound
  equals `MAX_TASK_BYTES` in the agent crate.
- Roadmap point 1 (specialist execution) disposes to the existing
  DAG executor (Current state). A child is a kernel session, not an
  `AgentRequest` invocation. The typed contracts remain the DAG
  surface.

### R2. Child registry

- The parent MUST keep a registry of direct children. The registry
  MUST survive host restart (roadmap points 5-6).
- The registry MUST live in `sessions.db` in a new table
  `session_children`. This follows the ADR-0086 precedent: session
  state belongs in the session store, not the effect ledger.
- A registry row MUST carry: parent session id, child session id,
  depth, task prompt sha256, the provider and model snapshot, the
  status, and the creation, adoption, and terminal timestamps. The
  row MUST also carry a `deleted_at` marker. The marker is null until
  deletion. The row MUST carry a `cancel_requested` marker. The
  marker is null until a cancel call.
- The child status machine MUST be: `spawned`, `running`, `succeeded`,
  `failed`, `cancelled`. A child MUST record exactly one terminal
  outcome. A transition MUST NOT enter a terminal state twice. A
  transition MUST NOT leave a terminal state.
- The registry tables MUST use additive `CREATE TABLE IF NOT EXISTS`
  migrations with a schema-version bump (law 12), like
  `session_goals` (crates/optimus-kernel/src/session/goals.rs:20,
  :271).
- Deletion MUST NOT change the terminal status. Deletion writes the
  `deleted_at` marker (R6). The display layer shows a deleted child
  with its true terminal outcome and the marker.
- The registry MUST expose the direct children of a session with the
  status, the depth, and the timestamps. It MUST NOT expose the
  grandchildren as direct children.
- The only retry-like path is re-adoption (R4). No separate child
  retry exists.

### R3. Depth limit

- The spawn MUST enforce a depth limit. The default limit MUST be
  1. A child session may not spawn its own child unless the limit is
  raised (roadmap point 8).
- The depth of a child MUST equal the parent depth plus one. A root
  session has depth 0. A direct child has depth 1.
- A spawn beyond the limit MUST fail with a clear diagnostic that
  names the limit and the depth.
- The limit lives in the kernel config, like the effect policy.

### R4. Daemon-backed execution

- Children MUST run daemon-backed. The host daemon owns the child
  execution. A child MUST survive parent client detach (roadmap
  point 9).
- The child execution MUST run as a kernel turn loop in the daemon
  worker pool, with its own `CancellationToken`. The task prompt is
  the turn message. The turn loop ends at a model stop, at the step
  budget, or at cancellation, like any kernel turn.
- The daemon MUST re-adopt non-terminal children at start. Adoption
  re-runs a child ONLY when the child has no execution manifest. The
  runner records the execution manifest before the status leaves
  `spawned`. A manifest-bearing child never re-runs.
- A child with an interrupted manifest MUST settle to the `failed`
  terminal with the reason `crash_interrupted`. It MUST NOT re-run.
  The turn loop is in-memory and not resumable. A re-run would apply
  its effects twice.
- The single-runner claim is the serve health guard: one daemon per
  home at a time (Current state). The registry writes form the
  exclusive claim on a child.
- Re-execution MUST preserve the exactly-one-terminal invariant. A
  crash may interrupt a run. It never produces two terminals.
- The daemon MUST be the only runner of a child turn in v1. A
  surface turn on a child session (CLI `--session`, desktop chat)
  MUST refuse with a diagnostic that names the child. The refusal
  mirrors the embedded-kernel rule (A9). The child has at most one
  manifest: the task turn.
- A kernel that is not daemon-backed (the embedded CLI mode) MUST
  refuse `session_spawn` with a clear diagnostic. It MUST NOT spawn a
  silent in-process child that dies with the client. Registry reads
  and cancellation MUST still work from any surface.

### R5. Inheritance and selection

- A child MUST inherit the parent provider, the parent skills, and
  the parent policy by default. Inheritance copies the parent
  routing (provider, model, fallback list). It copies the parent
  effect policy, the parent autonomy profile, and the parent command
  envelope. Skills inherit by home. The child reads the same skill
  registry (roadmap point 7).
- The spawn MUST accept an explicit provider and model selection. An
  explicit selection overrides the inherited routing for that child
  (roadmap point 7).
- The child permission ceiling MUST NOT exceed the parent ceiling.
  The spawn MUST validate any permission override with
  `AgentPermissions::is_subset_of`. A child never outranks its parent.

### R6. Cancellation and deletion

- The parent MUST be able to cancel a child. Cancellation MUST stop
  the child turn loop. Cancellation MUST cancel the child
  descendants, all the way down the hierarchy. The roadmap
  acceptance says: "Child cancellation stops the child process
  tree". The v1 wording is the child execution tree: the turn loop,
  the descendants, and the runtime effects.
- Cancellation MUST be durable. The cancel call MUST record a
  `cancel_requested` marker with the reason in the registry. The
  record happens before the token cancel. A crash after the record
  does not lose the cancellation.
- The cancel call MUST mirror the delete bounded-wait rule. A
  non-terminal child with no live runner settles at the cancel call.
  The cancel records the `cancelled` terminal with the reason
  `runner_lost`. A live runner settles `cancelled` via the token.
- Adoption MUST settle a non-terminal child with a `cancel_requested`
  marker to `cancelled` with the reason `cancel_requested`. Adoption
  MUST NOT re-run such a child. The settle never double-terminals.
- The parent MUST be able to delete a child. Deletion MUST write a
  durable tombstone: the `deleted_at` marker on the registry row
  (roadmap point 10). The terminal outcome stays unchanged.
- Deletion MUST serialize with the run. Deletion of a non-terminal
  child MUST request cancellation first. The delete call MUST wait
  for the `cancelled` terminal before it writes the tombstone. The
  wait ends at the runner settle; the cancellation check runs at each
  model step, so the wait is bounded.
- The wait MUST be bounded. When the runner is gone, the wait MUST
  NOT hang. A non-terminal child with no live runner settles at the
  delete call. The delete records the `cancelled` terminal with the
  reason `runner_lost` and writes the tombstone. A live runner means
  an open kernel on the child session. The live-session registry
  (spec-025) carries the fact.
- The daemon adoption MUST skip any row that is terminal or
  tombstoned. The settle in the previous clause never double-terminals.
- A runner that observes the tombstone MUST stop work and MUST NOT
  record a terminal. The tombstone covers the post-terminal cleanup
  only. The status guard rejects any second write.
- A deleted row MUST never return to an active status. Adoption MUST
  exclude any row with a `deleted_at` marker. Attribution rows MUST
  survive the tombstone.

### R7. Usage attribution

- The child usage MUST be attributed to the parent turn that spawned
  the child. The attribution MUST persist in the execution store,
  `execution.db` (roadmap point 11).
- A new table `execution_child_attribution` MUST link the parent
  manifest, the child session, and the child manifest. The child
  manifest reference MUST be unique: one child turn attributes at
  most once.
- An attribution row MUST snapshot the child usage: input, output,
  total, reasoning, and cached tokens, plus the duration. The write
  happens when the child turn finishes.
- The attribution MUST reconcile. The snapshot totals MUST equal the
  sums of the child manifest model calls in the same store.
- An attribution row MUST reference the task manifest: the child
  first turn. The child has at most one manifest by construction
  (R4, exclusive-runner). Reconciliation stays total across a crash.
- The context tree MUST show the children with their status and their
  attributed usage. The session detail JSON and the CLI MUST include
  the child list (roadmap point 11).

### R8. Observability

- Child lifecycle events MUST be durable and ordered. A new table
  `session_child_events` MUST record: `spawned`, `adopted`, `running`,
  `succeeded`, `failed`, `cancelled`, `deleted`. The `deleted` event
  is a lifecycle event, not a terminal.
- The events MUST follow the message-plane pattern: one row per event
  type per child, unique, in state-machine order. Terminal event types
  appear at most once per child. The table uses the additive migration
  rule of R2 (law 12).
- The events MUST be queryable by child session id and by parent
  session.

### R9. Security and ceremony

- The spawn tool MUST be a `Capability` tool. The class is
  `ToolPolicy::Capability` (optimus-packs lib.rs:368). The class
  matches the spec-025 session tools and the spec-026 goal tool
  (invocation.rs:182-188; ADR-0086:81-82). The class contract says:
  session-local, no external effect, no approval. The spawn call
  creates a session row and a registry row.
- The child external effects stay approval-gated. The child inherits
  the parent effect policy (R5). Every external child effect goes
  through the same SmartDeny path as a parent effect (law 7
  downstream). The permission ceiling (law 6) holds via
  `AgentPermissions::is_subset_of`.
- The new tools MUST live in a new `children` pack, on-demand, like
  the `collaboration` pack (spec-025). The Core pack schema budget is
  full (crates/optimus-packs/tests/packs_budget.rs).
- The ceremony MUST follow spec-019 and the spec-025 precedent.
  The ceremony covers the `ToolInvocation` variants and the catalog
  entries. It covers the module ratchet and the Engineering Memory
  reconciliation. It covers the doctor backup enumeration.
- The child MUST inherit the parent security posture. A child never
  holds a wider permission set than the parent.

## Acceptance criteria

- [ ] A1. Given a daemon-backed parent session and the paced offline
  scripted model (`offline_pace`, host chat.rs:239), when the parent
  spawns three children in parallel with typed task prompts, then
  three admission handles return before any child completes, the
  registry shows three direct children, and each child records exactly
  one terminal outcome (R1, R2, R4).
- [ ] A2. Given a host restart while children run, when the daemon
  starts again, then a never-started child re-adopts and completes; an
  interrupted child settles to the `failed` terminal with the reason
  `crash_interrupted`; a child with a `cancel_requested` marker
  settles to `cancelled` with the reason `cancel_requested`; each
  child records exactly one terminal outcome; a crash-injection probe
  shows the in-flight effect exists exactly once after re-adoption,
  never twice (R2, R4, R6).
- [ ] A3. Given the default depth limit of 1, when a depth-1 child
  attempts a spawn, then the spawn fails with a diagnostic that names
  the limit and the depth (R3).
- [ ] A4. Given a depth limit raised to 2 (kernel config), when a
  depth-1 child with a running child of its own is cancelled, then
  the child turn loop stops, the descendants cancel, and the child
  records the `cancelled` terminal exactly once with the reason (R3,
  R6); given a non-terminal child with no live runner, when the
  parent cancels it, then the child settles to `cancelled` at the
  cancel call with the reason `runner_lost` (R6).
- [ ] A5. Given a terminal child, when the parent deletes it, then
  the registry row stays with the terminal outcome and the
  `deleted_at` marker, the child no longer lists as active, the
  terminal event count stays one, and the attribution rows remain;
  given a running child, when the parent deletes it, then the delete
  call waits for the `cancelled` terminal and writes the tombstone
  only after, and the child never re-adopts; given a non-terminal
  child whose daemon is dead, when the parent deletes it, then the
  delete settles the child to `cancelled` with the reason
  `runner_lost` and writes the tombstone, and re-adoption never
  re-runs the child (R2, R6, R7, R8).
- [ ] A6. Given a completed child, when the parent context tree is
  queried, then the tree shows the child with the status and the
  attributed usage, and the attribution totals reconcile with the
  child execution manifests (R7).
- [ ] A7. Given a spawn with no explicit selection, when the child
  runs, then the child uses the parent provider and policy; given an
  explicit selection, then the selection wins; given a permission
  override above the parent ceiling, then the spawn refuses (R5).
- [ ] A8. Given a parent client that detaches while children run, when
  the children complete in the daemon, then the registry and the
  events show the terminal outcomes, observable after reconnect; given
  a surface turn on a child session, when it opens the session for a
  chat, then the daemon refuses with a diagnostic that names the child
  session (R4, R8).
- [ ] A9. Given an embedded (non-daemon) kernel, when it calls
  `session_spawn`, then it receives the daemon-required diagnostic,
  never a silent in-process spawn (R4).
- [ ] A10. Given the full implementation, when `just check` and
  `just verify` run, then the new suites
  `crates/optimus-workflow/tests/recursion.rs` and
  `crates/optimus-workflow/tests/attribution.rs` pass, the
  cancellation suite extends with the descendant-cancel case, the
  child events are ordered and unique, the doctor backup enumeration
  includes `sessions.db` and `execution.db`, and the spec-019
  ceremony artifacts are present: the `ToolInvocation` variants, the
  catalog entries, the Engineering Memory reconciliation, and the
  module ratchet (R8, R9).

## Out of scope

- Process-level isolation of children. In v1, children run in the
  daemon process on worker threads. The roadmap "process tree" wording
  maps to the child execution tree: the turn loop, the descendants,
  and the runtime effects.
- Cross-machine child relay. The spec-017 transports may carry it
  later.
- Live child progress in the parent transcript. In v1, the parent sees
  the terminal outcome and the lifecycle events. Live streaming is a
  later add.
- The campaign DAG surface. The `WorkflowRunStore` children (spec-005
  R3) and the specialist DAG executor (Current state) stay the
  deterministic workflow surface. This spec adds the session
  hierarchy.

## Open questions

- Whether a child session accepts a goal (spec-026) at spawn. Default:
  no in v1. The typed task prompt and the step budget cover the task
  bounds.

## Links

- `crates/optimus-workflow/src/child_lease.rs` and
  `crates/optimus-workflow/src/orchestrator_envelopes.rs` — the seeds
  this spec activates.
- `crates/optimus-agent/src/lib.rs` — the typed contracts and the
  permission ceiling.
- `crates/optimus-workflow/src/specialist_vertical.rs` — the existing
  DAG executor (roadmap point 1 disposition).
- `crates/optimus-host/src/chat.rs:408` — the cancellable turn loop
  the child execution reuses.
- `crates/optimus-host/src/dispatch.rs:188, :276-289` — the daemon
  worker pool and the exactly-once terminal send.
- `apps/optimus-cli/src/main.rs:1332-1335` — the serve health guard,
  the single-runner claim.
- `crates/optimus-kernel/src/session.rs` — the durable session store
  the registry attaches to.
- `crates/optimus-kernel/src/execution.rs` and
  `crates/optimus-kernel/src/execution_schema.rs` — the execution
  store the attribution attaches to.
- `specs/025-session-messaging/spec.md` — the durable-store and tool
  ceremony pattern this spec follows.
- `specs/026-goals/spec.md` and
  `docs/decisions/0086-goals-are-session-scoped-budget-enforced-objectives.md`
  — the session-store precedent and the store re-homing rule.
- `docs/decisions/0087-the-session-message-plane-is-a-durable-ops-store.md`
  — the exactly-one-terminal and ordered-events laws.