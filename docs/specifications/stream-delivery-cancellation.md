---
knowledge_type: specification
status: historical
covers:
  - crates/optimus-kernel/src/lib.rs
  - crates/optimus-kernel/tests/kernel_turn.rs
  - crates/optimus-host/src/chat.rs
  - apps/optimus-desktop/src/native_workers.rs
  - apps/optimus-desktop/src/server.rs
depends_on:
  - docs/contracts/high-risk-contracts.md
  - docs/decisions/0021-owned-execution-and-causal-delivery.md
  - docs/maps/observability-and-evaluations.md
validated_by:
  - crates/optimus-kernel/tests/kernel_turn.rs
  - apps/optimus-desktop/src/server.rs
  - apps/optimus-desktop/src/native_workers.rs
  - apps/optimus-desktop/e2e/**
last_verified_commit: 28b0e96ac73ddedb8387478b63c8edaa0a385ac5
---

# Cancel turns when stream delivery is lost

- **Date:** 2026-07-20
- **Mode:** Standard, behavior-first TDD
- **Owner:** `optimus-kernel` cancellation contract and desktop stream adapters

## Problem and observable outcome

A desktop chat producer currently keeps running after its event receiver closes or
the bounded SSE event queue fills. The transport stops accepting events, but the
provider/tool turn continues without a consumer.

After this milestone, a failed native or HTTP stream-event delivery requests the
same cooperative cancellation already used by the kernel. The active session turn
and execution manifest settle as `cancelled`; no later tool begins after the
cancellation is observed.

## Repository truth

### Confirmed current behaviour

- `CancellationToken` reaches model completion and every model/tool loop boundary.
- Kernel cancellation settles both session turns and execution manifests once as
  `cancelled`.
- HTTP event delivery is non-blocking and fails on full or disconnected bounded
  channels.
- Native `EventLoopProxy::send_event` reports event-loop closure.
- Existing `turn_with_sink` callbacks cannot report delivery failure.

### Reasonable inference

A cancellation-aware sink adapter can close the defect without changing provider,
runtime, session, or execution schemas.

### Unresolved limitation retained

Synchronous `ureq` connection establishment/write cannot be force-aborted. The
cancellation request remains cooperative and may only settle after the provider's
existing timeout/read boundary.

## Scope

- Add one compatibility-preserving kernel streaming entry point whose sink returns
  `continue` or `cancel`.
- Convert `cancel` into the existing shared `CancellationToken`.
- Make desktop native and HTTP stream callbacks return `cancel` on delivery
  failure or bounded-queue backpressure.
- Prove durable cancelled terminal state and adapter mapping.
- Update current observability/architecture authority and generated Engineering
  Memory.

## Non-scope

- Client-initiated cancellation protocol or cancel button.
- Provider transport replacement or forced thread/process termination.
- Retrying dropped events, buffering beyond current bounds, reconnect/resume, or
  replaying a cancelled turn.
- Rollback or compensation for an effect already committed before delivery loss.
- CLI/gateway changes; they do not use the desktop streaming callback.
- Trace, evaluation, routing, GPU, retrieval, or specialist-agent expansion.

## Contracts and invariants

1. Existing `turn`, `turn_with_sink`, and explicit-token APIs retain source and
   behavioural compatibility.
2. A cancellation-aware sink can only continue or request cancellation; it cannot
   widen permissions or supply a replacement result.
3. Once a sink requests cancellation, the shared token remains cancelled.
4. The kernel checks cancellation through its existing provider and loop seams;
   no new detached worker is introduced.
5. Delivery failure before a tool dispatch prevents that tool from starting.
6. Session turn and execution manifest use existing one-terminal-outcome storage
   and settle `cancelled` with the existing cancellation error code.
7. Full and disconnected HTTP event channels both request cancellation; delivery
   remains non-blocking.
8. Native event-loop closure requests cancellation.
9. Terminal event delivery after the turn settles is best-effort and cannot change
   the already-authoritative terminal state.
10. An already-committed durable effect is not rolled back; cancellation fences
    later progression and remains truthful about the accepted transcript.

## Compatibility and mutation boundaries

- No SQLite schema, serialized record, provider wire format, or tool descriptor
  changes.
- The new kernel API is additive. Only internal desktop callback signatures change.
- No unbounded channel, wait, retry, or allocation is added.
- Existing user files, credentials, remotes, branches, and generated JSON are not
  manually modified.

## Failure and recovery

- Full/disconnected stream delivery requests cancellation once and suppresses
  further event delivery attempts.
- Provider cancellation latency remains bounded only by its existing cooperative
  seam and transport timeout.
- A process crash continues to use existing interrupted-turn recovery; this
  milestone adds no competing recovery state.
- Reopening the session observes the persisted cancelled terminal result; it does
  not resume the cancelled turn.

## Execution ledger

### Slice 1 — Kernel cancellation-aware stream delivery

- **Outcome:** a sink-declared cancellation reaches an active model and persists
  cancelled session/execution terminal states.
- **Dependencies:** existing `CancellationToken`, `run_recorded_turn`, and stores.
- **RED:** focused kernel test calls the new API with a model that emits once then
  waits for cancellation; compilation/behaviour fails before implementation.
- **GREEN:** additive stream-control enum/API adapts cancellation into the existing
  explicit-token turn path.
- **Refactor:** preserve old streaming methods as compatibility wrappers.
- **Verify:** focused test plus kernel turn regression suite.
- **Complete when:** model observes cancellation and both durable records are
  cancelled exactly once.

### Slice 2 — Desktop native and HTTP adapters

- **Outcome:** native event-loop closure and HTTP full/disconnected event delivery
  return cancellation control to the kernel.
- **Dependencies:** Slice 1 and existing non-blocking transport results.
- **RED:** adapter mapping tests require failure → cancel and success → continue.
- **GREEN:** `chat_turn` accepts the control-returning callback and calls the new
  kernel API; both transports map their real send results.
- **Refactor:** one shared boolean-to-control helper prevents divergent semantics.
- **Verify:** desktop unit tests, focused HTTP stream tests, and Playwright.
- **Complete when:** every desktop streaming caller handles delivery outcome and no
  ignored active-stream send result remains.

### Acceptance and final verification

- Focused RED/GREEN evidence exists for each slice.
- Kernel and desktop affected tests pass.
- Final canonical gates: workspace format, all-target/all-feature strict Clippy,
  all-feature workspace tests, strict rustdoc, desktop Playwright, Engineering
  Memory semantic/generate/strict/currentness.
- Final diff matches this scope and contains no unrelated/generated-manual edits.
- Candidate remains uncommitted and unpushed.

## Prohibited actions

Do not commit, push, create/switch branches, open pull requests, deploy, release,
publish, install dependencies, access credentials, rewrite history, or modify
unrelated work.

## Final disposition

Completed on the uncommitted candidate derived from
`28b0e96ac73ddedb8387478b63c8edaa0a385ac5`.

- Slice 1 RED: compilation failed only because `StreamControl` and
  `turn_with_controlled_sink` were absent.
- Slice 1 GREEN: the delivery-rejection regression and all 18 kernel-turn tests
  passed; session and execution stores each persisted cancellation.
- Slice 2 RED: desktop compilation failed only because the shared delivery-control
  adapter was absent.
- Slice 2 GREEN: the mapping regression and all 17 desktop Rust tests passed.
- Final Rust gates passed: formatting, all-target/all-feature strict Clippy, 256
  tests, and strict rustdoc.
- Desktop Playwright passed 36/36.
- Engineering Memory semantic tests passed 12/12, followed by deterministic
  generation, strict validation, and currentness.
- No schema, provider wire format, permission, dependency, credential, branch,
  remote, deployment, release, or publication mutation was introduced.
