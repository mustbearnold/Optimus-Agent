---
doc_id: decisions-0087-the-session-message-plane-is-a-durable-ops-store
doc_type: decision
plane: decision
status: current
authority: record
summary: Session-to-session messaging (spec-025) is a durable SQLite store owned by optimus-ops (messages.db, gateway-authority pattern with idempotent message-id keys, ordered events, exactly-one-terminal), enforced by the kernel through five on-demand collaboration-pack tools; session attributes (inbound policy, discovery opt-in, dialog expiry) persist on the session row; delivery is live-aware (dormant targets stay queued until resume); risky payloads are permission-classified and effect execution stays SmartDeny-gated.
reviewed_on: 2026-08-08
review_by: 2026-11-08
knowledge_type: decision
covers:
  - specs/025-session-messaging/spec.md
  - crates/optimus-ops/src/message_plane.rs
  - crates/optimus-kernel/src/message_ops.rs
  - crates/optimus-kernel/src/session.rs
  - crates/optimus-packs/src/catalog.rs
depends_on:
  - docs/decisions/0070-an-outbound-send-is-a-durable-obligation.md
  - docs/decisions/0071-a-routing-address-is-not-a-session-identity.md
  - docs/decisions/0081-truthful-approval-resolution-and-session-consent.md
validated_by:
  - crates/optimus-kernel/tests/message_plane.rs
  - crates/optimus-kernel/tests/tool_coverage.rs
---

# ADR-0087: The session message plane is a durable ops store

- **Status:** Accepted
- **Date:** 2026-08-08

## Context

Spec-025 (approved) requires a session-to-session message plane: durable
messages between any two sessions, opt-in discovery, per-session inbound
policy (auto-accept / hold-approval with expiry / deny), permission
classification of effect requests, failure-honest receipts, idempotent
at-least-once delivery, exactly-one-terminal outcomes, and ordered
observability events. The roadmap (Phase 1a.2) adds Prime Agent delivery
modes (auto / steer / follow_up) and a family roster.

Constraints from the existing architecture: sessions and their durable
state live in `sessions.db` (kernel-owned); the gateway (optimus-ops)
owns the durable delivery-authority pattern with claim leases and
idempotency (ADR-0070); the Core pack has a hard schema budget (Core +
heaviest on-demand pair must fit the default 2800-token budget); apps may
not open core state directly (crate-layers); `classify_command` exists in
optimus-policy and `externality()` distinguishes project-local from
external effects.

## Decision

1. **The message plane is a new SQLite store in optimus-ops**
   (`messages.db`): `session_messages` (id primary key, from/to session,
   kind, payload, reply-to correlation, mode, machine id, state,
   classification, timestamps), `session_message_events` (ordered,
   unique per (message, event type)), `session_reply_waits`, and
   `live_sessions` (lease-style live registry with 1-hour recency).
   The message id is the idempotency key: re-enqueueing the same id is a
   no-op. Terminal transitions (expired/refused/failed) are guarded in
   code and every message records exactly one terminal outcome.
   `messages.db` joins the doctor backup set (spec-018 R3).
2. **Session attributes persist on the session row** (additive columns in
   `sessions.db`): `inbound_policy` (default `hold-approval`, secure by
   default), `discoverable` (default 0), `dialog_expiry_seconds`
   (default: the plane's 30-minute constant).
3. **Delivery is live-aware.** A kernel registers its session as live on
   open. `session_send` to a live auto-accept target delivers immediately
   (A1); a dormant auto-accept target stays queued and delivers on resume
   (A2). `deny` produces a refused event; anything else holds.
4. **The tool surface is one on-demand pack** (`collaboration`, five
   tools: `session_send`, `session_inbox`, `session_roster`,
   `session_review`, `session_policy`). The Core waist cannot absorb five
   more tools under the schema budget (the spec's open question defaulted
   to Core; the budget is the harder constraint). The model activates the
   pack like any other; surfacing of delivered messages is kernel-level
   and does not require the pack.
5. **Delivery modes** are a per-message attribute: `auto` and `steer`
   surface on the receiving session's next turn (injected once,
   `surfaced_at`); `follow_up` stays in the inbox for polling.
6. **Permission classification** (R5) runs at send time with
   `classify_command` on the first command-like line; non-project-local
   externality is `pending`, otherwise `approved`. The result is stored
   with the message and shown to the receiving agent. Executing any
   effect the message requests still goes through the normal SmartDeny
   path — the message plane never bypasses approval (ADR-0081).
7. **Hold-approval approval** is a durable decision via `session_review`
   (approve -> held -> approved -> delivered; deny -> refused). Held
   messages expire after the dialog expiry at inbox polls and turn starts
   (`message_held_expired` equivalent: the `expired` event; the sender
   observes it on the same record).
8. **Bounded reply waits** (R6, A10) poll with a deadline and record
   `reply_wait_expired`; they never hang and honor cancellation.
9. **Failures are named diagnostics, never success receipts**: missing
   targets and store failures are `session_send_failed` errors; over-cap
   payloads are `message_too_large`; policy outcomes (refused, expired)
   are events, never transport errors (the Claude Code lesson).

## Alternatives considered

- **Messages in `sessions.db`.** Rejected: the plane is a cross-session
  delivery authority like the gateway, and the kernel's session store is
  per-session-shaped; a dedicated ops-owned store keeps doctor/backup and
  lifecycle separate (ADR-0070 pattern).
- **Messages in the gateway store.** Rejected: the gateway is
  platform-bound (channels); session-to-session delivery has different
  states (held/approved) and would pollute the platform ledger.
- **Tools in Core.** Rejected by the schema budget: Core + heaviest pair
  must fit 2800 tokens; five message tools (~320 tokens) overflow. The
  collaboration pack also matches the Hermes lesson (waist small, edges
  on demand).
- **Durable live registry with leases vs. send-time heuristics.**
  Adopted the registry: A1 needs immediate delivery to live sessions,
  A2 needs queued-for-dormant; a recency-based lease needs no daemon.
- **Separate tools per action vs. one action-enum tool.** Five distinct
  tools: each has a narrow schema; the pack is on-demand so the waist is
  unaffected.
- **Expiry via daemon vs. lazy.** Lazy expiry at inbox polls and turn
  starts: same observable outcome, no new background process.

## Reasons

- The gateway pattern (ADR-0070) already solves idempotent durable
  delivery; reusing its shape keeps the semantics familiar to operators
  and covered by the same backup/doctor conventions.
- Persisting policy/discovery/expiry on the session row makes them
  survive restart by construction (A2) and keeps the plane store free of
  per-session settings.
- The collaboration pack keeps the Core waist within its pinned budget
  and makes messaging a capability the model loads deliberately.
- Classification at send time with a recorded outcome satisfies R5
  (the receiver sees the classification; effects remain SmartDeny-gated)
  without inventing a second approval spine.

## Consequences

- `messages.db` is a new store: doctor lists it, backup includes it,
  the durability inventory must cover it (A8).
- Sessions gain three additive columns; existing databases migrate on
  open (idempotent ALTER TABLE).
- The kernel opens one more store per session (SQLite WAL; negligible).
- `activate_pack` gains `collaboration` in its enum.
- Tool coverage ledger, EM reconciliation, and the module ratchet move
  with the ceremony (spec-019 R2).

## Risks

- **Live-registry staleness.** A closed kernel leaves a live row; the
  1-hour recency lease treats it as dormant afterwards, so sends to a
  just-closed session may deliver immediately instead of queueing.
  Mitigation: delivery to a dormant session is harmless (it surfaces on
  resume); the recency bound keeps the window small. A later daemon can
  unregister cleanly.
- **Classification is a heuristic.** `classify_command` on the first
  command-like line can misjudge prose or miss embedded requests.
  Mitigation: the result is recorded and visible (never hidden), and
  effect execution is SmartDeny-gated regardless of classification —
  classification informs the agent, it does not authorize.
- **Expiry races.** Two processes expiring the same held message: the
  terminal guard (exactly-one-terminal) makes the loser a no-op error,
  never a double outcome.
- **Held-message pile-up.** An unattended hold-approval session can
  accumulate held messages until expiry. Mitigation: expiry is lazy but
  bounded by the dialog expiry; the inbox tool reports states.

## Evaluation evidence

- `crates/optimus-kernel/tests/message_plane.rs` — A1 live delivery +
  single surfacing, A2 dormant queue + resume, A3 deny + gone-target
  failure, A4 hold + expiry order, A5 classification, A6 idempotency,
  A7 opt-in discovery, A8 backup set + ordered events, A9 threads,
  A10 bounded wait, A11 exactly-one-terminal.
- `crates/optimus-kernel/tests/tool_coverage.rs` — the five tools
  dispatch through real turns; `activation_enum_pins` covers the new pack.
- `crates/optimus-ops` store tests; packs schema-budget tests.

## Conditions for reconsideration

- If the Core schema budget grows, the collaboration tools may move into
  the waist; the spec's open question then resolves to Core.
- If a daemon owns kernel lifecycle, replace the recency lease with
  explicit register/unregister.
- If cross-machine relay lands (spec-017), the machine id on the
  envelope is the routing key; the store schema does not change.

## Relevant code

- `crates/optimus-ops/src/message_plane.rs` — store, states, events.
- `crates/optimus-kernel/src/message_ops.rs` — tools, policy, surfacing.
- `crates/optimus-kernel/src/session.rs` — policy/discovery columns.
- `crates/optimus-packs/src/catalog.rs` — collaboration pack.
- `apps/optimus-cli/src/doctor.rs` — backup set.

## Relevant tests

- `crates/optimus-kernel/tests/message_plane.rs` (A1-A11).
- `crates/optimus-kernel/tests/tool_coverage.rs`.
- `crates/optimus-packs/tests/packs_budget.rs`.
