---
doc_id: decisions-0088-recursive-children-are-durable-session-hierarchy
doc_type: decision
plane: decision
status: current
authority: record
summary: Recursive children (spec-034) form a durable session hierarchy — a parent kernel session spawns full child kernel sessions with one typed task prompt, receives an admission handle immediately, and tracks them in a session_children registry in sessions.db with a depth limit (default 1), daemon-backed execution in the host worker pool with re-adoption after restart, cancellation and durable tombstones, and usage attribution in execution.db reconciled against the child manifests; the tool surface is a new on-demand children pack with Capability-policy spawn.
reviewed_on: 2026-08-08
review_by: 2026-11-08
knowledge_type: decision
covers:
  - specs/034-recursive-children/spec.md
  - crates/optimus-workflow/src/children.rs
  - crates/optimus-kernel/src/session/children.rs
  - crates/optimus-host/src/children.rs
  - crates/optimus-packs/src/catalog.rs
depends_on:
  - docs/decisions/0086-goals-are-session-scoped-budget-enforced-objectives.md
  - docs/decisions/0087-the-session-message-plane-is-a-durable-ops-store.md
validated_by:
  - crates/optimus-workflow/tests/recursion.rs
  - crates/optimus-workflow/tests/attribution.rs
  - crates/optimus-runtime/tests/cancellation.rs
---

# ADR-0088: Recursive children form a durable session hierarchy

- **Status:** Accepted
- **Date:** 2026-08-08

## Context

Spec-034 (approved) requires recursive children for the Phase 1a.3
capability: a parent session spawns full child kernel sessions with one
typed task prompt, gets an admission handle at once, and tracks the
children in a durable registry. Children survive host restart, enforce
a depth limit (default 1), inherit or override provider and policy,
run daemon-backed, cancel with their descendants, delete with durable
tombstones, and attribute their usage to the parent turn.

Constraints from the existing architecture: sessions and their durable
state live in `sessions.db` (kernel-owned); the execution store
(`execution.db`, kernel-owned) records one manifest per turn with the
model-call usage; the host daemon (`optimus serve`) runs a worker pool
and a cancellable turn loop per chat stream; the Core pack has a hard
schema budget (ADR-0087); the agent crate holds typed contracts and a
permission ceiling (`AgentPermissions::is_subset_of`); ADR-0086
established the re-homing rule — roadmap store names resolve to the
real store layout at ADR time.

## Decision

1. **The child registry is a session-scoped table in `sessions.db`.**
   New table `session_children`: parent session id, child session id,
   depth, task prompt sha256, provider and model snapshot, status,
   creation, adoption, and terminal timestamps, plus a `deleted_at`
   marker (null until deletion). Statuses: `spawned`, `running`,
   `succeeded`, `failed`, `cancelled`. A child records exactly one
   terminal outcome; deletion never changes the terminal status.
   Session state belongs in the session store (ADR-0086); the effect
   ledger does not carry session-scoped records.
2. **Child lifecycle events are ordered and unique per type.** New
   table `session_child_events` follows the message-plane pattern:
   `spawned`, `adopted`, `running`, `succeeded`, `failed`, `cancelled`,
   `deleted`, one row per event type per child, terminal event types
   at most once. The `deleted` event is the tombstone record; it is
   not a second terminal outcome.
3. **The attribution is a table in `execution.db`.** New table
   `execution_child_attribution` links the parent manifest, the child
   session, and the child manifest (UNIQUE child manifest reference).
   The row snapshots the child usage: input, output, total, reasoning,
   cached tokens, duration. The roadmap "optimus-store" crate line
   resolves to the execution store, per the ADR-0086 re-homing rule.
4. **The orchestrator lives in `optimus-workflow` (new module
   `children.rs`): the supervisor state machine, admission, adoption,
   depth checks, tombstone rules. The execution glue lives in
   `optimus-host` (new module `children.rs`): the daemon owns the
   runner jobs, the cancellation tokens, and the re-adoption sweep.
   The kernel receives an optional `ChildrenHandle` in `KernelConfig`
   (the `SelfDevelopmentHandler` precedent); embedded kernels get
   `None` and refuse `session_spawn` with a daemon-required diagnostic.
5. **The child runs as a kernel turn loop in the daemon worker pool.**
   The child reuses `chat_turn_cancellable`: open the child session,
   route the inherited or selected provider, run the task prompt turn
   with a supervisor-owned `CancellationToken`. The parent client can
   detach; the daemon owns completion. On daemon start, the supervisor
   re-adopts non-terminal children. Adoption re-runs ONLY a child with
   no execution manifest (the turn never started). A child with an
   interrupted manifest settles to the `failed` terminal with the
   reason `crash_interrupted`; the turn loop is not resumable, and a
   re-run would apply its effects twice. The single-runner claim is
   the serve health guard: a second `serve` on a healthily served
   home exits 3 (apps/optimus-cli/src/main.rs:1332-1335). The
   exactly-one-terminal invariant holds across crashes.
6. **Cancellation and deletion.** `session_cancel_child` records a
   durable `cancel_requested` marker with the reason in the registry,
   then cancels the child token and the descendants recursively.
   Cancellation is durable: adoption settles a child with the marker
   to `cancelled` with the reason `cancel_requested` and never
   re-runs it; a non-terminal child with no live runner settles at
   the cancel call to `cancelled` with the reason `runner_lost`.
   Deletion writes a durable tombstone (the `deleted_at` marker) and
   never changes the terminal status. Deletion serializes with the
   run: a non-terminal child is cancelled first and the delete call
   waits for the `cancelled` terminal before the tombstone write.
   The wait is bounded. A non-terminal child with no live runner
   settles at the delete call to `cancelled` with the reason
   `runner_lost`. A runner that observes the tombstone stops and
   records nothing; adoption excludes terminal and tombstoned rows;
   a deleted row never returns active; attribution rows survive.
7. **The tool surface is a new on-demand `children` pack**, four tools:
   `session_spawn` (Capability policy class, session-local, no
   external effect, no approval — the ADR-0086 goal-tool precedent),
   `session_cancel_child`, `session_delete_child`, `session_children`.
   The Core waist cannot absorb more tools under the schema budget
   (ADR-0087 precedent). Child external effects stay SmartDeny-gated
   through the inherited effect policy.
8. **The daemon is the only runner of a child turn in v1.** A surface
   turn on a child session (CLI `--session`, desktop chat) refuses
   with a diagnostic. A child has at most one manifest: the task
   turn. The attribution row references that manifest, so
   reconciliation stays total across a crash.
9. **The child never outranks the parent.** Inherited or explicit
   provider and policy selections validate against the parent
   configuration; permission overrides validate with
   `AgentPermissions::is_subset_of` against the parent ceiling.

## Consequences

- The registry and attribution tables are additive CREATE TABLE IF
  NOT EXISTS migrations with a schema-version bump (law 12); old
  homes open unchanged. `sessions.db` and `execution.db` already sit
  in the doctor backup set.
- Children run in the daemon process on worker threads in v1. Process
  isolation is a later add; the roadmap "process tree" wording maps to
  the child execution tree (turn loop, descendants, effects).
- Embedded (CLI) kernels can list, cancel, and delete children but
  cannot spawn: the daemon-required diagnostic is honest about the
  survival guarantee.
- Attribution is reconcilable by construction: UNIQUE child manifest
  reference, snapshot totals equal the child manifest model-call
  sums, and a child has at most one manifest (re-run happens only
  before the first manifest begins), so reconciliation stays total
  across a crash.
