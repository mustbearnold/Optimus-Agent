---
doc_id: decisions-0055-a-fix-is-proven-at-the-commit-it-fixes
doc_type: decision
plane: decision
status: historical
authority: record
summary: "Superseded by ADR-0073 (2026-08-01) together with the optimus-engineering crate. Records differential proof: a regression test is run at the base commit with only the test carried across, and a base run that never reached the test is inconclusive rather than not-fixed."
reviewed_on: 2026-08-01
review_by: never
knowledge_type: decision
depends_on:
  - docs/decisions/0052-isolated-durable-engineering-runs.md
  - docs/decisions/0053-a-repository-is-asked-not-assumed.md
  - docs/decisions/0054-a-selector-may-only-over-select.md
  - docs/decisions/0073-an-unreachable-vertical-is-archived-not-carried.md
---

# ADR-0055: A fix is proven at the commit it fixes, or it is not proven

- **Status:** Accepted 2026-07-29 — superseded 2026-08-01 by [ADR-0073](0073-an-unreachable-vertical-is-archived-not-carried.md)
- **Date:** 2026-07-29
- **Program:** program P42

> **Superseded.** `crates/optimus-engineering` was removed from the workspace on
> 2026-08-01, never having been integrated by any consumer. Nothing below is
> rewritten: the reasoning is preserved because it, not the code, is what a
> future attempt would need.

## Context

A run that ends with a green suite has established that the tests pass. It has
not established that any of them would have caught the bug it claims to fix.
Those are different claims, and only the second one supports "this closes issue
N".

The gap is not hypothetical. A model asked to fix a bug and add a regression
test will frequently produce a test that exercises the *fixed* behaviour
without exercising the *bug* — asserting what the new code does rather than
what the old code got wrong. Such a test passes at base. It would not have
caught the bug, and it will not catch its return. Every signal available from a
single-commit run says this patch is good.

[ADR-0052](0052-isolated-durable-engineering-runs.md) already separated
corroboration from exit status, and `PhaseStep::expect_failure` already exists
for a step that proves its point by failing. What was missing is the thing that
makes the failure meaningful: running the *same* test at the *base* commit.

## Decision

A fix is verified differentially. The regression test runs at the base commit
and on the patch, and only **fail-then-pass** counts.

The procedure has a step that is easy to omit and fatal to omit:

1. Cut a **detached** checkout at the base SHA.
2. Carry **only the named test files** across from the patch worktree — never
   the fix. The test does not exist at base, and a harness asked to run a test
   that is not there exits non-zero, which from outside is indistinguishable
   from the test failing. That false red is the most convincing wrong answer
   available here; carrying the test is what removes it.
3. Confirm the base checkout is dirty in *exactly* the carried paths. Anything
   else means the fix came along and every verdict from that tree is void.
4. Run at base. It must fail.
5. Run on the patch. It must pass.

Four combinations, four named verdicts, and one of them proves the fix:

| base | patch | verdict |
|---|---|---|
| fail | pass | `Proven` |
| pass | pass | `TestPassesWithoutTheFix` — the test is not testing the bug |
| fail | fail | `NotFixed` |
| pass | fail | `PatchBrokeIt` |

Plus a fifth state that is not a combination at all. **`Inconclusive` is not
`NotFixed`.** A base run that timed out, did not build, or ran in a checkout
that carried more than the tests never reached the test. Its red exit status
looks exactly like a genuine failure, and reading it as one would manufacture a
proof out of a broken build. This is the same third state
[ADR-0053](0053-a-repository-is-asked-not-assumed.md) gives branch protection,
for the same reason: *could not tell* is not *answered no*.

`proves_the_fix()` is written as `matches!(self, Self::Proven)` rather than as
a negation, so a new verdict variant is a compile error rather than a silent
default to "good enough".

**Detecting "never ran" is a heuristic, and it is allowed to be.**
`NEVER_RAN_MARKERS` matches build-failure output. Like the impact selector's
rules ([ADR-0054](0054-a-selector-may-only-over-select.md)), the markers can
only move a verdict *toward* `Inconclusive`, never toward `Proven`. A build
break at base that matches no marker is read as a genuine failure — the
residual risk, named here rather than papered over.

**A deadline that does not bound wall time is not a deadline.** Building this
surfaced a defect in `ProcessRunner`: the timeout killed the child but not its
children, and a surviving grandchild held the stdout pipe open, so the drain
never reached EOF. `sh -c 'sleep 30'` under a 300ms deadline returned after 30
seconds — correctly flagged `timed_out`, and 100× late. On Unix the child now
leads its own process group and the deadline signals the group.

## Alternatives considered

**`git stash` the fix and re-run in place.** Rejected. It mutates the run's own
worktree, so an interruption mid-proof leaves the fix stashed and the tree
looking finished-and-broken. A separate detached checkout cannot damage the
work.

**Revert the fix by applying the reverse diff.** Rejected. It requires deciding
which hunks are "the fix" and which are "the test" — the same judgement call
the carry step makes, but performed on a patch rather than on files, where a
mistake is much harder to see.

**Trust `expect_failure` on a step run in the patch worktree.** Rejected. That
proves a command fails on the patch, which is a different and much weaker
statement. Nothing about it involves the base commit.

**Merge `Inconclusive` into `NotFixed`.** Rejected — it is the same conflation
as treating an unreachable forge as an unprotected branch, and it fails toward
"proof" rather than away from it.

**Skip the carry and run the base checkout as-is.** Rejected, and this is the
subtle one: it produces `Proven` for every new test, because the test is
missing at base and the harness exits non-zero. It would look like it worked.

## Reasons

- The one thing a regression test must do is fail before the fix. Nothing else
  in a run establishes that.
- Naming four verdicts instead of pass/fail means each refusal comes with the
  reason a human needs, not just a red mark.
- The states that cannot be distinguished honestly are given their own name
  rather than being assigned to the nearest neighbour.

## Consequences

- E40.9's verification phase can require a `Proven` verdict before a run claims
  a fix, and refuse `TestPassesWithoutTheFix` explicitly rather than by
  silence.
- Base checkouts live under the runs directory, are detached (so a run cannot
  push from one), and are cut fresh every time — a stale one from an
  interrupted proof would carry whatever the last attempt copied in.
- A caller must name the test paths. This is deliberate: the component will not
  guess which parts of a patch are "the test", because guessing wrong carries
  the fix.
- Every timed-out step across the crate now actually stops at its deadline, not
  just the differential ones.

## Risks

- **Marker coverage.** A base build failure whose output matches no marker
  reads as a genuine test failure and yields a false `Proven`. Mitigated by the
  asymmetry (markers only add caution) but not eliminated.
- **Carrying the wrong file.** A caller that names a source file in
  `test_paths` carries the fix to base, and the proof degrades to
  `TestPassesWithoutTheFix` — a refusal, so it fails safe, but for a reason
  that reads as a bad test rather than a bad request.
- **Grandchild kill is Unix-only.** On Windows the deadline still kills only
  the leader; the fallback is documented in `kill_group` rather than assumed
  away.

## Evaluation evidence

`crates/optimus-engineering/tests/differential_proof.rs` — 14 tests against
real git, real worktrees and real child processes. The "bug" is a line in a
file; the "regression test" is a shell script that reads it.

- `a_test_that_fails_at_base_and_passes_on_the_patch_is_proven`
- `a_test_that_passes_without_the_fix_is_refused` — the case that matters: a
  real fix, a real test, a green suite, refused anyway
- `a_fix_that_does_not_fix_it_is_refused` /
  `a_patch_that_breaks_a_passing_test_is_named_as_such`
- `a_base_run_that_never_reached_the_test_is_inconclusive_not_proven`
- `a_base_run_that_times_out_is_inconclusive` — asserts wall time under 10s for
  a 300ms deadline against `sleep 30`, which is the process-group regression
- `the_base_checkout_gets_the_test_but_never_the_fix` — reads the base
  checkout's source file back and asserts it still contains the bug
- `the_base_checkout_is_detached_so_a_run_cannot_push_from_it`
- `a_stale_base_checkout_is_not_reused`
- `a_test_path_cannot_climb_out_of_the_patch_worktree`

Plus 7 unit tests in `differential.rs` covering the never-ran classifier and
the verdict contract.

## Conditions for reconsideration

- A harness that reports test counts machine-readably, which would replace the
  marker heuristic with a real "did the test execute" signal.
- A repository whose tests cannot run from a second checkout (absolute paths,
  a single shared build lock), where the base run needs a different isolation
  strategy.
- Evidence that `Inconclusive` dominates in practice, meaning the carry set is
  usually too small to make the base tree build.

## Relevant code

- `crates/optimus-engineering/src/differential.rs`
- `crates/optimus-engineering/src/command.rs` — `kill_group`, the process group

## Relevant tests

- `crates/optimus-engineering/tests/differential_proof.rs`
