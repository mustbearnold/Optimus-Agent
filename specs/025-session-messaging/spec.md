---
doc_id: spec-025-session-messaging
doc_type: reference
plane: work
status: current
authority: canonical
summary: Session-to-session messaging for Optimus — a durable session-message plane where any session can message any other session, with peer discovery, per-session inbound policy (auto-accept / hold-for-approval with expiry / deny), permission-classifier evaluation of inbound requests before the receiver acts, idempotent at-least-once delivery, and failure-honest receipts — the capability class Claude Code added as cross-session SendMessage + ListAgents, planned natively.
reviewed_on: 2026-08-08
review_by: 2026-11-08
knowledge_type: specification
covers:
  - crates/optimus-workflow/src/child_lease.rs
  - crates/optimus-workflow/src/orchestrator_envelopes.rs
  - crates/optimus-kernel/src/session.rs
  - crates/optimus-ops/src/gateway.rs
depends_on:
  - docs/decisions/0070-an-outbound-send-is-a-durable-obligation.md
  - docs/decisions/0071-a-routing-address-is-not-a-session-identity.md
  - specs/017-gateway-breadth/spec.md
---

# Spec-025: Session-to-session messaging — the message plane

Status: current
Owner: optimus-agent-development (prompt-only owner)

## Revision table

| Round | Verdict | Findings | Fixes |
|---|---|---|---|
| 1 | REJECTED | B1: permission-classifier precedent cited as "(Confirmed: ADR + scorecard)" — no in-repo ADR/scorecard mentions it; 5 nits (policy outcomes folded into session_send_failed, stacked (MAY)(MUST), A4 double-then, SmartDeny self-approval wording, unresolvable S7.3–S7.5) | B1: re-cited to classify_command (command_class.rs:122) + ADR-0081:80; nits applied: policy/transport split in R4, (MAY)(MUST) split, A4 single G/W/T, local-operator SmartDeny spine (ADR-0071:97-99), S7.3–S7.5 source-faithful note (round 2) |
| 2 | REJECTED | B1: R4 transport-failure MUST (session_send_failed, never success) uncriteria'd; B2: R6 threading/reply MUSTs uncriteria'd; 5 nits (message_too_large, machine-id row assertion, backup_relative_paths overstatement, ListAgents definitional MUST, policy persistence) | A3 transport branch, A9 (correlation + thread view), A10 (reply_wait_expired) added; A1 machine-id + over-cap; A2 policy persistence; backup_relative_paths() reworded; ListAgents tag dropped (round 3) |
| 3 | REJECTED | B1: durability-inventory MUST uncriteria'd (omission undetectable); B2: exactly-one-terminal-outcome MUST uncriteria'd (delivered+failed risk); 3 nits (opt-out default, classification-state visibility, by-session query) | A8 covers backup-list inclusion + by-session query; A11 asserts exactly-one terminal outcome across retry/failure; A5 asserts seen classification state; A7 fresh-session default (round 4) |
| 4 | REJECTED | B1: R7 "ordered" event-row attribute uncriteria'd (scrambled order passes; spec-017 R10/A4 mirrors it); 2 nits (A7 listing fields, dialogExpiry default) | A8 asserts state-machine order (queued→terminal) mirroring spec-017 R10/A4; A7 asserts id/title/status/last-activity; A4 asserts 30-min default (round 5) |
| 5 | APPROVED | 1 non-blocking nit (spec-019 ceremony pins not textually named in any criterion) | Applied 2026-08-08 (A8 names ceremony artifacts: ToolInvocation variant, catalog entry, EM reconciliation, module ratchet) |

## Purpose

Optimus sessions are isolated: a session's only inter-agent contact is
the DAG's parent→child lease (`crates/optimus-workflow/src/child_lease.rs`,
orchestrator envelopes) — there is no way for one session to message
another session directly. Claude Code recently shipped exactly this
capability class: cross-session `SendMessage` (sessions message each
other on any of your machines) with `ListAgents` discovery,
`crossSessionInbound` + `dialogExpiry` settings (messages to sessions
running with bypassed permissions are held for approval; others
auto-deliver), permission-classifier evaluation before dispatch, and
the hard-won lesson that a failed inbox write must be reported as an
error, never as "Message sent".

This spec plans the same capability natively for Optimus as the
**message plane**: a durable session-message store where any session
can message any other session (live or dormant), with peer discovery,
per-session inbound policy (auto-accept / hold-for-approval with
expiry / deny), permission-classifier evaluation of inbound requests
before the receiving agent acts, idempotent at-least-once delivery on
the existing claim-lease pattern, and failure-honest receipts. Same
host in v1; the envelope format carries a machine id so cross-machine
relay (via spec-017 transports) is a later add, not a redesign.

## Current state (Confirmed behaviour)

- Sessions are durable: `SessionStore::open/create/list` in
  `crates/optimus-kernel/src/session.rs`; durable session reopen is a
  shipped parity slice (Confirmed: source + scorecard).
- Inter-agent messaging today is DAG-only and parent→child:
  `crates/optimus-workflow/src/child_lease.rs` (leased child-agent
  campaign steps — source-faithful numbering S7.3–S7.5 from
  child_lease.rs; the steps live in the workflow engine, not a spec
  section) + `orchestrator_envelopes.rs`; there is no
  session↔session message store, tool, or inbox (Confirmed: source).
- The gateway (spec-007/017) is a durable local delivery authority
  with claim leases, exactly-one-terminal-outcome, and failure-honest
  delivery semantics (ADR-0070; `crates/optimus-ops/src/gateway.rs`)
  (Confirmed: source + ADR).
- ADR-0071: a routing address is not a session identity — session
  ids and routing addresses are distinct planes (Confirmed: ADR).
- Permission/approval truthfulness: ADR-0081 (truthful approval
  resolution), SmartDeny (a defensive win), and the
  permission-classifier precedent — `classify_command` in
  `crates/optimus-policy/src/command_class.rs` (line 122) and
  ADR-0081:80 (Confirmed: source + ADR).

## Design lessons from Claude Code's cross-session messaging

1. Delivery honesty: Claude Code's `SendMessage` once printed
   "Message sent" when the inbox write had actually failed; it was
   fixed to report failed deliveries as errors. Optimus MUST build
   failure-honest receipts in from day one (R4).
2. Inbound control: `crossSessionInbound` + `dialogExpiry` — messages
   to a session running with bypassed permissions are held for
   approval, others auto-deliver; held messages expire. Optimus's
   per-session inbound policy is the native form (R3).
3. Discovery: `ListAgents` — sessions find peers. Optimus's discovery
   is opt-in (R2).
4. Auto-mode safety: messages are evaluated by the permission
   classifier before dispatch — the receiver's agent never sees an
   unvetted effect request (R5).
5. Long payloads: Claude Code truncates long summaries rather than
   failing sends. Optimus enforces a bounded message size with a
   named diagnostic, never a silent drop (R4).

## Requirements

### R1. The message plane

- A durable session-message store MUST exist (SQLite, same authority
  pattern as the gateway/ops stores) with one inbox per session:
  message id (uuid), from_session, to_session, kind, payload, reply-to
  (correlation), machine id, state (queued / delivered / held /
  approved / expired / refused / failed), and timestamps (MUST).
- Messages MUST be sendable to live OR dormant sessions (dormant =
  delivered on resume) (MUST).
- The envelope MUST carry a machine id from day one (MUST).
- Same-host delivery is v1; cross-machine relay is a later add via
  spec-017 transports (MAY).
- The message store MUST be covered by the doctor's durability
  inventory — add it to `backup_relative_paths()` per spec-018 R3
  (MUST).

### R2. Peer discovery

- A discovery surface MUST list peers: live + dormant sessions opted
  into discovery, with id, title, status, and last activity; sessions
  MUST be opted out by default (discoverability is opt-in per
  session) (MUST).
- Discovery MUST respect session privacy: an opted-out session MUST
  NOT appear in any peer listing (MUST).
- The discovery surface is the product form of Claude Code's
  `ListAgents` (definitional; the normative content is R2 above).

### R3. Per-session inbound policy

- Every session MUST have an inbound policy: `auto-accept` (messages
  land in the inbox and surface on the next turn) | `hold-approval`
  (messages are held until the receiving side approves; equivalent to
  Claude Code's `crossSessionInbound` hold) | `deny` (senders get a
  refused event) (MUST).
- A session running with bypassed/auto-approved permissions MUST be
  forced to `hold-approval` for inbound messages that request effects
  — the crossSessionInbound security rule, native (MUST).
- Held messages MUST expire after a per-session `dialogExpiry`
  duration (config, default 30 min): expiry produces the named event
  `message_held_expired` and the sender receives the expiry event
  (MUST).
- Inbound policy is a session attribute, persisted with the session
  (MUST).

### R4. Send, receipt, and failure honesty

- A `session_send` tool MUST enqueue a message to a target session's
  inbox and return a receipt (message id + state) (MUST).
- The tool MUST land through the spec-019 R2 ceremony in one commit:
  `ToolInvocation` variant, catalog entry, dispatch arm, coverage
  pins + a real dispatch test, EM reconciliation, module ratchet
  (MUST).
- Transport failure MUST be reported as an error with the named
  diagnostic `session_send_failed` (target session gone, store write
  failed) — NEVER a success receipt (MUST; the Claude Code lesson,
  and ADR-0070 durability).
- Policy outcomes are NOT transport failures: a `deny`-policy send
  returns the refused event (R3) and held-and-expired is emitted
  later as `message_held_expired` — never folded into
  `session_send_failed` (MUST).
- Messages MUST be bounded in size (config; default cap); an
  over-cap message MUST be refused with `message_too_large` before
  enqueue, never truncated silently — if truncation is ever added it
  MUST be explicit in the receipt (MUST).
- Delivery MUST be at-least-once with idempotent inbox semantics: a
  retried delivery of the same message id MUST NOT duplicate the
  inbox entry (MUST; claim-lease + dedupe on the gateway pattern).
- Exactly one terminal outcome per message MUST be recorded
  (delivered / refused / expired / failed) (MUST).

### R5. Permission classifier on inbound

- Before the receiving agent can act on a message, any effect request
  in the message MUST be evaluated by the permission classifier; a
  classified-risky request MUST require approval through the existing
  local-operator SmartDeny grant spine (ADR-0071:97-99) regardless of
  the receiving session's inbound policy (MUST; ADR-0081 truthfulness,
  auto-mode safety).
- The classification result MUST be recorded in the observability
  plane with the message (MUST).
- The receiving agent MUST see the message with its classification
  state (approved / denied / pending), never a bare unvetted request
  (MUST).

### R6. Threading and reply

- Replies MUST carry the correlation id so a message thread forms;
  the store MUST expose a thread view (MUST).
- A session MAY await a reply with a bounded wait (async handoff);
  the bounded wait MUST expire with a named event
  `reply_wait_expired`, never hang (MUST).

### R7. Observability

- Every message transition (queued, delivered, held, approved,
  expired, refused, failed) MUST emit an ordered, durable event row
  (MUST; law 11).
- Message-plane events MUST be queryable by session and by message id
  (MUST).

## Acceptance criteria

- [ ] A1. Given two live sessions A and B (B auto-accept), when A
  sends via `session_send`, then B's next turn surfaces the message,
  the receipt reports delivered, and the store shows one delivered
  row carrying the machine id; given an over-cap message, then
  `message_too_large` is returned before enqueue, never a truncated
  send (R1, R4).
- [ ] A2. Given a dormant session, when a message is sent to it, then
  it is queued and surfaces on resume; given a session whose inbound
  policy is `hold-approval`, when it is reopened, then the policy is
  still `hold-approval` (persisted with the session) (R1, R3).
- [ ] A3. Given B with `deny` policy, when A sends, then A's receipt
  is a refused event, never success (R3, R4); given a target session
  that is gone or a store write failure, when A sends, then A
  receives the `session_send_failed` error, never a success receipt
  (R4).
- [ ] A4. Given B running with bypassed permissions and a message
  requesting an effect, when the message arrives and is approved,
  then it is delivered and recorded; without approval by
  `dialogExpiry`, `message_held_expired` is recorded and A receives
  the expiry event; the expiry uses the configured per-session
  `dialogExpiry` duration with the documented default of 30 min (R3).
- [ ] A5. Given a message containing an effect request, when it
  reaches B, then the permission classifier result is recorded and a
  risky request requires SmartDeny approval before B's agent acts;
  the receiving agent sees the message with its classification
  state (approved / denied / pending), never a bare unvetted request
  (R5).
- [ ] A6. Given a retried delivery of the same message id, when it
  lands, then the inbox holds exactly one copy (idempotent) (R4).
- [ ] A7. Given peer discovery, when a session is opted in, then it
  appears in listings with id, title, status, and last activity; when
  opted out, then it never appears; a FRESH session is opted out by
  default and does not appear until opted in (R2).
- [ ] A8. Given the full implementation, when `just verify` runs,
  then the message-plane suite passes with zero skips and every
  transition is observable; `doctor backup-list` / the
  `backup_relative_paths()` enumeration includes the message store,
  events are queryable by session AND by message id, and the events
  for a message return in state-machine order (queued → delivered →
  terminal), mirroring spec-017 R10/A4's ordered-event assertion
  (R1, R7); the `session_send` tool's spec-019 ceremony artifacts are
  present — `ToolInvocation` variant, catalog entry, EM
  reconciliation, module ratchet (R4).
- [ ] A9. Given a reply sent from B to A's message, when the thread
  view is queried, then the reply carries the correlation id and the
  store exposes the thread (R6).
- [ ] A10. Given A awaiting a reply with a bounded wait, when the
  reply does not arrive within the bound, then the wait expires with
  `reply_wait_expired` and does not hang (R6).
- [ ] A11. Given a message whose delivery is retried and then fails
  permanently, when the terminal state is recorded, then the message
  records EXACTLY ONE terminal outcome (delivered / refused /
  expired / failed) — never two (delivered+failed) — across the
  retry/failure paths (R4).

## Out of scope

- Cross-machine relay (envelope carries machine id; relay via
  spec-017 transports is a later add).
- Group messaging / chat rooms (point-to-point only).
- Human-in-the-loop chat UIs for the message plane (the dashboard,
  spec-021, MAY render it later).
- Changes to the DAG's parent→child lease semantics (the message
  plane is orthogonal; the lease stays the execution coupling).

## Open questions

- Whether `session_send` lives in the Core pack or a new
  `collaboration` pack — default: Core (cross-session messaging is a
  waist-level capability, like the memory tools).
- Default inbound policy for new sessions — default: `hold-approval`
  (secure by default; auto-accept is explicit opt-in).
- Whether a session should be able to revoke a sent message before
  delivery (recall) — default: no in v1; the refusal/expiry events
  cover the failure cases.

## Links

- `crates/optimus-workflow/src/child_lease.rs` +
  `crates/optimus-workflow/src/orchestrator_envelopes.rs` — the DAG
  messaging this generalizes.
- `crates/optimus-kernel/src/session.rs` — the durable session store
  the inboxes attach to.
- `crates/optimus-ops/src/gateway.rs` — the claim-lease + idempotent
  delivery pattern R4/R6 reuse.
- `docs/decisions/0070-an-outbound-send-is-a-durable-obligation.md`,
  `docs/decisions/0071-a-routing-address-is-not-a-session-identity.md`,
  `docs/decisions/0081-truthful-approval-resolution-and-session-consent.md`
  — durability, routing-plane, and approval truthfulness laws.
- `specs/017-gateway-breadth/spec.md` — the delivery-failure contract
  (R8) this spec mirrors for the session plane.
- `specs/018-deployment-ops/spec.md` — R3 backup set must include the
  message store (R1).
- Claude Code cross-session messaging (changelog, 2026): the
  feature class + bug lessons this spec plans natively.
