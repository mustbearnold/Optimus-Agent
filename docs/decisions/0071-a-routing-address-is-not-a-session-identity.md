---
doc_id: decisions-0071-a-routing-address-is-not-a-session-identity
doc_type: decision
plane: decision
status: current
authority: record
summary: The gateway's per-message session field is a routing address, not a kernel session id; a turn derives its session deterministically from that address and returns the address unchanged as the reply target, and a remote-initiated turn that trips SmartDeny settles once as a paused reply that only the local operator can resolve.
reviewed_on: 2026-08-01
review_by: 2026-11-01
knowledge_type: decision
covers:
  - crates/optimus-host/src/gateway_turn.rs
  - crates/optimus-host/src/chat.rs
  - apps/optimus-cli/src/gateway_http.rs
  - crates/optimus-ops/src/gateway.rs
depends_on:
  - docs/decisions/0070-an-outbound-send-is-a-durable-obligation.md
  - docs/decisions/0021-owned-execution-and-causal-delivery.md
validated_by:
  - crates/optimus-host/src/gateway_turn.rs
  - crates/optimus-host/tests/gateway_address_contracts.rs
---

# ADR-0071: A routing address is not a session identity

- **Status:** Accepted
- **Date:** 2026-08-01

## Context

ADR-0070 made a reply owed to an external channel a durable obligation, and it
kept routing addresses opaque: `target` is stored exactly as the owning channel
wrote it (`telegram:42`) and parsed only by that channel's adapter.

The field that carries the address is `InboundMessage::session_id`, and the
turn closure `drain_one` calls has signature

```rust
FnMut(&InboundMessage) -> Result<(String, Option<String>), String>
```

where the returned `Option<String>` becomes `OutboundMessage::session_id` and
is written by `record_obligation` as the obligation's **delivery target**. One
field therefore carries two different things on two different legs of the same
turn: on the way in, where the message came from; on the way out, where the
reply must go.

Both existing gateway turns read that field as a kernel session id and returned
a kernel session id in its place:

- `optimus-host`'s `drain_gateway_once` UUID-parsed the address and propagated
  the parse error. A live `telegram:42` therefore failed the turn, `fail_claim`
  retried it to the `MAX_ATTEMPTS` bound, and the message dead-lettered. A real
  Telegram message could not be answered at all.
- `optimus-cli`'s `gateway_http::drain_once` parsed the same field with `.ok()`,
  so a non-UUID address was silently discarded and every webhook request opened
  a fresh session with no memory of the last one.

Both then returned `Some(kernel.session_id().to_string())`, so every obligation
either path minted was addressed to a bare kernel UUID — an address no adapter
can route, and one `deliver_owed_sends` classifies as a definite failure.

The second question arrives with the first live transport. A remote-initiated
turn that reaches a high-risk effect returns
`KernelError::Runtime(RuntimeError::NeedsApproval { job_id, node_index })`.
`turn_loop` deliberately propagates that rather than terminalising it, because
SmartDeny is a durable pause: the accepted turn and its execution manifest stay
alive until the exact bound call is resolved. `drain_one` sees only `Err`, so
today a paused turn is a failed claim — retried, pausing a second and third
manifest, then dead-lettered. That is an architectural law 10 violation (one
execution, no terminal outcome) and, worse, an amplification vector: a remote
sender gets three paused jobs for one message.

## Decision

**1. `session_id` on a gateway message means one thing: the routing address of
the conversation it belongs to.** It is the channel's own address string, in the
`<channel>:<address>` shape every adapter already mints and
`tests/channel_seam_contracts.rs` already asserts. The gateway continues never
to parse it.

**2. A turn derives its kernel session from that address deterministically.**
`session_for_address` is a UUID v5 under a fixed Optimus gateway namespace, so
the same chat resumes the same session across restarts with no new table, no
migration, and no lookup that could disagree with itself. An absent address
means an anonymous turn: a fresh session, and — per ADR-0070 — no obligation.

**3. A turn returns the address it was reached at, unchanged.** Not the session
it happened to run in. This is what makes the ledger's opaque-target rule true
end to end rather than true only for adapters that never went through a kernel.

**4. A remote SmartDeny pause is terminal for the message and open for the
job.** The turn catches `NeedsApproval`, settles the gateway message exactly
once with a reply that says the work is paused pending the operator, and leaves
the job paused. The job id and node index go to the local operator's log only.

**5. The remote sender is never given a handle to their own approval.** The
reply names no job id, and no gateway surface grants one. Approval remains
`optimus approvals grant <job_id>`, run locally by whoever owns the machine.

## Alternatives considered

- **Add a separate `reply_to` column to the inbound row.** Honest, but it makes
  every adapter, the HTTP surface, and the stored schema carry a second field
  whose only correct value is the one already in `session_id`. The defect is
  that the field's meaning was never decided, not that one field is too few.
- **Keep parsing a UUID and fall back to a derived session when it fails.**
  Preserves the old webhook behaviour exactly, at the cost of leaving the field
  bivalent — which is the actual bug. A caller cannot then tell what it is
  holding, and neither can the next reader of the code.
- **Store the address→session map in a table.** Adds a migration and a second
  source of truth for something a pure function already answers, and it can
  drift from the ledger it is supposed to agree with.
- **Treat `NeedsApproval` as a failed turn and let the message dead-letter.**
  This is the status quo. It ends with three paused manifests per message and no
  reply, which is both law 10 and law 7 read backwards.
- **Let the remote sender approve by replying.** Rejected outright. It converts
  SmartDeny from a local-operator gate into something the requester controls,
  which is exactly the boundary architectural law 15 exists to hold.

## Reasons

- The opaque-address rule from ADR-0070 was only enforceable where no kernel sat
  in the middle. Deriving the session from the address, rather than the reverse,
  restores it for every path.
- Determinism buys conversation continuity for free. A Telegram chat that keeps
  its thread across an agent restart is not a separate feature; it falls out of
  the derivation.
- A paused effect and an unanswered message are different failures. Conflating
  them made the safe outcome (pause) look like the unsafe one (error) and then
  retried it, which is how a safety mechanism turns into an amplifier.
- Telling the remote sender that something is paused is honest and costs
  nothing. Telling them *which* job is paused hands a remote party the one
  identifier the approval path is keyed on.

## Consequences

- A caller that previously round-tripped the kernel UUID this gateway returned
  now round-trips an address. Behaviour is unchanged for anyone echoing what the
  gateway hands back, which is what both surfaces already return.
- A caller that hard-codes an old kernel UUID as its session gets a stable
  session derived from that string rather than the original one. One
  conversation restarts once, at the upgrade. Nothing is lost from the store.
- Obligations minted by a kernel turn become routable, so a reply reaches the
  chat that asked instead of being settled as an unroutable failure.
- A gateway message whose turn paused is answered and closed. The paused job
  survives it and stays visible to `optimus approvals`.
- `uuid` gains the `v5` feature workspace-wide.

## Risks

- **A derived session is guessable from a public address.** It identifies a
  local session; it grants nothing. Reading or writing that session still
  requires the local store, and every effect inside it still passes SmartDeny.
- **A chat that changes its address gets a new session.** True, and correct: a
  new address is a different conversation as far as the gateway can tell.
- **An honest "paused" reply tells a remote party that an approval gate exists.**
  Accepted. The alternative is silence or a lie, and neither is better than a
  sentence that names no job and offers no way to resolve it.

## Evaluation evidence

- `crates/optimus-host/tests/gateway_address_contracts.rs` — address→session
  determinism, address round-trip through the ledger, and the paused-turn
  terminal path.
- `crates/optimus-ops/tests/channel_seam_contracts.rs` already asserts every
  obligation is addressed `<channel>:…`; it passed before only because its turns
  echoed `message.session_id` directly rather than going through a kernel.

## Conditions for reconsideration

- A channel appears whose routing address is not stable for the lifetime of a
  conversation, so a derived session would fragment it.
- Sessions gain identity that must be assigned rather than derived — for example
  one conversation deliberately spanning two channels.
- Approval acquires a remote-safe delegation model, at which point clause 5 is
  the thing to revisit, deliberately and on its own.

## Relevant code

- `crates/optimus-host/src/gateway_turn.rs`
- `crates/optimus-host/src/chat.rs`
- `apps/optimus-cli/src/gateway_http.rs`
- `crates/optimus-ops/src/gateway.rs`

## Relevant tests

- `crates/optimus-host/tests/gateway_address_contracts.rs`
- `crates/optimus-ops/tests/channel_seam_contracts.rs`
