---
doc_id: decisions-0078-a-transcript-is-a-provider-contract-and-an-unreachable-toggle-asks
doc_type: decision
plane: decision
status: current
authority: record
summary: Stored transcripts are repaired to the tool-call pairing every provider requires, provider rejections carry the provider's own reason, and a Developer Full Access capability the user cannot enable asks for approval instead of denying with impossible advice.
reviewed_on: 2026-08-04
review_by: 2026-11-04
knowledge_type: decision
covers:
  - crates/optimus-kernel/src/tool_pairing.rs
  - crates/optimus-kernel/src/compress.rs
  - crates/optimus-kernel/src/session/repair.rs
  - crates/optimus-kernel/src/turn_loop.rs
  - crates/optimus-kernel/src/openai_compat.rs
  - crates/optimus-policy/src/developer_access.rs
  - crates/optimus-policy/src/lib.rs
depends_on:
  - docs/decisions/0044-bounded-project-trust-and-capability-broker.md
  - docs/decisions/0046-approval-resumes-the-turn.md
  - docs/decisions/0048-context-and-page-result-budgets.md
  - docs/decisions/0076-developer-full-access-is-a-scoped-grant-with-a-stable-supervisor.md
validated_by:
  - crates/optimus-kernel/src/tool_pairing.rs
  - crates/optimus-kernel/src/compress.rs
  - crates/optimus-kernel/src/openai_compat.rs
  - crates/optimus-policy/src/lib.rs
  - crates/optimus-kernel/tests/kernel_turn.rs
---

# ADR-0078: A transcript is a provider contract, and an unreachable toggle asks

- **Status:** Accepted
- **Date:** 2026-08-04

## Context

A self-development session failed in three reinforcing ways, and each one hid
the next.

Every `terminal` call was refused with `Developer Full Access does not grant
system.modify in the selected scope`, advising the user to enable the
capability. They could not. `system.modify` answers to the `production_systems`
toggle, `DeveloperAccessGrant::validate` refuses any grant that sets it, and the
settings UI never offers it (ADR-0076). Because `OpaqueShell` — any `sh -c` —
classifies as `system.modify` (ADR-0035), the mode built for self-development
made the most ordinary development command impossible, while the *same* command
merely asks when Developer Full Access is off: turning the mode on removed an
approval path instead of adding one.

The repeated failures made the turn long enough to compress. Compression cuts a
fixed-size middle, and the cut is positional while a tool call is structural, so
it landed between an assistant message carrying `tool_calls` and the `tool`
messages answering it. The parent was summarised away; the orphans stayed.

Every OpenAI-compatible provider rejects that shape. The transcript is saved, so
the damage was not confined to the turn that caused it — every later turn
replayed the same invalid history and was rejected again. The session was
permanently unusable.

What the user saw was
`model: https://api.deepseek.com/chat/completions: status code 400`. That is
`ureq`'s `Display` for a status error and nothing more: `send_json` returns
non-2xx as `Err`, so the client's own "HTTP {status} from provider: {snippet}"
branch was unreachable, and the provider's sentence naming the exact malformed
message was read off the socket and dropped.

## Decision

1. **Tool-call pairing is an invariant of stored history, not a hope.**
   `tool_pairing` owns it. A `tool` message whose `tool_calls` parent is absent
   is dropped wherever history is stored — after compression and on session
   load — because nothing can make it meaningful and keeping it is what the
   provider rejects.
2. **Compression cuts on a group boundary.** The tail start moves forward past
   any leading `tool` messages so it never begins with a result whose call is
   being summarised. Forward rather than backward: pulling the parent in would
   also drag in sibling results bounded only by `max_tool_result_chars`, and the
   verbatim tail must stay under the budget (ADR-0048).
3. **Synthesising a missing result is for the outgoing request only.** A request
   must be answerable now, so an open call is answered as not completed before
   it goes on the wire. Stored history is left alone, because an unanswered call
   is not always abandoned — a call parked on SmartDeny is waiting for the user,
   and writing "did not complete" into the transcript would contradict a live
   approval and duplicate the result once they decide (ADR-0046).
4. **A provider rejection carries the provider's reason.** The status arm is
   matched out, the body is read, and an OpenAI-shaped `/error/message` is
   lifted to the front of a bounded message. An unrecognised body is passed
   through bounded rather than discarded.
5. **A capability the user cannot enable asks; one they can still denies.**
   Under Developer Full Access, a request outside the granted scope is denied
   and says which scope. A request whose capability toggle is off is denied and
   names the toggle to turn on — except for `system.modify` and `commerce.spend`,
   which no valid grant and no UI can enable. Those ask for approval of the
   exact action.

## Consequences

- A session can no longer be bricked by its own history. Existing bricked
  sessions repair themselves the next time they are opened.
- A 400 is diagnosable from the message alone.
- Developer Full Access never auto-authorizes `system.modify`; the fence in
  ADR-0076 is unchanged. What changes is the answer when it is refused: an
  approval prompt for that exact effect rather than an instruction the user
  cannot follow. This restores the pre-grant behaviour rather than widening it,
  and `pause_before_destructive` already routed the same class to an approval.
- Repair is silent on stored history and reported on the wire, so a transcript
  that needed rescuing before a model call says so in the turn's status stream.

## Alternatives considered

- **Keep the tail cut and rely on repair alone.** Rejected: the repair would
  discard real tool results on every compression pass rather than only when
  something has already gone wrong.
- **Drop the assistant parent instead of answering its calls.** Rejected as the
  larger lie — the assistant did ask, and hiding the call invites the model to
  repeat work it already started.
- **Reclassify `sh -c` as project execution so it stops reaching
  `system.modify`.** Rejected: ADR-0035 chose that classification deliberately
  because an opaque command string can conceal any host effect, and the fix for
  an impossible denial is not a weaker classification.
- **Let the UI expose `production_systems`.** Rejected: ADR-0076 states the mode
  does not open that fence, and a per-action approval is the narrower grant.
- **Repair transcripts only at load.** Rejected: a turn that goes invalid
  mid-flight would still fail, and the failure would persist.

## Evaluation evidence

- Compression never leaves an unpaired transcript at any tail size:
  `compress::tests::every_tail_size_leaves_a_transcript_a_provider_accepts`.
- The live orphan is dropped and the parked approval is not answered:
  `tool_pairing::tests`.
- The approval flow still hands the model the effect it authorised, not an
  invented failure:
  `kernel_turn::project_write_emits_exact_approval_lifecycle_before_any_effect`.
- A rejection reports the provider's reason:
  `openai_compat::tests::a_rejected_request_reports_why_the_provider_rejected_it`.
- The unreachable toggle asks and the reachable one still denies:
  `optimus-policy` broker tests.

## Conditions for reconsideration

If a provider adopts a different history contract, the invariant moves behind
the provider adapter rather than being relaxed in the store. Do not extend
request-time synthesis to stored history without a rule that can distinguish an
abandoned call from a parked one.

## Reasons

Stored state that a provider will reject is not a degraded turn, it is a dead
session, so the invariant belongs where history is written rather than where it
is read. And a denial whose recovery action cannot be performed is
indistinguishable from a bug: the broker stays authoritative either way, so the
honest answer is to ask.
