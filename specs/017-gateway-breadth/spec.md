---
doc_id: spec-017-gateway-breadth
doc_type: reference
plane: work
status: current
authority: canonical
summary: Live multi-transport messaging for Optimus — a typed transport-adapter contract, live Discord and Slack adapters plus Email (WhatsApp/Signal deferred by ADR), mock-first wire conformance for every adapter, single-process multi-adapter supervision on the existing claim-lease engine, inbound authorization with named refusal diagnostics, and delivery failure reported as an error rather than silent success.
reviewed_on: 2026-08-08
review_by: 2026-11-08
knowledge_type: specification
covers:
  - crates/optimus-ops/src/gateway.rs
  - apps/optimus-cli/src/telegram_cmd.rs
  - apps/optimus-cli/src/gateway_http.rs
  - docs/architecture/parity-capability-ledger.json
depends_on:
  - docs/decisions/0070-an-outbound-send-is-a-durable-obligation.md
  - docs/decisions/0071-a-routing-address-is-not-a-session-identity.md
---

# Spec-017: Gateway breadth — live multi-transport messaging

Status: current
Owner: optimus-agent-development (prompt-only owner)

## Revision table

| Round | Verdict | Findings | Fixes |
|---|---|---|---|
| 1 | APPROVED | 5 non-blocking nits (RFC-2119 wording, task-vs-subprocess, Email A7, spec-021 ref) | All 5 applied 2026-08-08 (MUST NOT wording, in-process task clause, A7 added, unnumbered dashboard ref) |

## Purpose

Optimus ships exactly one live messaging transport: the Telegram
long-poll adapter (`apps/optimus-cli/src/telegram_cmd.rs`). Discord and
Slack exist only as mock enqueue adapters, and WhatsApp, Signal, and
Email have no adapter at all. The SOTA scorecard records "Live
Discord/Slack bot transports (mock enqueue only)" as a leading product
loss, and the parity ledger's `gateway.discord-slack` row is mock-only.
Hermes, the parity target, runs live Telegram, Discord, Slack,
WhatsApp, Signal, and Email surfaces.

This spec closes the transport gap with a mechanism, not a pile of
adapter code: a typed transport-adapter contract, mock-first wire
conformance for every adapter (so CI proves the wire shape without a
live network), single-process supervision of N adapters on the existing
claim-lease engine, and inbound authorization so a bot token exposed in
one chat cannot drive the agent from any chat.

The durable-send and routing foundations already exist as ratified
ADRs — ADR-0070 (an outbound send is a durable obligation) and
ADR-0071 (a routing address is not a session identity) — so this spec
extends the existing queue, not a new one.

## Current state (Confirmed behaviour)

- The gateway is a SQLite-backed claim-lease engine in
  `crates/optimus-ops/src/gateway.rs` (≈1,090 lines): `enqueue`,
  `claim_one`, `renew_claim`, `release_claim`, `complete_claim`,
  `fail_claim`, `cancel_claim`, `reconcile`, and `drain_one` implement
  a durable outbox/inbox with per-message claims, leases, and exactly
  one terminal outcome per message (Confirmed: code + ledger row
  `gateway.queue`).
- One live adapter exists: Telegram long-poll in
  `apps/optimus-cli/src/telegram_cmd.rs`, run via the CLI gateway
  surface (`gateway telegram run`; `gateway_http.rs` serves the status
  HTTP surface). Telegram also has a mock adapter (Confirmed: ledger
  rows `gateway.telegram` and `gateway.ui`).
- Discord and Slack transports are mock enqueue only — a message can be
  enqueued into the outbox as if it came from those transports, but no
  process connects to Discord's or Slack's APIs (Confirmed: ledger row
  `gateway.discord-slack`; scorecard "leading product losses" #4).
- There is no adapter trait: the Telegram adapter is a standalone
  command, so a second transport would have to copy its loop and drift
  from it (Inferred from repo shape: no `TransportAdapter`-style
  abstraction exists in `crates/optimus-ops/src/gateway.rs`).
- Inbound messages carry a `from` routing address; outbound sends are
  durable obligations per ADR-0070, and ADR-0071 establishes that a
  routing address is not a session identity (Confirmed: ADRs).

## Requirements

### R1. Transport-adapter contract

- A `TransportAdapter` contract MUST define, in one place: a stable
  transport identifier, inbound message conversion to the gateway's
  canonical `InboundMessage`, outbound delivery from the gateway's
  canonical `OutboundMessage`, auth/session lifecycle, a health probe,
  and a `TerminalOutcome` per send (delivered / failed-permanently /
  failed-transiently with retry policy) (MUST; RFC 2119).
- Every adapter MUST be runnable both as a standalone command
  (`optimus gateway <transport> run`) and under the supervisor
  (R7) (MUST).
- The contract MUST be the single implementation path for ALL
  transports including the existing Telegram one: Telegram MUST be
  refactored onto the contract, and the refactor MUST preserve the
  existing Telegram conformance (mock + live smoke) with zero
  behaviour change (MUST).
- Transport-specific types MUST NOT leak into `crates/optimus-ops`
  core outside the adapter module owning them (MUST).

### R2. Live Discord adapter

- `optimus gateway discord run` MUST connect a Discord bot via the
  gateway websocket API using a bot token from the Optimus home
  config (MUST).
- The adapter MUST receive direct messages and, when configured, server
  messages from an allowlisted set of channels, converting them to
  canonical inbound messages carrying the routing address (MUST).
- The adapter MUST send replies back to the originating chat, and MUST
  report delivery outcome per R1's terminal-outcome contract (MUST).
- Discord-specific markup (embeds, mentions, attachments) MUST be
  reduced to plain text + attachment paths on inbound, and plain text
  on outbound (no embed builder in v1) (MUST).

### R3. Live Slack adapter

- `optimus gateway slack run` MUST connect via Slack Socket Mode using
  an app-level token (no inbound ports; the supervisor owns the
  websocket lifecycle) (MUST).
- The adapter MUST receive messages from configured channels/DMs within
  the allowlist and send replies to the originating conversation
  (MUST).
- Slack message blocks/attachments MUST be reduced to plain text +
  attachment paths on inbound (MUST).

### R4. Email adapter

- `optimus gateway email run` MUST poll an IMAP inbox on a configured
  cadence, convert unseen messages from the allowlisted senders into
  canonical inbound messages, and send replies via a configured SMTP
  relay (MUST).
- Email threading MUST use `In-Reply-To`/`References` headers so a
  reply continues the originating thread (MUST).
- Poll cadence, IMAP folder, SMTP relay, and allowlisted senders MUST
  all be config, not code (MUST).
- The adapter MUST NOT deliver attachments inline; attachments are
  written to the Optimus artifact store and referenced by path (MUST).

### R5. WhatsApp and Signal adapters

- WhatsApp and Signal transports are DEFERRED to a follow-up ADR
  decision on transport choice (WhatsApp Cloud API vs libsignal-based;
  Signal via signal-cli vs native libsignal). This spec MUST NOT
  commit to a specific WhatsApp/Signal implementation. Deferral to the
  ADR is a MAY: the contract in R1 is the only normative requirement
  the deferred transports will inherit.
- A transport is NOT added to this spec's acceptance criteria until its
  choice ADR is ratified (MUST).

### R6. Inbound authorization

- Each adapter MUST carry a per-transport allowlist (chats/senders)
  configured in the Optimus home config; a message from outside the
  allowlist MUST be refused before any agent turn, and the refusal MUST
  be recorded with a named diagnostic that names the transport and the
  refusal class, e.g. `transport_refused_unauthorized` (MUST).
- An inbound message from an authorized chat that asks for a
  high-risk effect still goes through the existing SmartDeny approval
  model — the allowlist is authorization to converse, not a permission
  grant for effects (MUST; AGENTS.md law 7, ADR-0081).
- Allowlist changes MUST be config-only and applied without restarting
  the adapter (re-read on next inbound or on a config-change signal)
  (SHOULD).

### R7. Multi-adapter supervision

- A single `optimus gateway run` (supervisor mode) MUST be able to run
  every configured adapter, each on the R1 contract, as an in-process
  task (thread/task per adapter, not subprocess) so that panic
  isolation and shared state access are well-defined (MUST).
- When one adapter fails (panic, auth failure, network partition
  beyond retry), the supervisor MUST isolate it: other adapters keep
  serving, the failed adapter is restarted with capped exponential
  backoff, and its state is visible via the status surface (MUST;
  AGENTS.md law 9 — cancellation, and law 11 — observable and ordered
  events).
- Supervisor restarts MUST never re-dispatch a claim that is already
  leased; the claim-lease engine in `crates/optimus-ops/src/gateway.rs`
  remains the single source of truth for exactly-one-terminal-outcome
  (MUST).
- `optimus gateway status` MUST report per-adapter state
  (running / stopped / failed + last error + uptime) in addition to
  the existing queue status (MUST).

### R8. Delivery failure is an error, never silent success

- When an adapter's send fails permanently (auth revoked, chat
  deleted, transport rejected), the gateway MUST mark the outbound
  message failed with the named diagnostic and MUST surface the failure
  to the operator (status surface + log); it MUST NOT report success
  (MUST; ADR-0070 — an outbound send is a durable obligation, and the
  observed Claude Code `SendMessage` bug class: "Message sent" printed
  when the inbox write had actually failed).
- Transient failures MUST retry with capped exponential backoff per the
  message's claim lease. A message may be redelivered at most
  `max_attempts` (config) times; beyond that it becomes
  failed-permanently (MUST).

### R9. Mock-first wire conformance

- Every adapter MUST ship a mock transport (following the existing
  Telegram/Discord/Slack mock pattern) and a wire-conformance test
  suite that drives the adapter through the mock transport end-to-end:
  inbound → claim → agent turn → outbound → receipt (MUST).
- The conformance suites MUST run in `just verify` with zero skips on
  the managed path; live-transport smoke tests are manual/flagged and
  MUST NOT be CI-blocking (MUST).
- The conformance suite MUST include failure injection: transport
  down, auth failure, permanent rejection — each MUST produce the
  named diagnostic and the correct queue state (MUST).

### R10. Observability

- Every adapter MUST emit ordered, durable event rows (inbound
  received, claimed, turn started, turn completed, send attempted, send
  outcome) into the existing event/observability plane; a failed send
  and a refused inbound MUST be queryable by transport (MUST; AGENTS.md
  law 11).

## Acceptance criteria

- [x] A1. Given a configured Discord bot token and an allowlisted
  channel, when `optimus gateway discord run` starts against the mock
  transport, then a message inbound → agent turn → reply → receipt
  completes end-to-end in the conformance suite, and the reply is
  delivered exactly once (R1–R3, R9).
- [x] A2. Given the same setup with a permanently-rejecting transport,
  when the reply send fails, then the outbox marks the message
  failed-permanently with the named diagnostic and the status surface
  reports the failure — never a success receipt (R8).
- [x] A3. Given two running adapters (e.g. Discord mock + Slack mock)
  under the supervisor, when one adapter is killed, then the other
  keeps serving and the supervisor restarts the dead one with backoff,
  with no claim double-dispatch (R7).
- [x] A4. Given a message from a chat not in the allowlist, when it
  arrives at any adapter, then it is refused before any agent turn with
  the named diagnostic `transport_refused_unauthorized` and recorded as
  an ordered event (R6, R10).
- [x] A5. Given the post-refactor codebase, when `just verify` runs,
  then the Telegram conformance suite (mock + wire shape) passes with
  zero skips and no behaviour change vs the pre-refactor baseline
  (R1, R9).
- [x] A6. Given any adapter running under the supervisor, when
  `optimus gateway status` is queried, then per-adapter state
  (running / stopped / failed + last error + uptime) is reported
  (R7).
- [x] A7. Given a configured IMAP inbox + SMTP relay and a mock mail
  transport, when `optimus gateway email run` polls and an
  allowlisted sender's message arrives, then it converts to a
  canonical inbound message; when the agent replies, then the reply
  carries `In-Reply-To`/`References` threading headers and any
  attachment is written to the artifact store and referenced by path
  (R4, R9).

## Out of scope

- Web/desktop dashboard for gateway administration (planned in a
  later interface spec; not this one).
- WhatsApp and Signal live transports until their ADR-0091
  implementations land (R5).
- Auto-update / deployment of the gateway as a service (spec-018).
- Spam filtering, moderation ML, multi-tenant org features.
- Voice/video transport.

## Open questions

- WhatsApp/Signal transport choice — RESOLVED by ADR-0091 (2026-08-11):
  Signal via a supervised signal-cli child (JSON-RPC); WhatsApp via the
  Cloud API webhook behind an operator-provided public endpoint.
- Should Email v1 support sending without a prior inbound thread
  (cold sends)? Default: no — replies only, until a use case mandates
  cold sends.

## Links

- `crates/optimus-ops/src/gateway.rs` — the claim-lease engine every
  adapter rides on.
- `apps/optimus-cli/src/telegram_cmd.rs` — the existing live adapter to
  be refactored onto the R1 contract.
- `apps/optimus-cli/src/gateway_http.rs` — the gateway status HTTP
  surface.
- `docs/decisions/0070-an-outbound-send-is-a-durable-obligation.md`
  — durable-send foundation.
- `docs/decisions/0071-a-routing-address-is-not-a-session-identity.md`
  — routing foundation.
- `docs/decisions/0081-truthful-approval-resolution-and-session-consent.md`
  — approval truthfulness for inbound-driven effects.
- `docs/architecture/sota-scorecard.md` — "leading product losses" #4.
- `docs/architecture/parity-capability-ledger.json` — rows
  `gateway.telegram`, `gateway.discord-slack`, `gateway.queue`,
  `gateway.ui`.
