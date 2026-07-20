---
knowledge_type: specification
status: historical
covers:
  - crates/optimus-kernel/src/lib.rs
  - apps/optimus-desktop/src/ipc/chat.rs
  - apps/optimus-desktop/src/native_workers.rs
  - apps/optimus-desktop/src/main.rs
  - apps/optimus-desktop/src/bridge.rs
  - apps/optimus-desktop/ui/app.js
  - apps/optimus-desktop/ui/index.html
  - apps/optimus-desktop/ui/style.css
depends_on:
  - docs/specifications/stream-delivery-cancellation.md
  - docs/contracts/high-risk-contracts.md
  - docs/decisions/0021-owned-execution-and-causal-delivery.md
validated_by:
  - crates/optimus-kernel/tests/kernel_turn.rs
  - apps/optimus-desktop/src/native_workers.rs
  - apps/optimus-desktop/e2e/**
last_verified_commit: 28b0e96ac73ddedb8387478b63c8edaa0a385ac5
---

# Explicit desktop turn cancellation

- **Date:** 2026-07-20
- **Mode:** Standard, behavior-first TDD
- **Owner:** desktop stream bridge/UI and kernel cooperative cancellation seam

## Problem and outcome

The desktop can now cancel a turn when stream delivery is lost, but a user cannot
explicitly stop an active request. The bridge returns a bare promise and the Send
button ignores clicks while busy.

After this milestone, an active desktop stream exposes a local cancel handle and
the composer Send control becomes Stop while busy. HTTP aborts only its own fetch;
native mode requests cancellation for the exact active stream ID. The durable
session turn and execution manifest continue to use existing cancelled terminal
states.

## Repository truth

### Confirmed current behaviour

- The protected uncommitted candidate adds `StreamControl` and converts delivery
  loss into the existing cooperative token.
- Native stream requests already have monotonically assigned bridge IDs and pass
  through a bounded queue with two workers.
- HTTP streaming uses a browser `fetch` body reader; dropping it disconnects the
  bounded server event receiver and triggers stream-loss cancellation.
- The composer tracks one `state.busy` turn and blocks double-send.
- No desktop cancel IPC method, active-turn registry, cancel handle, or Stop UI
  exists.

### Reasonable inference

A process-local exact-ID registry plus cancellable bridge promises can add user
control without a persistent schema or remotely addressable cancel endpoint.

### Retained unresolved limitation

Cancellation is cooperative. Synchronous provider connect/write can settle only
at its existing timeout/read seam. A cancellation request racing an already
terminal turn may be accepted by the token registry while the authoritative
terminal result remains success; the stream terminal outcome, not the request
acknowledgement, is authoritative.

## Scope

- Add an additive kernel controlled-sink API that accepts a caller-owned token.
- Add a bounded native active-stream registry keyed by the existing request ID.
- Register before queue admission, roll back on admission failure, and unregister
  after terminal settlement.
- Handle native `chat_cancel` on the event-loop fast path so it cannot wait behind
  chat workers.
- Return bridge promises with a synchronous idempotent `cancel()` method.
- HTTP `cancel()` aborts its own fetch only; native `cancel()` sends its exact
  stream ID.
- Turn the existing Send control into Stop while busy and render an intentional
  cancelled state rather than a generic failure.
- Add focused race/mapping tests and one browser-level Stop contract.

## Non-scope

- Cancelling non-streaming `chat`, cron, campaigns, gateway work, or another
  client/process.
- A network cancellation endpoint, durable cancellation registry, reconnect,
  stream resume, or cancellation after process restart.
- Forced thread/process termination, provider transport replacement, or rollback
  of effects committed before cancellation.
- Multiple simultaneous composer turns, per-tool cancellation, or queue reprioritization.
- Trace, routing, evaluation, retrieval, or specialist-agent expansion.

## Contracts and invariants

1. Existing streaming and explicit-token APIs remain compatible; the new kernel
   seam is additive.
2. Native registration precedes queue admission. Failed admission removes only
   the same registration and cannot leave a cancellable ghost.
3. At most `CHAT_QUEUE_CAPACITY + CHAT_WORKERS` stream tokens are registered.
4. Duplicate live stream IDs fail closed without replacing the owner token.
5. Cancellation addresses exactly one active/pending native stream ID. Unknown or
   completed IDs return `requested: false` and mutate nothing.
6. Repeated cancellation is idempotent. `requested: true` means the cooperative
   token was signalled, not that cancellation won a terminal race.
7. Worker completion unregisters the exact stream before accepting ID reuse.
8. HTTP cancellation is capability-local: aborting a promise can affect only that
   request because no cancel-by-ID HTTP endpoint is introduced.
9. Bridge cancellation is synchronous and idempotent; promise settlement still
   occurs once through the existing terminal/error path.
10. Stop is visible and keyboard/click accessible only while one composer turn is
    active. Double-send remains blocked.
11. Intentional cancellation preserves partial assistant text, marks it
    non-streaming/cancelled, and does not display a generic red failure.
12. Existing session/execution one-terminal-outcome and effect-boundary contracts
    remain authoritative.

## State, compatibility, and mutation boundaries

- Native active tokens are process-local, mutex-protected, bounded by existing
  worker/queue limits, and discarded on process exit.
- No database migration, serialized durable record, provider wire change, new
  dependency, permission expansion, or credential access.
- Existing 19-path uncommitted stream-loss candidate is protected and extended,
  not reverted, staged, or committed.

## Failure, race, and recovery behaviour

- Queue full/disconnected: registration is removed and enqueue returns the existing
  bounded error.
- Cancel before worker pickup: the queued request receives an already-cancelled
  token and terminalizes through the normal kernel path.
- Cancel during provider/tool loop: existing cooperative checks fence later work.
- Cancel after unregister: no-op with `requested: false`.
- Cancel racing terminal settlement: the stream's persisted terminal outcome wins;
  no second terminal record is written.
- Native event-loop loss and HTTP disconnect continue to use the prior stream-loss
  fallback independently of explicit Stop.
- Process crash continues to use existing interrupted-turn recovery; this registry
  is not reconstructed.

## Execution ledger

### Slice 1 — Native exact-ID cancellation ownership

- **Outcome:** pending/active native stream IDs can be signalled without waiting
  behind chat workers and settle through the existing cancellation token.
- **Dependencies:** protected stream-loss candidate and bounded native worker pool.
- **RED:** registry tests require register/cancel/unregister, duplicate rejection,
  pre-pickup cancellation, admission rollback, and bounded ownership.
- **GREEN:** caller-owned-token kernel/chat seam plus mutex-protected registry and
  fast-path native cancellation.
- **Refactor:** centralize registration lifecycle in one small owner type.
- **Verify:** focused kernel and desktop native-worker tests.
- **Complete when:** exact-ID race cases pass and no chat worker is needed to signal
  cancellation.

### Slice 2 — Cancellable bridge handles

- **Outcome:** both native and HTTP `chatStream` calls return promises with one
  idempotent `cancel()` method.
- **Dependencies:** Slice 1 and existing authenticated bridge transport.
- **RED:** bridge-source contract test requires AbortController signal wiring,
  exact native `chat_cancel`, and one-shot cancellation.
- **GREEN:** attach cancel to the underlying non-`async` task promise; clean handler
  state on all terminal paths.
- **Refactor:** share bridge cancellation error classification.
- **Verify:** desktop Rust assembled-document tests and focused Playwright bridge
  behavior.
- **Complete when:** HTTP abort is local, native request ID is exact, and repeated
  cancel sends at most one signal.

### Slice 3 — Composer Stop behaviour

- **Outcome:** Send becomes Stop during a turn and intentional cancellation keeps
  partial content with a cancelled status.
- **Dependencies:** Slice 2 and single-turn `state.busy` ownership.
- **RED:** Playwright asserts Send→Stop→Send transition and non-error cancellation.
- **GREEN:** retain active cancel closure, route busy Send clicks to it, and
  distinguish intentional cancellation in UI settlement.
- **Refactor:** one busy-control renderer owns label/title/disabled state.
- **Verify:** focused Playwright case, existing shell/composer tests, and full 36+
  desktop suite.
- **Complete when:** mouse and Enter cannot double-send and Stop remains accessible.

### Final verification

Run focused tests, then once on final implementation bytes: workspace formatting,
all-target/all-feature strict Clippy, all-feature workspace tests, strict rustdoc,
desktop Playwright, Engineering Memory semantic/generate/strict/currentness, exact
indexed-tree verification, final diff review, and changed-path hygiene.

## Prohibited actions

Do not commit, stage, push, create/switch branches, open pull requests, deploy,
release, publish, install dependencies, access credentials, rewrite history, or
modify unrelated/protected work.

## Final disposition

Completed on the combined uncommitted candidate derived from
`28b0e96ac73ddedb8387478b63c8edaa0a385ac5`.

- Slice 1 RED failed only because active-stream ownership types were absent.
- Slice 1 GREEN passed five native-worker ownership/admission tests, all 18
  kernel-turn tests, and strict focused Clippy.
- Slice 2 RED failed only because cancellable bridge handles were absent.
- Slice 2 GREEN passed the bridge contract and all 21 desktop Rust tests.
- Slice 3 RED observed the busy control remaining disabled/labelled Send.
- Slice 3 GREEN passed actual HTTP one-shot abort and composer Stop/partial-text
  browser contracts.
- Final Rust gates passed: formatting, all-target/all-feature strict Clippy, 260
  tests, and strict rustdoc.
- Desktop Playwright passed 38/38; changed JavaScript files passed `node --check`.
- Engineering Memory semantic tests passed 12/12 before deterministic generation,
  strict validation, and currentness.
- No persistent schema, provider protocol, permission, dependency, credential,
  branch, remote, deployment, release, or publication mutation was introduced.
