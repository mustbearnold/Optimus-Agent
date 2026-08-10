---
doc_id: decisions-0091-whatsapp-signal-transport-choice
doc_type: decision
plane: decision
status: current
authority: record
summary: WhatsApp and Signal ride the spec-017 transport contract behind two operator-facing choices. Signal uses signal-cli as a supervised child process (official protocol, JSON-RPC over a local socket, no inbound ports). WhatsApp uses the Meta Cloud API with an operator-provided public HTTPS webhook (official API, phone allowlist + webhook secret, fail-closed). Unofficial client-side protocols are rejected for both: they reverse-engineer the platforms' private wire formats, which is an unverifiable security and stability boundary.
reviewed_on: 2026-08-11
review_by: 2026-12-11
knowledge_type: decision
covers:
  - apps/optimus-cli/src/gateway_supervisor.rs
  - crates/optimus-ops/src/transport.rs
depends_on:
  - specs/017-gateway-breadth/spec.md
  - docs/decisions/0070-an-outbound-send-is-a-durable-obligation.md
  - docs/decisions/0081-truthful-approval-resolution-and-session-consent.md
---

# ADR-0091: WhatsApp and Signal transport choice

## Status

Current.

## Context

spec-017 R5 defers WhatsApp and Signal to a transport-choice ADR. Both are
messaging channels the operator may use for remote control, and both must
ride the same claim→turn→settle spine as Telegram, Discord, Slack, and
Email, with the same security posture:

- Inbound authorization (ADR-0081): the allowlist authorizes a
  conversation, never a permission. High-risk effects still pause for
  SmartDeny approval.
- The durable outbound ledger (ADR-0070) stays the delivery authority.
- No adapter may require inbound TCP ports on the operator's machine,
  unless the transport is explicitly supervised and documented.

The platforms differ in what they officially offer:

- Signal ships `signal-cli` — the official client as a JVM binary with a
  JSON-RPC mode over a local socket. No reverse engineering. A JVM runtime
  is the cost.
- Meta offers the WhatsApp Cloud API — an official HTTP API with webhook
  push. It requires a public HTTPS endpoint the operator must provide.
  No phone-side client needed.
- Both platforms have unofficial client libraries that speak the private
  wire protocol directly. They are widely used, but they are reverse
  engineering: protocol churn, account risk, and an unverifiable security
  boundary.

## Decision

### Signal: signal-cli as a supervised child process

The adapter owns a `signal-cli` child process (JSON-RPC mode, local socket).
The supervisor restarts it with backoff on failure, exactly like any other
adapter worker. This is the supervised-child pattern the self-development
supervisor already uses: build before replace, health probe, emergency
stop.

Rationale:

- Official protocol. Security boundaries stay in platform code.
- No inbound ports. The child talks to the platform over its own outbound
  connection; Rust talks to the child over a local unix socket.
- The JVM dependency is an install-time cost, documented in the runbook,
  not a runtime architecture change.

Revisit condition: if the JVM runtime becomes unacceptable, evaluate
native `libsignal-client` binding with a dedicated review. Do not adopt
unofficial client libraries at any point.

### WhatsApp: Meta Cloud API behind an operator-provided HTTPS webhook

The adapter registers a webhook endpoint and validates every inbound
request before any processing:

1. The endpoint requires TLS (the operator terminates TLS at their chosen
   reverse proxy or tunnel).
2. A webhook secret header must match before the payload is parsed.
3. The sender phone number must pass the allowlist (ADR-0081).
4. Fail-closed: any missing or invalid check refuses the inbound with the
   named `transport_refused_unauthorized` diagnostic.

Outbound uses the Cloud API send-message REST call, settled through the
outbound ledger exactly like every other transport.

Rationale:

- Official API. No reverse engineering, no account risk.
- The public endpoint is a narrow, audited surface: one path, one secret
  check, allowlist before any agent turn. This is a security boundary
  enforced by code, not by obscurity.
- No operator public endpoint means WhatsApp stays disabled with a
  documented status. That is a state, not a defect.

Revisit condition: if a future operator deployment can provide no public
endpoint, re-evaluate a supervised whatsmeow-style child with a dedicated
review. Do not adopt it before that review.

## Consequences

- Both adapters are ordinary `TransportAdapter` implementations with
  mock-first conformance (spec-017 R9). Live smoke requires a real account
  and is operator-initiated.
- `signal-cli` becomes a documented system dependency (see runbook). The
  adapter never stores its registration secrets in home config; it reads
  the operator's existing signal-cli data directory.
- The WhatsApp webhook adds a new public surface. It must be supervised
  (running only under `optimus gateway run`) and its status must be
  visible in the status surface (spec-017 R7).
- No change to the outbound ledger, the allowlist model, or SmartDeny.

## Alternatives considered

- **Unofficial client protocols for either platform** (whatsmeow-style /
  private wire formats): rejected — unverifiable security boundary,
  protocol churn, account risk. Revisited only through a dedicated review
  if a future operator deployment cannot provide a public endpoint
  (WhatsApp) or if the JVM runtime becomes unacceptable (Signal).
- **Native `libsignal-client` binding for Signal now**: rejected — high
  effort and high risk for no user-visible gain; the supervised signal-cli
  child already delivers the official protocol. Revisit condition covers
  it.
- **WhatsApp Cloud API without a public endpoint (polling)**: not offered
  by Meta; webhook push is the only official inbound. Rejected by the
  platform, not by us.

## Reasons

- The repo's laws (constitution) require security boundaries enforced by
  code and permissions; official protocols keep the boundary in audited
  platform code.
- The supervised-child pattern is already proven in this repo
  (self-development supervisor: build before replace, health probe,
  emergency stop), so the Signal adapter reuses an established runtime
  shape instead of inventing one.
- The allowlist + SmartDeny posture (ADR-0081) is transport-agnostic and
  unchanged: authorization to converse never authorizes effects.

## Risks

- signal-cli: JVM runtime dependency (install-time cost, documented in the
  runbook); JSON-RPC surface of the child must be version-pinned.
- WhatsApp webhook: a new public surface. Mitigations: one path, mandatory
  TLS, secret header before parsing, allowlist before any turn, fail-closed
  diagnostics, supervised lifecycle.
- Both transports depend on platform API stability; the revisit conditions
  name the triggers.

## Evaluation evidence

- Mock-first conformance per spec-017 R9: both adapters are ordinary
  `TransportAdapter` implementations riding the same claim→turn→settle
  spine already exercised by
  `crates/optimus-ops/tests/adapter_conformance.rs`.
- Live smoke is operator-initiated with a real account; no live evidence is
  claimed in this ADR.

## Conditions for reconsideration

- Signal: if the JVM runtime becomes unacceptable → evaluate native
  libsignal with a dedicated review.
- WhatsApp: if no operator deployment can provide a public HTTPS endpoint →
  re-evaluate a supervised unofficial-protocol child with a dedicated
  review.

## Relevant code

- `crates/optimus-ops/src/transport.rs` — the contract both adapters
  implement.
- `apps/optimus-cli/src/gateway_supervisor.rs` — the supervisor that owns
  the signal-cli child and the webhook lifecycle.

## Relevant tests

- `crates/optimus-ops/tests/adapter_conformance.rs` — contract spine.
- `crates/optimus-ops/tests/supervisor_isolation.rs` — worker isolation
  (A3) the signal-cli child inherits.
