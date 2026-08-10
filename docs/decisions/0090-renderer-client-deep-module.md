---
doc_id: decisions-0090-renderer-client-deep-module
doc_type: decision
plane: decision
status: current
authority: record
summary: The renderer's flat 92-method `OptimusTransport` call surface is replaced by one typed deep module (`apps/optimus-ui/src/ipc/client/**`) — `createOptimusClient(transport)` exposing 19 typed domain objects plus a session-scoped `ChatSession` whose turns settle exactly once with a classified outcome, plus an ordered runtime observer. The frozen wire (spec-015) is untouched: transports, broker selection, and their pinned tests keep passing as-is; `conversationStore.apply` keeps consuming `StreamEvent` verbatim.
reviewed_on: 2026-08-10
review_by: 2026-11-10
knowledge_type: decision
covers:
  - apps/optimus-ui/src/ipc/client/**
depends_on:
  - docs/decisions/0038-ui-ipc-architecture.md
  - docs/decisions/0083-one-wire-protocol-for-all-surfaces.md
---

# ADR-0090: Renderer client as a deep module over the frozen wire surface

## Status

Current.

## Context

The renderer reaches the host through `OptimusTransport` (`contracts.ts`):
a flat 92-member `DesktopMethod` union behind one generic `invoke<T>`,
plus two streaming entry points (`chat`, `chatApprovalResolve`). Every
caller re-learns wire names, snake_case params, result envelopes
(`{pending?: …}` / `{jobs?: …}`), error flattening, and stream terminal
rules per call site: ~97 call sites across 14 production files, and the
chat flow is the only surface with a state machine (`RunStatus`), a
terminal contract (spec-014 R4/R5/R9), and a control plane
(`chat_cancel`).

This is a shallow-module shape: the interface is nearly as complex as the
sum of its callers' knowledge. Depth is concentrated in the stream
protocol (handshake, terminal synthesis, cancel round trip, R4/R9 pins),
but that knowledge is currently re-derived at every chat call site and in
`conversationStore`'s terminal branching.

The wire protocol is frozen (spec-015, ADR-0083/0084): transports, broker
selection, and their pinned tests (`ipc/*.test.ts`) must not change.

## Decision

Add `apps/optimus-ui/src/ipc/client/**`, a typed deep module that is the
renderer's single consumer of `OptimusTransport`:

- `createOptimusClient(transport | null)` returns one `OptimusClient`
  with 19 typed domain objects (`sessions`, `approvals`, `cron`,
  `artifacts`, `jobs`, `gateway`, `settings`, `fs`, `memory`, `skills`,
  `packs`, `campaigns`, `providers`, `consents`, `projects`, `system`,
  `shell`, optional `browser`, and `chat`).
- `chat(sessionId)` returns a pre-bound `ChatSession`: `send` /
  `approve` / `cancel` each start a `Turn` whose `outcome` settles exactly
  once, classified as `completed | failed | cancelled |
  awaiting-approval | disconnected`. R4 folding (`resume_error` →
  `failed`, `still_pending` → `awaiting-approval`), R7 grant-before-
  resolve ordering, and R9 interpretation (structured `IpcError.code`
  first — `connection_lost` | `closed_unexpectedly`, #147 — with the
  text-sniff `connection lost|closed unexpectedly` as the documented
  fallback for message-only transports, kept in one documented place)
  live inside the module.
- A minimal ordered `RuntimeObserver` logs every call and stream event in
  arrival order (law 11).
- Writes return fresh projections where provably free (execution
  approve/term-run → approvals+jobs; cron writes → job list), removing
  the `await mutate; await load()` pattern; skipped where it would
  over-fetch.
- Errors are typed: `IpcError` (message preserved), `NoTransportError`
  (constructed with `null` — the packaged broker-absence terminal
  affordance), `TurnInFlightError` (one live send-turn per session).

Non-goals (explicit): no wire change, no transport change, no
`conversationStore.apply` change (`StreamEvent`/`RunStatus` delivered
verbatim), no registry/proxy machinery (deferred until a renderer plugin
domain exists), no structured error codes on the wire (a future additive
transport change may attach `cause.code` without breaking pinned tests).

## Consequences

- Callers learn typed domain objects instead of the 92-name union;
  snake_case, envelopes, and `|| []` defaults live in one place per
  domain.
- The stream lifecycle (one terminal, R9 layering, cancel idempotency,
  start-failure vs stream-failure) is enforced once, at the module seam
  (law 10), and testable in isolation against a fake `OptimusTransport`.
- `ipc/*.test.ts` and all four transports remain untouched; new client
  seam tests are additive.
- The one-adapter rule is satisfied: the seam already has four adapters.
- Migration is batched by domain on `main`, each batch a small verified
  commit (issue #146).
