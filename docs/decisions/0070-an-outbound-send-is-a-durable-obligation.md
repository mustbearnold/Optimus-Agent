---
doc_id: decisions-0070-an-outbound-send-is-a-durable-obligation
doc_type: decision
plane: decision
status: current
authority: record
summary: A reply owed to an external channel is committed to a durable ledger in the same transaction that makes the turn terminal, attempted only after the attempt is recorded, and never retried when the outcome is unknown; the honest guarantee is at-least-once with a fenced ambiguity window, not exactly-once.
reviewed_on: 2026-08-01
review_by: 2026-11-01
knowledge_type: decision
covers:
  - crates/optimus-ops/src/gateway.rs
  - crates/optimus-ops/src/gateway/outbound_ledger.rs
  - crates/optimus-ops/src/gateway/outbound_receipts.rs
  - crates/optimus-ops/src/telegram.rs
depends_on:
  - docs/decisions/0021-owned-execution-and-causal-delivery.md
validated_by:
  - crates/optimus-ops/src/gateway/outbound_ledger.rs
  - crates/optimus-ops/src/telegram.rs
---

# ADR-0070: An outbound send is a durable obligation

- **Status:** Accepted
- **Date:** 2026-08-01

## Context

ADR-0021 made local SQLite the delivery authority for the gateway, and program
P28 gave the **inbound** half real durability: claims, leases, fenced attempts,
and exactly one terminal outcome per turn.

The **outbound** half had none of it. `complete_claim` committed the turn, and
only afterwards did an adapter read the result back and call the platform. Two
windows sat inside that gap and neither was recorded anywhere:

1. Between commit and send, nothing durable said a send was owed. A crash there
   lost the reply silently — the turn showed `succeeded`, the user got nothing,
   and no surface could tell the difference.
2. During the send, a crash was indistinguishable from never having tried. The
   only recovery signal was `list_ambiguous_sends`, which reports *every*
   succeeded turn without a receipt, so never-attempted and
   attempted-outcome-unknown looked identical — and those two demand opposite
   actions.

The 2026-08-01 competitive audit filed this as B-ARCH-07, blocking B-CAP-02
(first live messaging transport). Shipping a real transport on top of a
best-effort send would have converted every process restart into silent message
loss, on a surface where the user is a person waiting for a reply.

The same read surfaced a second defect. The adapter sent `drained.reply_preview`
— the field `complete_claim` builds with `.take(200)` for operator display — so
every reply over 200 characters arrived truncated, while the gateway recorded a
delivery receipt for it.

## Decision

1. **The obligation is written by the turn, not by the sender.**
   `record_obligation` runs inside the same transaction as the terminal
   `gateway_messages` update. After commit there is no instant in which the turn
   is terminal and the debt is unrecorded. A failed turn owes nothing; a turn
   with no routing address owes nothing.
2. **The obligation carries the whole reply.** `gateway_outbound.body` holds the
   full outbound text. `reply_preview` remains what it always was — a display
   truncation — and no send path reads it.
3. **The attempt is recorded before the network is touched.** `claim_outbound`
   leases the obligation, increments the attempt count, and inserts an
   `in_flight` attempt row in one transaction, then returns. A crash after that
   point is visible as attempted-with-unknown-outcome.
4. **An unknown outcome never retries on its own.** A `sending` row whose lease
   expires becomes `ambiguous`, never `pending`. Only
   `resolve_ambiguous_obligation` moves it, and only after a human establishes
   whether the platform already has the message.
5. **A definite failure retries to a bound.** Five attempts, then `abandoned`.
   Retrying is safe only because the platform refused; the adapter takes one
   attempt per obligation per cycle so the bound counts intervals rather than
   loop iterations.
6. **Routing addresses stay opaque to the gateway.** `target` is stored exactly
   as the owning channel wrote it (`telegram:42`) and parsed only by that
   channel's adapter.
7. **The turn-level receipt columns become a projection.** `delivered_unix` and
   `terminal_reason` are maintained by the ledger, so surfaces that already read
   them stay truthful without knowing the ledger exists. A turn whose send is
   still `pending` or `sending` is no longer counted as ambiguous — it is owed
   work with a known position, not an unanswerable question. The read side lives
   in `outbound_receipts`, a sibling of the ledger: it queries `gateway_messages`
   and never the obligation tables, so the write authority and the compatibility
   view cannot drift into each other.

## Alternatives considered

- **Send inside the turn transaction.** Holds a write lock across a network
  call, and a rollback cannot unsend. Rejected outright.
- **Retry ambiguous sends automatically.** Cheap to build and the usual default,
  but it converts every timeout into a probable duplicate. On a chat surface a
  duplicate is worse than a delay, and no transport in scope offers a dedupe
  primitive to make it safe.
- **Reuse `gateway_messages` with more columns.** One row cannot carry N
  attempts with distinct outcomes, and the turn and its send settle
  independently — a succeeded turn can owe a send that ultimately fails.
- **Put the ledger in a sibling module (`src/outbound_ledger.rs`).** Would have
  forced `open_database` and `now_unix` to `pub(crate)`, widening the gateway's
  private surface to every module in the crate. As a child module it reaches
  them by the ancestor-visibility rule and widens nothing.
- **Keep `list_ambiguous_sends` as the only recovery surface.** It cannot
  distinguish the two states that need opposite handling, which is the defect.

## Reasons

Durability has to be established by whoever already holds the transaction. Any
design where the sender records its own intent has a window where the intent is
only in memory, and that window is precisely where crashes are invisible.

Recording the attempt before the call is what buys the distinction between
never-tried and outcome-unknown. It costs one write per send and is the only
thing that makes recovery a decision rather than a guess.

## Consequences

- A crash between turn commit and send no longer loses the reply; the next poll
  cycle finds it owed and sends it.
- Replies over 200 characters arrive whole.
- Operators gain `list_pending_obligations` / `list_ambiguous_obligations` /
  `list_unsettled_obligations` and an explicit resolution verb, in place of one
  undifferentiated list.
- B-CAP-02 can put a live transport behind the adapter without inheriting silent
  loss. Remote-initiated effects still ride the SmartDeny approval spine — this
  ledger governs delivery, not authorization.
- `gateway.rs` ratchets down 1006 → 816 production lines: the outbound receipt
  cluster moved out to the module that now owns it.

## Risks

- **At-least-once is still at-least-once.** A confirmed send whose confirmation
  is lost in transit lands in `ambiguous`, and an operator who resolves it as
  not-delivered will produce a duplicate. The ledger makes that a recorded human
  decision instead of an automatic one; it does not eliminate it.
- **Ambiguous obligations accumulate without an operator.** Nothing drains them
  automatically — by design — so an unattended deployment grows a queue. The
  status counts make it visible; acting on it is out of scope here.
- **`MAX_SEND_ATTEMPTS` is a bound, not a backoff.** Pacing lives in the
  adapter's cycle. An adapter that polls in a tight loop would exhaust the bound
  quickly; the telegram adapter takes one attempt per obligation per cycle
  precisely to prevent that.

## Evaluation evidence

- `crates/optimus-ops/src/gateway/outbound_ledger.rs` — 14 tests covering
  obligation creation on the commit path, the whole-reply regression, single-
  winner claiming, the retry bound, expiry-to-ambiguous, and both operator
  resolutions.
- `crates/optimus-ops/src/telegram.rs` — adapter tests proving the chat receives
  the full reply, a send stranded by a crash goes out on the next cycle, a
  refused send is not retried inside the same cycle, and an unknown send is
  never re-sent by the adapter.

## Conditions for reconsideration

A transport enters scope that offers a real dedupe primitive (a client-supplied
idempotency key the platform honours), making automatic retry of ambiguous sends
safe — at which point clause 4 should be revisited for that transport only.
Alternatively, product evidence that operators never resolve the ambiguous queue
would mean the manual gate is the wrong shape and needs a policy-driven default.

## Relevant code

- `crates/optimus-ops/src/gateway/outbound_ledger.rs`
- `crates/optimus-ops/src/gateway/outbound_receipts.rs`
- `crates/optimus-ops/src/gateway.rs`
- `crates/optimus-ops/src/telegram.rs`

## Relevant tests

- `crates/optimus-ops/src/gateway/outbound_ledger.rs`
- `crates/optimus-ops/src/telegram.rs`
