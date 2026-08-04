---
doc_id: decisions-0057-an-issue-earns-its-way-into-a-run
doc_type: decision
plane: decision
status: historical
authority: record
summary: "Superseded by ADR-0073 (2026-08-01) together with the optimus-engineering crate. Records triage as a contract gate: an issue produces checkable acceptance criteria and a reproduction, or a grounded refusal held to the same evidentiary standard and written in the reporter's own words."
reviewed_on: 2026-08-01
review_by: never
knowledge_type: decision
depends_on:
  - docs/decisions/0052-isolated-durable-engineering-runs.md
  - docs/decisions/0053-a-repository-is-asked-not-assumed.md
  - docs/decisions/0054-a-selector-may-only-over-select.md
  - docs/decisions/0056-a-reviewer-that-wrote-the-patch-is-not-a-reviewer.md
  - docs/decisions/0073-an-unreachable-vertical-is-archived-not-carried.md
---

# ADR-0057: An issue earns its way into a run, or is refused in the reporter's own words

- **Status:** Accepted 2026-07-29 — superseded 2026-08-01 by [ADR-0073](0073-an-unreachable-vertical-is-archived-not-carried.md)
- **Date:** 2026-07-29
- **Program:** program P41

> **Superseded.** `crates/optimus-engineering` was removed from the workspace on
> 2026-08-01, never having been integrated by any consumer. Nothing below is
> rewritten: the reasoning is preserved because it, not the code, is what a
> future attempt would need.

## Context

`TRIAGE` had to produce acceptance criteria and a reproduction before a run
could leave it. Nothing anywhere constrained what those items *said*.
`stated_by(navigator, AcceptanceCriteria, "it should work")` satisfied the
phase table exactly as well as a real contract does, and the run then entered
`IMPLEMENT` with nothing to satisfy.

That is the front door of the failure the whole program guards the back door
against. Differential proof (ADR-0055) catches a test that does not test the
bug; role separation (ADR-0056) catches a review that is not independent. Both
assume there is a *bug* and a *requirement* — that somebody pinned down what
the issue is and how anyone will know it is fixed. When triage is vapour, every
later gate verifies a patch against an unstated problem, and passes.

The symmetric failure matters just as much and gets less attention: an issue
*refused* without grounds. "Too vague, closing" is the cheapest sentence a
model can produce, it ends a run early, and it closes a bug report that a
human wrote. A triage layer that only makes refusal easy has optimised for
closing issues, not for working them.

## Decision

Triage produces a **checkable result** — a contract or a refusal — and a
deterministic checker decides whether it is *admissible*. Three verdicts, and
the third state is once again the load-bearing one:

- **`Admissible`** — nothing here is demonstrably wrong. Explicitly *not* a
  judgement that the triage is right: deterministic code cannot tell whether
  the acceptance criteria are the correct ones. It can tell that a quote is
  really in the issue, that a named component really exists, and that the risk
  class is not below what the component list implies. It checks those, and does
  not pretend to check the rest.
- **`Incomplete`** — fields missing or blank. The triage did not finish;
  presence is reported alone, before grounding, so a retry fixes one missing
  field instead of wading through twenty grounding failures it caused.
- **`Ungrounded`** — the triage said things the issue or the repository does
  not support, each named with what would have to change.

**Every verdict blames the triage. None blames the issue.** A failed check is a
triage to retry, not an issue to close — the two have the same symptom and
opposite remedies, and conflating them lets a lazy triage close a real bug.
Closing an issue takes an explicit `Refused` result, and a refusal is held to
the same evidentiary standard as a contract:

- **`TooVague`** needs a quote from the issue showing what is missing.
- **`TooLarge`** needs a proposed split of at least two parts — "split it"
  without a split is not a split (E41.5).
- A refusal whose quotes are not in the issue is itself refused. A model
  cannot close a report by inventing what the reporter said.

The grounding rules, all of which can only move a result *away* from
admissible (the ADR-0054 asymmetry):

1. **Quotes must appear in the issue body**, whitespace- and case-normalised
   so re-wrapping survives, and at least 12 characters so that matching one
   proves something.
2. **Named components and tests must exist** in the repository.
3. **Risk may not be understated**: a component matching the profile's
   sensitive globs (ADR-0053's floor plus declared additions) cannot be filed
   below `Sensitive`.
4. **The stop condition must say something the criteria do not** — a stop
   condition that restates a criterion never fires.
5. **Size ceilings** (E41.5): more than 3 owning components, 6 criteria, or 20
   expected files is more than one task, and the verdict names the remedy — a
   `TooLarge` refusal with a split. `TriageLimits::tighten` takes element-wise
   minima with the ceiling, so a repository can lower the limits and cannot
   raise them, the same shape as the sensitive floor.

Admission is enforced by construction: `evidence_drafts` — the only path from
a triage result to the `AcceptanceCriteria` and `Reproduction` evidence that
`TRIAGE` owes — takes the verdict as an argument and refuses everything except
`Admissible`. There is no way to record triage output that was not checked.

## Alternatives considered

**Prompt the navigator with a rubric.** Rejected — it is the current state.
The rubric and the violation are produced by the same process, which is the
same reason ADR-0056 rejects prompted independence.

**Have the checker itself close vague issues.** Rejected. The checker is
deterministic and cannot distinguish "this issue is vague" from "this triage
was lazy" — both look like thin output. Only a grounded `Refused`, which
carries quotes that survive the same checks, may close anything.

**Semantic validation of criteria (are they the *right* criteria?).**
Rejected as out of scope for deterministic code, and dishonest to fake with
heuristics. That judgement belongs to the P43 navigator (E43.1) and,
ultimately, to the human accepting the criteria — the P41 exit gate measures
exactly that acceptance rate.

**Fuzzy quote matching (edit distance, embeddings).** Rejected. Exact
containment after normalisation is auditable — a human can verify any
grounding decision with a text search. A fuzzy match that "probably" grounds a
quote reintroduces the confident guess this program exists to remove.

**One `Rejected` verdict covering both bad triage and bad issues.** Rejected —
it is the same conflation as `Unknown`/`Unprotected` (ADR-0053) and
`Inconclusive`/`NotFixed` (ADR-0055), pointed at somebody's bug report.

## Reasons

- Everything checked is something a model asserting it could fabricate
  cheaply, and everything not checked is something fabrication does not
  survive anyway — an invented component fails at `INVESTIGATE`, but an
  invented quote would have survived all the way to a PR description.
- The refusal path being *harder* than the contract path (quotes plus a split)
  points the incentive the right way: the cheap way out is to do the work.
- Naming the remedy in the oversize verdict ("belongs in a too_large refusal
  with a proposed split") turns a rejection into the next attempt's
  instructions.

## Consequences

- `TRIAGE` evidence can now be *required* to come through `evidence_drafts`,
  making vapour criteria unrecordable rather than merely discouraged. The
  controller wires this in when E43.1's navigator lands.
- A grounded refusal halts the run `NotActionable` with the reporter's own
  words in the stop reason — the issue comment writes itself from the record.
- The P41 exit gate ("ten historical issues produce criteria a human accepts
  without edit in eight cases") remains open: it measures the *navigator*
  (E43.1) through this contract, and there is no navigator yet. The contract
  is the ruler; the gate needs the thing to measure.
- `optimus-engineering` gains its first crates.io dependency beyond serde
  (`globset`, already in the workspace) to match sensitive globs.

## Risks

- **Grounding is not truth.** Every quote can be real and the reading still
  wrong. `Admissible` is named to keep that visible, and the human acceptance
  gate exists because of it.
- **The 12-character floor is a heuristic.** A short issue can have no
  12-character span worth quoting; the floor can force a longer quote than the
  point needs. Preferred over the alternative, where "it" grounds anything.
- **Ceilings are blunt.** A genuine single task can touch four components.
  The remedy is honest — refuse with a split, or a human overrides — but the
  friction is real and lands on legitimate large work.

## Evaluation evidence

`crates/optimus-engineering/tests/triage_contract.rs` — 6 integration tests
against this repository's real paths and a real `DevTaskRun`:

- `an_admissible_contract_is_exactly_what_triage_owes` — checked contract →
  drafts → recorded → run advances to `INVESTIGATE`
- `an_ungrounded_contract_produces_nothing_the_run_will_accept` — the invented
  quote and the missing component are both named in one verdict; no path to a
  recorded item exists
- `a_vague_issue_is_refused_in_the_reporters_own_words` — grounded refusal →
  `NotActionable` halt with the quote in the stop reason
- `an_issue_that_is_four_tasks_is_split_not_attempted` — the oversize verdict
  names the remedy, and the remedy is admissible
- `the_checker_holds_a_refusal_to_the_same_standard_as_a_contract` — a lazy
  refusal quoting words the reporter never wrote does not close their issue
- `triage_evidence_is_the_navigators_and_the_role_rules_agree` — P41's drafts
  pass P43's role check in `TRIAGE`, pinning the two subsystems together

Plus 17 unit tests in `triage.rs`, one per rule.

## Conditions for reconsideration

- The P41 exit gate run: if fewer than eight of ten historical issues yield
  human-accepted criteria, the contract's fields — not just the navigator —
  are suspect.
- Evidence that the ceilings misfire in practice (legitimate single tasks
  refused, or programmes squeezed under the limits by vague components).
- An issue tracker with structured fields (reproduction, environment) that
  would let grounding check structure rather than quotes.

## Relevant code

- `crates/optimus-engineering/src/triage.rs`

## Relevant tests

- `crates/optimus-engineering/tests/triage_contract.rs`
