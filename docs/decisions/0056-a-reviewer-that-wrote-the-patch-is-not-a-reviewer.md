---
doc_id: decisions-0056-a-reviewer-that-wrote-the-patch-is-not-a-reviewer
doc_type: decision
plane: decision
status: current
authority: record
summary: - Date: 2026-07-29 - Program: program P43
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: decision
covers:
  - crates/optimus-engineering/src/roles.rs
depends_on:
  - docs/decisions/0052-isolated-durable-engineering-runs.md
  - docs/decisions/0053-a-repository-is-asked-not-assumed.md
  - docs/decisions/0055-a-fix-is-proven-at-the-commit-it-fixes.md
  - docs/plans/github-engineer-program.md
validated_by:
  - crates/optimus-engineering/tests/role_separation.rs
---

# ADR-0056: A reviewer that wrote the patch is not a reviewer

- **Status:** Accepted
- **Date:** 2026-07-29
- **Program:** program P43

## Context

[ADR-0052](0052-isolated-durable-engineering-runs.md) says the implementation
model does not approve its own patch. That was a sentence in a document. The
phase table gave `REVIEW` read-only authority and required a `ReviewFindings`
item to leave it, and nothing anywhere checked where the findings came from.

The gap is the one that matters most, because the thing it lets through is
invisible. A model asked to review its own work agrees with itself — not
dishonestly, but because reviewing is re-deriving, and the same reasoning
re-derives the same answer. The run then holds a `ReviewFindings` row that is
byte-for-byte indistinguishable from an independent one. Every downstream
signal — the phase advanced, the contract was satisfied, the PR says
"reviewed" — is technically true and substantively empty.

Prompting cannot close this. "You are now the reviewer; be critical" changes
the label on the context, not the context. The instruction and the violation
are written by the same process.

## Decision

Evidence a model **asserts** carries the role *and the context* that asserted
it, and three rules decide whether it lands. They run inside
`DevTaskRun::record`, so a violation is a refusal, not a warning.

1. **A role may only produce the evidence its role is for.** An implementer
   does not file findings; a reviewer does not produce diffs. Well-formedness,
   mostly — it catches confusion rather than deception.
2. **A reviewer may not be a context that produced a diff in this run.** Not
   the same *role* — the same **context**. The run keeps the set of every
   context that has recorded a `Diff`, so a repair authored three phases later
   is still an author when review comes round again.
3. **Authority follows the role, not the phase alone.** A reviewer's ceiling is
   read-only wherever it runs. A phase that permits project writes does not
   hand them to a read-only role that happens to be active in it.

`context` is opaque: a session id, an agent invocation id, whatever the caller
uses for "one continuous piece of reasoning". Its only job is comparison.

**Evidence that came from running a command is not checked, and that is the
point.** `just verify` exiting zero is a fact about a process. It carries the
command, the commit, the exit status and a digest of the output; who pressed
enter changes none of them. Role rules exist to make a *claim* attributable,
and a command outcome is precisely what a claim is not. Those items are
attributed to `Role::Controller` — deterministic code, one context, no
reasoning to doubt. Drawing the line at "is there a command behind this?"
keeps the check exactly where models enter and nowhere else.

**Refusal happens at record time.** A rule applied later would be arguing with
a log that already says the patch was reviewed, and by then the honest options
are to delete history or to keep it and annotate. Neither is a good position;
not reaching it is cheap.

`Role::producing(kind)` names the canonical producer of each evidence kind, for
a caller deciding whom to dispatch. It is a convenience, not a way in: naming
the right role does not make a context independent, and rule 2 still refuses a
reviewer that wrote the patch. There is a test that says exactly this.

**Routing is a task-class signal, not a model choice** (E43.6).
`routing_for(phase)` returns the role a phase's reasoning belongs to and what
it is worth — high effort for root cause, planning and final review; standard
for implementation; cheap for classification. This crate cannot reach the
router and should not: picking a provider is the kernel's decision, made with
telemetry this crate never sees. What the phase table *does* know is which work
is expensive to get wrong, and that is the part a router is missing today.

## Alternatives considered

**Prompt the reviewer to be adversarial.** Rejected. It is the current state,
and it is what this ADR exists because of.

**Require a different *role*, and trust the label.** Rejected — it is the same
prompt in a struct. A context that writes the diff and then constructs
`RoleIdentity::new(Role::Reviewer, …)` with its own context passes a role check
and fails the actual requirement. Comparing contexts is the only version of
this that has teeth.

**Require a different *model*.** Rejected as both too strong and too weak. Too
strong: two runs of the same model from independent contexts, one of which
never saw the implementation reasoning, are genuinely independent for this
purpose. Too weak: it says nothing about a single model instance switching
hats, which is the failure actually observed.

**Check at phase exit rather than at record time.** Rejected. The record is
append-only, so a late check has to either refuse to advance while keeping a
false row, or rewrite history. Refusing the write is the only option that
leaves the log true at every instant.

**Apply the rules to command outcomes too.** Rejected. It would mean the
controller needs a role broad enough to produce every kind, which is a role
that permits everything — the check would exist and constrain nothing.

**A `verified_independent: bool` flag on the review item.** Rejected. Whoever
sets it is whoever the rule is about.

## Reasons

- The failure this prevents produces no visible symptom. Nothing downstream can
  detect it, so it has to be prevented at the point of entry.
- Comparing contexts rather than roles targets the actual mechanism: shared
  reasoning, not a shared label.
- Exempting command outcomes keeps the rule narrow enough to be strict.
  Everything it applies to is something a model said.

## Consequences

- `EvidenceItem::stated` is now `EvidenceItem::stated_by` and requires an
  author. Every existing call site had to name one, which is the migration
  working as intended: there was no correct default.
- `EvidenceItem::author` is `#[serde(default)]` to the controller, so runs
  recorded before P43 still load. Unattributed history reads as the
  controller's rather than as some role's — the conservative direction, since
  the controller can produce nothing that needs independence.
- A run cannot leave `REVIEW` until some context that did not write the patch
  files findings. Single-context operation now *stalls* there rather than
  silently self-approving. That is the intended behaviour and it will be felt
  before E43.1–E43.3 exist to supply the separate contexts.
- P44's PR body can state "independently reviewed" from the record, because the
  record can now prove it.

## Risks

- **Context ids are supplied by the caller.** A caller that passes a fresh
  string per evidence item defeats rule 2 entirely. The rules are a boundary
  inside the run, not a defence against the code that drives the run; the
  driver is the place that has to get this right, and E43.1–E43.3 are where
  that lands.
- **Rule 3 is stricter than it looks.** A read-only role active in a
  project-write phase is refused outright, not narrowed. If a navigator ever
  needs to assert something during `IMPLEMENT`, the authority model — not this
  check — is what should change.
- **Independence is structural, not semantic.** Two contexts that were given
  the same reasoning by whoever spawned them are separate by this rule and not
  in fact. This is a floor.

## Evaluation evidence

`crates/optimus-engineering/tests/role_separation.rs` — 10 tests driving a real
`DevTaskRun` to `REVIEW` and attempting the violation every way a confused or
obliging model would.

- `the_context_that_wrote_the_patch_cannot_review_it` — and the refusal leaves
  no row behind
- `changing_the_label_does_not_change_the_reasoning` — the same context under
  three different roles, all refused
- `a_repair_author_is_still_an_author_when_review_comes_round_again` — the set
  accumulates across phases rather than tracking the most recent author
- `an_independent_context_may_review_the_same_patch` — the rule permits the
  thing it exists to require
- `a_command_outcome_needs_no_role_because_it_makes_no_claim`
- `every_asserted_item_says_who_asserted_it_across_a_restart` — attribution and
  refusal both survive a save/load cycle
- `a_record_written_before_roles_existed_still_loads`

Plus 16 unit tests in `roles.rs`, including
`naming_the_right_role_does_not_buy_independence`.

## Conditions for reconsideration

- A context identifier the run can *derive* rather than be told, which would
  turn rule 2 from a boundary into a defence.
- Evidence that rule 3 blocks legitimate work, meaning role authority and phase
  authority need to compose rather than intersect.
- A review signal strong enough to justify relaxing rule 2 — an independent
  verifier that re-derives findings from the diff alone, with no access to the
  implementation reasoning.

## Relevant code

- `crates/optimus-engineering/src/roles.rs`
- `crates/optimus-engineering/src/run.rs` — `record`, `diff_authors`

## Relevant tests

- `crates/optimus-engineering/tests/role_separation.rs`
