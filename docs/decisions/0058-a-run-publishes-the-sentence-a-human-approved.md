---
doc_id: decisions-0058-a-run-publishes-the-sentence-a-human-approved
doc_type: decision
plane: decision
status: historical
authority: record
summary: "Superseded by ADR-0073 (2026-08-01) together with the optimus-engineering crate. Records publication authority for a user's repository: the approval is the exact consequence sentence, the push publishes an explicit SHA rather than a branch tip, and delete, rename and force are unconstructible rather than filtered."
reviewed_on: 2026-08-01
review_by: never
knowledge_type: decision
depends_on:
  - docs/decisions/0052-isolated-durable-engineering-runs.md
  - docs/decisions/0053-a-repository-is-asked-not-assumed.md
  - docs/decisions/0056-a-reviewer-that-wrote-the-patch-is-not-a-reviewer.md
  - docs/decisions/0073-an-unreachable-vertical-is-archived-not-carried.md
  - docs/plans/github-engineer-program.md
---

# ADR-0058: A run publishes the sentence a human approved, and nothing else

- **Status:** Accepted 2026-07-29 — superseded 2026-08-01 by [ADR-0073](0073-an-unreachable-vertical-is-archived-not-carried.md)
- **Date:** 2026-07-29
- **Program:** program P44

> **Superseded.** `crates/optimus-engineering` was removed from the workspace on
> 2026-08-01, never having been integrated by any consumer. Nothing below is
> rewritten: the reasoning is preserved because it, not the code, is what a
> future attempt would need.
>
> One clarification worth carrying, because the two planes look alike: the
> forge operations described here are a *product* capability aimed at a user's
> own repository. They never governed how a coding agent delivers changes to
> **this** repository, which uses managed delivery and forbids `gh` and pull
> requests outright (`AGENTS.md`).

## Context

Everything before `READY_TO_PUBLISH` happens inside a worktree the run owns.
Everything after it happens somewhere a mistake cannot be deleted: a branch on
the forge is visible the moment it lands, a pull request notifies humans, and
GitHub's own behaviour turns two innocent-looking ref operations — renaming or
deleting a PR's head branch — into closing the PR.

ADR-0052 §7 already said publication is never auto-authorized. What it did not
say is what an authorization *is*. The prevailing shape — "Allow shell
command?" over a `git push` the human has to parse — approves a mechanism, not
a consequence. Program P44's first requirement inverts that: the prompt states
the consequence (`Publish branch wip/fix-123 to GitHub`), never the mechanism.
That sentence needs somewhere to live, something that checks the push against
it, and an answer for the gap between approval time and push time — the
worktree can gain a commit after the human says yes, and a push of the *branch*
would then publish something nobody saw.

Two smaller facts with the same shape:

- GitHub assigns PR numbers. This repository's naming-planes law already
  forbids choosing one (a `pr/44-…` branch invented because the program phase
  is P44 has closed a PR here before; the hard gates in `AGENTS.md` exist
  because of it).
- A PR body is prose, and prose is where a model asserts things no command
  observed. A body written by the implementation model is a report card written
  by the student.

## Decision

Publication is deterministic code — the plan stated in `publish_plan.rs`,
authority and effects in `delivery.rs`, body rendering in `pr_body.rs` — and
every rule is a type or a refusal rather than an instruction:

1. **The approval is a sentence, and the record holds it.** A
   [`PublishPlan`] renders its consequence exactly once:
   `Publish commit <sha> as branch <branch> on <repository>, then open a draft
   pull request against <base>.` A yes is recorded as `HumanApproval` evidence
   whose summary **is** that sentence. `push` and `create_draft_pr` refuse
   unless the run record already holds a corroborating approval whose text
   equals the current plan's consequence, character for character. A stale plan
   self-invalidates: the sentence embeds the commit, so a worktree that moved
   produces a different sentence, and the recorded approval no longer covers
   it. Nothing needs to detect staleness; equality does it.

2. **The push publishes the approved commit, not the branch tip.** The refspec
   is `<sha>:refs/heads/<branch>` — the exact commit the human approved. A
   commit that landed after approval stays local, rather than riding out on a
   branch-name push. There is no window between check and push in which the
   world can change, because the thing pushed is immutable.

3. **Deleting, renaming and forcing are unconstructible.** The refspec is
   built, never accepted as input. Branch names that would smuggle a second
   meaning — a colon (a refspec of its own), a leading `-` (an option), a
   leading `+` (force), emptiness (deletion), whitespace, `*` — are refused at
   construction with the reason named. There is no force field, no delete
   function, and no path that pushes to a differently-named remote ref. A
   non-fast-forward push therefore fails in git, is recorded with its real
   exit status, and corroborates nothing.

4. **An effect is confirmed by reading it back.** `git push` exiting zero is
   transport, not truth. After the push, `git ls-remote` must report the
   approved SHA at the branch; before the PR, the same check again; after the
   PR, `gh pr view --json` must report that head SHA, draft state, and the
   planned base. Only the confirmed pair corroborates — the same shape as the
   differential proof, where an exit status alone is never the point. The new
   [`EvidenceItem::observed_confirmed`] constructor records exactly this: a
   command that ran, and separately whether its effect was observed.

5. **The PR number is parsed, never chosen.** It exists only inside the receipt
   returned after GitHub's own output names it, and is confirmed against
   `gh pr view`. Output without a number is a refusal, not a guess.

6. **The body has no prose parameter.** `body_from_evidence` is the only
   producer of a PR body, and it renders the run record: every claim line
   carries the evidence sequence number that backs it, only corroborating
   items become claims, and failed observations never render as achievements.
   Unsupported claims are not rejected by a checker; they are rejected by
   there being nothing to write them with.

## Alternatives considered

**Approve once, trust the run to push "what was reviewed".** Rejected. The gap
between approval and push is exactly where an extra commit slips in, and a
branch-name refspec publishes it silently. Pushing the approved SHA removes
the gap instead of policing it.

**Bind the approval to a digest of the plan.** Works, but a digest hides what
was approved. The sentence *is* the binding and stays human-readable in the
record — an error can say "no recorded approval says: …" and show the exact
words. A hash would make the record's most important row opaque.

**Validate push flags with a denylist (`--force`, `--delete`, `--mirror`…).**
Rejected. A denylist is wrong the day git grows a flag. The module builds the
whole argument vector itself; there is nothing to filter because nothing is
accepted.

**Let the model draft the body and check its claims against evidence.**
Rejected for this phase. Checking prose claims is fuzzy matching under another
name (ADR-0057 rejected it for quotes). A rendered record is grep-ably
traceable; a checked draft is probabilistically traceable. When a template
wants sections the record cannot fill, the honest content is what the record
holds, not better prose.

**Confirm the repository via `gh repo view` from the working directory.**
Rejected — that is cwd inference, the thing E44.2's "repository confirmation"
exists to remove. The owner/name is parsed from the push remote's URL and
pinned on every `gh` call with `--repo`; a remote whose URL does not name a
forge repository cannot have a PR created for it at all.

## Reasons

- Every rule guards a mistake that is invisible after it happens: a pushed
  extra commit looks reviewed, a renamed head looks like housekeeping (and
  closes the PR), a chosen PR number looks assigned, an asserted body line
  looks observed. The cheapest place to stop all four is before the write.
- The approval sentence doubles as the audit trail. The record shows what a
  human was told would happen, in the words they read, next to the receipts
  showing it happened.
- Reusing the corroboration model (rule 4) means `Published` exits through
  `satisfied_evidence` like every other phase — no special authority path, no
  bypass to keep sound.

## Consequences

- The kernel's approval UI has one job: show `plan.consequence()` verbatim and
  record `plan.approval_draft()` on yes. Any paraphrase breaks the equality
  check — deliberately, because a paraphrase is a different approval.
- A worktree that gains a commit after approval needs a fresh approval, even
  though the push would have succeeded. Friction, accepted: the alternative is
  publishing unseen work.
- `Published` needs `PushReceipt` **and** `DraftPullRequest` to corroborate,
  so a push whose PR creation then fails leaves the run parked in `Published`
  with the branch live on the forge. The record says exactly how far it got;
  P45's recovery resumes from there rather than re-pushing.
- Subsequent pushes to an existing PR head (the `AddressingFeedback` loop, and
  this repository's `pr/N-…` local / `wip/…` remote convention) are program
  P45 work and will extend [`PublishPlan`], not bypass it.

## Risks

- **Sentence equality is strict.** A UI that trims a trailing space records an
  approval that covers nothing. The draft helper exists so the UI never
  hand-builds the row; the risk is a caller that does.
- **`gh` output formats drift.** The number is parsed from the PR URL and
  cross-checked against `--json number`; both changing shape at once would
  refuse valid creations (fail closed, never open).
- **A confirmed push and a refused PR strand a remote branch.** Recorded,
  visible, recoverable — but until P45 lands, cleanup is a human's.

## Evaluation evidence

`crates/optimus-engineering/tests/delivery.rs` — integration tests against a
real bare remote with real `git push`; only `gh` is stubbed:

- an unapproved push is refused and nothing reaches the remote
- approving one commit does not approve the commit after it
- an approved push lands, is confirmed, and corroborates `PushReceipt`
- the push publishes the approved commit even when the branch has moved on
- a phase without publish authority cannot push, approval or not
- the PR number is GitHub's; output without one is a refusal
- a PR whose head is not the approved commit does not corroborate
- the body handed to `gh` is byte-identical to the rendered record
- with both receipts confirmed, `Published` exits to `WaitingForCi`

- an approval recorded before the gate asked does not authorize the push
- a lying read-back cannot make a push believed
- a push that lands while its client dies is recorded as landed
- a refused push records what the remote already held
- a created PR whose confirmation fails is still on the record

Plus unit tests in `publish_plan.rs` for branch/remote/repository/SHA/refspec
validation, forge-URL parsing and consequence rendering, in `delivery.rs` for
PR-number parsing, and in `pr_body.rs` for body and title rendering, the
final-attempt filter, and markdown escaping of untrusted words.

## Addendum — independent review hardening (2026-07-29)

An independent fresh-context review of the first implementation returned
**not mergeable** with two blockers and a set of security should-fixes. All
were adopted the same day; each is now a rule the code enforces, listed here
so the decision record matches what ships:

1. **Every exit path leaves a record** (blocker). The first implementation
   recorded evidence only on the confirmed-success path, so a failed push was
   invisible to the run record — and rule 4's read-back never ran when the
   client failed, though a dying client is exactly when the remote's state is
   in doubt. Now `push` reads the remote back on *every* outcome and records a
   row before returning: a push that landed while its client died is recorded
   as landed (`PushLandedClientFailed` — the branch is live, blind retry is the
   wrong recovery), a refused push records what the remote already held, an
   unanswerable remote is `PushOutcomeUnknown` (unknown, not refused), and a
   created PR whose confirmation fails records that the PR exists without a
   believed receipt.
2. **The approval is scoped to the gate that asked** (blocker). Sentence
   equality alone let an approval recorded during any earlier phase authorize
   a later push — a planted row, or an honest one from a superseded attempt.
   Authorization now additionally requires the row to sit at
   `ReadyToPublish` at that phase's *current* attempt: the approval answers
   the gate that asked, not any gate that ever will ask.
3. **The repository is host-qualified and the sentence shows it.**
   `owner/name` alone let `evil.example/owner/name` render the same approval
   sentence as `github.com`'s. The repository field is `host/owner/name`
   (gh accepts the `HOST/OWNER/NAME` form in `--repo`), parsed from
   `git remote get-url --push` (the fetch URL can diverge from where a push
   actually lands), and validated segment-by-segment — 2–3 segments, no
   leading `-` or `.`, charset pinned — so a crafted URL cannot smuggle words
   into the sentence a human reads or arguments onto the `gh` command line.
4. **Config amplification is pinned off.** `push.followTags`,
   `push.recurseSubmodules` and friends can widen a push beyond its refspec;
   the built argv now carries `--no-follow-tags --recurse-submodules=no`, so
   repository config cannot make the push publish more than the sentence said.
5. **More names are refused at construction:** a remote that is a path
   (leading `.` or `~` — `git push .` writes the main checkout's refs), a
   branch equal to the base branch, a branch that is `HEAD` or `@` or carries
   non-ASCII-graphic characters (bidi overrides can make two branch names
   render identically), and a PR number not anchored at the URL's end.
6. **The rendered body defends itself.** Only evidence from each phase's
   final attempt renders (a superseded green is not a claim about the tree
   that ships), and untrusted words — issue titles, asserted summaries — are
   markdown-escaped, so an unterminated `<!--` in any GitHub user's issue
   title cannot swallow the evidence citations after it.
7. **The module split is a boundary, not bookkeeping.** The 800-line gate
   forced `publish_plan.rs` out of `delivery.rs`, and the seam is the
   decision's own: `publish_plan.rs` states what one publication will do and
   refuses malformed names; `delivery.rs` executes a stated plan and records
   what actually happened. Plan fields are private; delivery reads them
   through accessors and cannot restate the plan.

## Conditions for reconsideration

- P45 feedback pushes: the moment a second push to a live PR head is needed,
  rule 2's "approved commit only" meets a moving target and the plan type
  grows a successor-approval story.
- A forge API for draft-PR creation that returns structured output atomically
  (create + confirm in one call) would collapse rule 4's second read.
- Evidence that sentence-equality approvals mis-fire in practice (approvals
  recorded but never matching) — that would indict the UI contract, not the
  equality.

## Relevant code

- `crates/optimus-engineering/src/publish_plan.rs`
- `crates/optimus-engineering/src/delivery.rs`
- `crates/optimus-engineering/src/pr_body.rs`
- `crates/optimus-engineering/src/run.rs` (`EvidenceItem::observed_confirmed`)

## Relevant tests

- `crates/optimus-engineering/tests/delivery.rs`
