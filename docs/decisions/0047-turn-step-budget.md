---
doc_id: decisions-0047-turn-step-budget
doc_type: decision
plane: decision
status: current
authority: record
summary: - Date: 2026-07-27 - Program: program P30+ (TUI + core foundation)
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: decision
covers:
  - crates/optimus-kernel/src/lib.rs
  - crates/optimus-kernel/src/turn_loop.rs
  - crates/optimus-kernel/src/chat_approval.rs
depends_on:
  - docs/decisions/0007-kernel-turn-loop.md
  - docs/decisions/0044-bounded-project-trust-and-capability-broker.md
  - docs/decisions/0046-approval-resumes-the-turn.md
validated_by:
  - crates/optimus-kernel/tests/kernel_turn.rs
---

# ADR-0047: A turn's step budget is 32 model round trips

- **Status:** Accepted
- **Accepted:** 2026-08-01 — delivered: `KernelConfig::default().max_steps == 32`, pinned by `crates/optimus-kernel/tests/adr_budgets.rs`
- **Date:** 2026-07-27
- **Program:** program P30+ (TUI + core foundation)

## Context

`KernelConfig::max_steps` caps how many times one turn may call the model.
`turn_loop.rs` checks it at the top of every iteration and returns
`KernelError::MaxSteps` when the count is reached. The default was 8.

Eight was chosen when a turn was a short thing. Two changes since have made it
too small to finish real work:

1. **ADR-0046 made approval pauses live inside the turn.** The step counter
   deliberately spans the pause — `turn_loop.rs` resumes from the highest step
   already recorded on the manifest, both to keep `(manifest_id, step)`
   collision-free and to stop a turn from buying itself a fresh budget every
   time it gets approved. So every held effect a turn touches spends steps on
   the round trip that proposed it, and the continuation counts on from there.
2. **ADR-0044 made `ReviewChanges` the default autonomy profile.** In the
   profile most users run, held effects are the common case, not the exception.

A step is one model round trip, not one tool call: `max_tool_calls_per_step` is
8, so eight steps could in principle carry 64 tool calls. Models do not work
that way. They call one tool, read the result, and decide — which spends a step
per observation. A turn that searches, reads two files, runs a command, has the
command held for approval, and then answers has used most of eight steps before
anything goes wrong. A retry after a failure uses one more.

The observed failure that prompted this: a live TUI session was asked to
summarise a web page. It proposed a Python script (approval card), the approved
action failed, it fell back to `web_search` twice, then proposed `curl`
(second approval card). The turn died at `max steps exceeded (8)` without ever
answering. Every individual step was reasonable. The budget was not.

A cap set below the length of a normal turn does not prevent runaway loops. It
converts ordinary work into a failure that looks like a loop, and it teaches
users that the agent gives up.

## Decision

**The default `max_steps` is 32.**

The cap keeps its purpose — it is the ceiling that stops a model stuck in a
retry loop from spending tokens without end — and keeps its shape: a hard limit,
checked before every round trip, spanning approval pauses. Only the number
changes.

`max_tool_calls_per_step` stays at 8, so the worst case a runaway turn can reach
is 32 round trips carrying at most 256 tool calls, every held effect among them
still gated by its own approval card.

## Alternatives considered

### Keep 8

The status quo, and the thing being overturned. It fails ordinary turns. A
budget that a well-behaved turn cannot live inside is not a safety limit, it is
a bug with an error message.

### Remove the cap, or set it to something like 1000

The cap is the only thing between a model in a retry loop and an unbounded bill.
Cancellation helps only while someone is watching. A number high enough to never
bind is the same as no cap, and the failure mode it prevents is real.

### Stop counting approval-paused steps against the budget

Appealing: the human's deliberation is not the model's work. But the step that
*proposed* the held effect is genuinely a model round trip, and a loop that
re-proposes the same effect after each denial is exactly the runaway the cap
exists to stop — exempting it would make that loop free. Carving out an
exception would also need a second counter, because the existing one is keyed to
`(manifest_id, step)` for model-call identity, not just for budgeting.

### Scale the budget by autonomy profile

More steps under YOLO, fewer under `ReviewChanges`. Backwards: `ReviewChanges`
is the profile where approval round trips eat the budget, so it needs the larger
number, not the smaller one. Autonomy is about what may run without asking, not
about how long a turn may think — conflating them was ADR-0044's explicit
"autonomy ≠ containment" line.

### Leave the default and expose it as a user setting

`max_steps` is already configurable through `KernelConfig`; nothing stops a
caller from raising it. That is not an answer, because the user cannot know the
right value and only discovers the wrong one as a failed turn. The default has
to be right on its own.

## Reasons

- The failure was observed in ordinary use, on the default profile, on a request
  with no unusual shape.
- 8 predates both the change that made approvals consume in-turn steps and the
  change that made approvals the default path.
- 32 is chosen to comfortably clear a turn with several tool observations and
  two or three approvals, while still being a number a human would notice in a
  bill.
- Nothing about the cap's safety role changes; a runaway loop still terminates,
  and no held effect becomes reachable without approval.

## Consequences

- A genuinely looping turn now costs up to four times as much before it stops.
- Turns that legitimately need many observations now finish instead of dying
  mid-work.
- `KernelError::MaxSteps` becomes a rarer and therefore stronger signal: hitting
  32 is much more likely to be a real loop than hitting 8 was.
- Any caller or test that assumed a default of 8 sees a different number.
  `tests/agent_contracts.rs` sets its own budget explicitly and is unaffected.
- Worst-case turn latency rises, because the ceiling on round trips rose.

## Risks

- **32 may still be too few** for a research-heavy turn with several approvals.
  If it is, the fix is evidence-led: raise it again against a specific failing
  turn, not pre-emptively.
- **32 may be too many to notice a loop.** A model retrying the same failing
  tool now does so up to 32 times before the cap catches it. The
  consecutive-failure limits in the runtime are the closer guard for that shape;
  if they turn out not to cover the in-turn case, that is the thing to fix, not
  the step budget.
- **Cost per pathological turn rises** in proportion. Cancellation and the
  streaming surfaces mean an attended turn can be stopped; an unattended one
  cannot, and pays the full ceiling.

## Evaluation evidence

- The live TUI session above, re-run: the same request completes and answers
  rather than reporting `max steps exceeded`.
- `max_steps_trips` in `kernel_turn.rs` continues to prove the cap fires, with
  its own explicit budget rather than the default.
- A test that a turn resuming after an approval counts on from the steps already
  recorded, rather than restarting the budget — the property ADR-0046
  established and this ADR depends on for the number to mean anything.

## Conditions for reconsideration

- A turn observed dying at 32 on legitimate work.
- Evidence of in-turn retry loops running to the cap rather than being caught
  earlier by consecutive-failure limits.
- A change that stops approval round trips from consuming turn steps, which
  would make the pre-ADR-0046 arithmetic valid again.

## Relevant code

- `crates/optimus-kernel/src/lib.rs` — `KernelConfig::default`
- `crates/optimus-kernel/src/turn_loop.rs` — the check, and the resume-aware
  step counter that makes the budget span an approval pause
- `crates/optimus-kernel/src/chat_approval.rs` — settlement leaving the turn
  running, which is what puts the continuation inside the same budget

## Relevant tests

- `crates/optimus-kernel/tests/kernel_turn.rs` — `max_steps_trips`
- `crates/optimus-kernel/tests/session_resume.rs` — `max_steps` error-code
  recording on a starved turn
