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
  - crates/optimus-ops/src/adapters/signal.rs
  - crates/optimus-ops/src/adapters/whatsapp.rs
  - apps/optimus-cli/src/gateway_supervisor.rs
depends_on:
  - specs/017-gateway-breadth/spec.md
  - docs/decisions/0070-outbound-send-reconciliation.md
  - docs/decisions/0081-inbound-authorization-before-agent-turns.md
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

## Alternatives rejected

- Unofficial client protocols for either platform (private wire formats;
  unverifiable security; churn and ban risk).
- Native libsignal binding for Signal now (high effort, high risk; the
  revisit condition covers it).
- WhatsApp Cloud API without a webhook (Cloud API has no long-poll mode;
  the official path is push-only).
