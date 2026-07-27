---
knowledge_type: decision
status: current
covers:
  - crates/optimus-kernel/src/chat_approval.rs
  - crates/optimus-kernel/src/turn_loop.rs
  - crates/optimus-runtime/src/lib.rs
  - crates/optimus-host/src/chat.rs
  - apps/optimus-tui/src/session.rs
depends_on:
  - docs/decisions/0007-kernel-turn-loop.md
  - docs/decisions/0012-kernel-effectors.md
  - docs/decisions/0016-canonical-tool-contract.md
  - docs/decisions/0020-work-graph-integrity-and-loopback-security.md
  - docs/decisions/0045-agent-host-and-surface-transports.md
validated_by:
  - crates/optimus-kernel/tests/kernel_turn.rs
last_verified_commit: null
---

# ADR-0046: Approving an exact action resumes the turn

- **Status:** Proposed
- **Date:** 2026-07-27
- **Program:** program P30+ (TUI + core foundation)

## Context

When a tool call reaches an effect that SmartDeny holds, the turn loop parks. It
records a `ToolApprovalBinding` and returns `NeedsApproval` without finishing the
turn — deliberately, and `turn_loop.rs` says so:

> SmartDeny is a durable pause, not a terminal failure. The accepted turn and
> execution manifest remain running until the exact bound call is resolved.

Resolution then does the opposite. `chat_approval.rs` grants the approval, runs
the job, appends the tool result and a **canned** assistant receipt
(`format!("Approved and completed: {}.", binding.summary)`), and calls
`finish_turn` plus `finish_timed`. `TurnStatus::Running` is `unreachable!`d on
both branches. The module states the invariant outright: resolution "settles the
accepted turn deterministically with a tool result and an assistant receipt. It
never asks a provider to regenerate the paused call."

The user-visible result is that approving something ends the conversation turn.
Ask a question that requires a held effect, approve it, and the answer never
arrives — only a receipt saying the effect ran. Every surface behaves this way:
the TUI shows the receipt, and the desktop reloads the transcript
(`OptimusApp.tsx` `resolveTranscriptApproval`). The user has to send a fresh
message to get an answer to the question they already asked.

Two things are wrong, and they compound:

1. **The turn closes.** The model is never called again, so nothing in the
   product turns an approved effect into an answer. Approval reads as a dead end.
2. **The tool result carries no output.** On approval the result payload is
   `{"ok": true, "job": …, "status": "Succeeded"}`. `EffectOutcome` is provenance
   only — attempt id, node id, effect hash, status, receipt *hash*. The receipt
   body is persisted in `effect_attempts.receipt_json` and read into
   `EffectAttemptOutcome`, but `latest_effect_outcome` hashes it and discards it.
   So even on a later turn the model learns that the action ran, not what it
   produced.

Fixing (1) without (2) produces a model that confidently narrates an outcome it
cannot see.

## Decision

**Approving an exact action resumes the turn it paused.** `resolve_chat_approval_exact`
stops being terminal: on settlement it records the tool result and finishes the
approval, and leaves the accepted turn and its execution manifest Running — the
state the turn loop already left them in. The host then continues the turn
through the existing `resume_pending_turn_with_sink`, which reuses the Running
manifest and re-enters `run_recorded_turn`. That path finishes the turn, exactly
once, when the model actually stops.

**The tool result carries what the effect produced.** `EffectOutcome` gains the
receipt body alongside the hash, and the approved outcome's `data` carries it
instead of `{ok, job, status}`. The hash stays: it is the provenance check, and
nothing about effect-link binding changes.

**The invariant that mattered is kept, narrowed to what it protects.** The old
rule — never ask a provider to regenerate the paused call — existed so that the
effect the user authorised is the effect that runs, and no model round trip can
substitute a different one. That property is untouched. The approved call is
never re-derived: the continuation starts *after* the recorded tool result, from
a transcript in which the approved call and its outcome are already fixed. What
the model may now do is decide what happens next, which is what it does after any
other tool result.

**Denial resumes too.** The denied call becomes a cancelled tool outcome carrying
the user's reason, and the model gets to acknowledge it or choose another route,
rather than the surface asserting a cancellation on the model's behalf. The canned
assistant receipt is deleted in both cases: it was the product speaking in the
agent's voice about work the agent had not seen.

**A continuation that hits another held effect parks again.** Nothing special:
the turn loop records a new binding and returns `NeedsApproval`, and the user
gets a second card. Approval is per-effect, and one approval never widens into a
standing grant.

## Alternatives considered

### Leave settlement terminal and tell the user to send a follow-up

What ships today (`AFTER_APPROVAL` in the TUI). Honest, but it makes every
approval cost the user an extra message, and the follow-up turn still cannot see
the effect's output — so the model answers from provenance it does not have.
Cheap, and wrong in the direction of teaching users that approvals are a dead end.

### Resume the turn without carrying the receipt body

Half the change, and the worse half. The model would know a job succeeded and
would narrate an outcome it never observed. Confabulation is a worse failure than
silence.

### Have the surface synthesise a follow-up user message

Each surface would send something like "continue" after approving. It works
without kernel changes, but it fabricates user turns in the durable transcript,
duplicates the behaviour across TUI and desktop, and leaves the execution
manifest for the paused turn Running forever.

### Auto-resume inside `resolve_chat_approval_exact`

Would need a `ModelProvider` at the resolution call site. That drags provider
identity into a method whose whole job is exact runtime identity checking, and it
would make the resolution call block on a model round trip. Keeping resolution
provider-free and letting the caller resume preserves the split.

## Reasons

- The turn loop and the resolution path already disagreed about whether a parked
  turn is alive. The turn loop is right — it is the one that parked it.
- The machinery exists. `resume_pending_turn_with_sink` already requires exactly
  the state a parked turn is in: an active turn, a Running manifest, and no
  pending chat approval. Settlement is what breaks those preconditions today.
- The receipt body is already durable. Carrying it is surfacing persisted data,
  not recording anything new.
- A canned assistant receipt is the product impersonating the agent. Deleting it
  removes a class of text that reads as the model's but never came from it.

## Consequences

- Resolving an approval now costs a model round trip. The call is no longer
  fast, and surfaces must treat it as a streaming turn, not a request/response.
- `ChatApprovalResolution` no longer carries `assistant_receipt`; callers that
  displayed it must consume the streamed continuation instead.
- The desktop's `resolveTranscriptApproval` reloads the transcript after the call
  returns. That still shows the right thing, just later — it will not stream. A
  follow-up is needed to make it stream, and is out of scope here.
- Turn timings for an approved turn now include the paused wall-clock, because
  the turn genuinely was open that whole time.
- Approval receipts stop appearing as assistant messages in the durable
  transcript. Any consumer counting on that shape breaks.

## Risks

- **A continuation loops.** The model sees a tool result and calls the same tool
  again. Bounded by the existing max-model-steps cap; the parked step is already
  counted.
- **A resumed turn fails after the effect succeeded.** The effect ran and is
  receipted; the turn reports failure. This is already true of any turn that
  fails after a successful tool call, but it is more visible when the tool call
  was one the user personally authorised.
- **Receipt bodies can be large.** They reach the model as tool-result content.
  Needs the same budget treatment as any other tool result, and may need
  truncation with the truncation stated.
- **Cancellation during a resumed turn.** Ctrl-C now has something to interrupt
  after an approval. The cancellation token is threaded through
  `resume_pending_turn_with_sink`, so this is covered, but it is newly reachable.

## Evaluation evidence

- A kernel test that parks a turn on a held effect, resolves the approval, and
  asserts the model is called again with the tool result in the transcript.
- The same test asserting the approved call is not re-derived: the tool call id
  and effect hash in the resumed transcript equal the ones that were approved.
- A test asserting the turn is finished exactly once, by the continuation.
- A denial test asserting the reason reaches the model as a cancelled outcome.
- A test asserting the receipt body, not `{ok, job, status}`, is what the model
  receives.
- Live: ask the TUI a question needing a held effect, approve, get an answer.

## Conditions for reconsideration

- Evidence that resuming lets a model widen the blast radius of an approval —
  for example by immediately re-requesting a related effect that the user reads
  as covered by the approval they just gave.
- Receipt bodies proving too large to carry without truncation that misleads.
- A surface that cannot stream and for which a blocking resolution call is
  unacceptable.

## Relevant code

- `crates/optimus-kernel/src/chat_approval.rs` — the terminal settlement being
  overturned
- `crates/optimus-kernel/src/turn_loop.rs` — the park that leaves the turn open
- `crates/optimus-kernel/src/lib.rs` — `resume_pending_turn_with_sink`
- `crates/optimus-runtime/src/lib.rs` — `EffectOutcome`, `latest_effect_outcome`
- `crates/optimus-store/src/lib.rs` — `effect_attempts.receipt_json`
- `crates/optimus-host/src/chat.rs` — `chat_approval_resolve`
- `apps/optimus-tui/src/session.rs` — the resolve worker
- `apps/optimus-ui/src/app/OptimusApp.tsx` — `resolveTranscriptApproval`

## Relevant tests

- `crates/optimus-kernel/tests/kernel_turn.rs`
- `crates/optimus-kernel/tests/session_resume.rs`
- `crates/optimus-runtime/tests/command_capture.rs`
- `apps/optimus-tui/src/session.rs` (surface tests)
