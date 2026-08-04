---
doc_id: decisions-0053-a-repository-is-asked-not-assumed
doc_type: decision
plane: decision
status: historical
authority: record
summary: "Superseded by ADR-0073 (2026-08-01) together with the optimus-engineering crate. Records why an engineering run resolves a repository's defaults, verification commands and sensitive-path floor from git and the tree rather than assuming them, and why a repository may raise but never weaken its own floor."
reviewed_on: 2026-08-01
review_by: never
knowledge_type: decision
depends_on:
  - docs/decisions/0031-safe-project-work-loop.md
  - docs/decisions/0044-bounded-project-trust-and-capability-broker.md
  - docs/decisions/0052-isolated-durable-engineering-runs.md
  - docs/decisions/0073-an-unreachable-vertical-is-archived-not-carried.md
---

# ADR-0053: A repository is asked, not assumed

- **Status:** Accepted 2026-07-29 — superseded 2026-08-01 by [ADR-0073](0073-an-unreachable-vertical-is-archived-not-carried.md)
- **Date:** 2026-07-29
- **Program:** program P41

> **Superseded.** `crates/optimus-engineering` was removed from the workspace on
> 2026-08-01, never having been integrated by any consumer. Nothing below is
> rewritten: the reasoning is preserved because it, not the code, is what a
> future attempt would need.

## Context

An engineering run ([ADR-0052](0052-isolated-durable-engineering-runs.md)) has a
worktree, a base SHA, and a phase, but it does not know what the repository it
is working in expects of it. Four questions come up in every run:

- What is the default branch?
- Is it protected, and by which checks?
- Which commands verify a change — quickly, and completely?
- Which instruction files govern the file being edited, and is there a PR
  template to fill in?

Today those answers are reconstructed inside a prompt. That fails in a specific
and unpleasant direction: a model asked "is `main` protected?" with no access to
the forge produces a confident sentence, not an error. The failure mode is not
"the run stops"; it is "the run continues on invented facts", and the invented
facts are exactly the ones that decide whether a change gets checked.

Reconstruction also fails silently in the good case. A repository whose gate is
`just verify` and a repository with no task runner at all both yield a plausible
guess. Only one of them is right, and nothing in the run distinguishes them.

## Decision

Repository facts are resolved once, from their actual sources, into a
`RepositoryPolicyProfile`. Git answers the default branch, the forge answers
protection, the task runner answers verification, the tree answers instruction
files and PR templates. Three rules govern what happens when a source does not
answer.

**1. Absent is not satisfied.** A branch with no protection ruleset resolves to
`BranchProtection::Unprotected` — a recorded fact — never to "requirements met
because there are none". `required_checks()` returns empty for every non-
`Protected` state, so a caller that iterates required checks cannot accidentally
read "no checks to satisfy" as "all checks satisfied" without also having asked
which state it is in.

**2. Unknown is not absent.** If the forge could not be asked — expired token,
missing `gh`, DNS failure, rate limit — the result is
`BranchProtection::Unknown { reason }`, and `is_determined()` is false.
Collapsing that into `Unprotected` would turn an expired credential into a green
light, which is the single most dangerous conflation available here. The same
rule applies to the default branch: an unresolvable one stays `None` rather than
becoming `"main"`.

**3. A repository cannot weaken its own floor.** `SENSITIVE_FLOOR` — `.github/**`,
`scripts/verify.sh`, `scripts/check-*.py`, the justfile, instruction files,
`.optimus/**`, keys and `.env` files — is unioned with declared sensitive paths,
never replaced by them. Declared verification commands are accepted only when
non-empty, so a repository cannot declare its way to "nothing to run". The first
thing a bad patch would otherwise do is edit the file that decides whether
patches get checked.

Detection follows the same discipline as resolution: `resolve_verification`
looks for real recipes in `just --summary` and picks the first that exists. A
repository with no recipes reports no verification rather than inventing one, and
`VerificationCommands::can_verify()` is how a caller finds out. Every field that
could not be resolved is named by `unresolved()`, so a caller can refuse to
proceed on a partial profile instead of discovering the gap later.

Instruction files resolve root-first so the nearest one wins, and a relative path
that climbs out of the repository resolves to nothing rather than to files above
the root.

**This module carries no authority.** It reports what a repository says. It does
not decide whether a push is allowed, whether review is required, or whether a
phase advances — those remain with the broker
([ADR-0044](0044-bounded-project-trust-and-capability-broker.md)) and the phase
controller ([ADR-0052](0052-isolated-durable-engineering-runs.md)). Keeping
resolution separate from enforcement is what makes `Unknown` safe to represent:
a reporter that cannot grant anything cannot grant something by mistake.

## Alternatives considered

**Ask the model.** Rejected. The failure mode is invented facts stated
confidently, and there is no signal in the output that distinguishes a recalled
fact from a fabricated one.

**Two states — protected or not.** Rejected. Every unreachable-forge case then
has to be assigned to one of them. Assigning it to `Unprotected` makes a broken
token look like a permissive repository; assigning it to `Protected` blocks all
work whenever the network hiccups. The third state is the only honest option.

**Parse `.optimus/policy.toml` now.** Deferred. No crate in the workspace parses
TOML, and adding a dependency to read a file format nothing else uses is cost
without a caller. `DeclaredPolicy` is taken as a struct, precedence is fully
tested, and file parsing is a marked follow-on that adds a parser to a resolved
contract rather than designing both at once.

**Let declared configuration replace the sensitive floor.** Rejected outright.
It is a self-disabling safety mechanism.

## Reasons

- An invented repository fact is worse than a missing one, because a missing one
  stops the run and an invented one steers it.
- Three states cost one enum variant and remove a whole class of silent
  downgrade.
- Resolving once and reporting `unresolved()` lets each caller choose its own
  strictness instead of hard-coding one policy into the resolver.
- Separating reporting from enforcement means a bug here cannot widen
  permissions; the worst it can do is refuse to proceed.

## Consequences

- E40.9 (phase step catalogue) can source its commands from
  `RepositoryPolicyProfile::verification` instead of hard-coding `just` recipes.
- P46 (elevated review) has a sensitive-path predicate — `is_sensitive` — that a
  repository cannot shrink.
- Callers must handle `Unknown` explicitly. This is deliberate friction: code
  that ignores it will fail to compile against the enum rather than quietly
  treat it as `Unprotected`.
- A profile can be partially resolved. `unresolved()` names which fields, and it
  is each caller's decision whether that is fatal.

## Risks

- **Forge shape drift.** `parse_protection` reads the GitHub branch-protection
  payload. A schema change degrades to `Unknown` rather than to a wrong answer,
  which is the correct failure direction, but it degrades silently until someone
  notices runs are blocked.
- **Detection heuristics age.** The focused and full recipe candidate lists are
  ordered guesses at convention. A repository whose gate has an unconventional
  name reports no verification. Declared commands are the escape hatch.
- **Timeout tuning.** `QUERY_TIMEOUT` is 30s. Too short strands slow forges in
  `Unknown`; too long stalls a phase. There is no evidence yet for a better
  number.

## Evaluation evidence

`crates/optimus-engineering/tests/repository_profile.rs` — 13 tests, plus one
`#[ignore]`d test that resolves this repository against the live forge.

The tests that carry the decision:

- `a_branch_with_no_ruleset_resolves_to_unprotected_not_to_satisfied`
- `a_forge_that_cannot_be_asked_is_unknown_and_never_unprotected` — 401, 403,
  and DNS failure each produce `Unknown`, not `Unprotected`
- `a_missing_gh_leaves_protection_unknown_rather_than_absent`
- `an_unknown_default_branch_does_not_become_main`
- `a_repository_with_no_recipes_reports_no_verification_rather_than_inventing_one`
- `a_repository_can_add_sensitive_paths_but_not_remove_the_floor`
- `an_instruction_path_cannot_climb_out_of_the_repository`

Run against this repository, the ignored test resolves: default branch `main`,
protection `Unprotected` (determined — `main` genuinely has no ruleset),
PR template `.github/pull_request_template.md`, instructions `AGENTS.md` and
`CLAUDE.md`, focused `just gates`, full `just verify`, `unresolved()` empty.

## Conditions for reconsideration

- A caller needs a repository fact that is not in the profile, and threading it
  through `DeclaredPolicy` is worse than adding a field.
- A forge other than GitHub is supported, at which point `resolve_protection`
  needs a seam rather than a `gh` invocation.
- Evidence that `Unknown` is being hit often enough in normal operation to be
  treated as a workflow problem rather than a fault signal.

## Relevant code

- `crates/optimus-engineering/src/repository.rs` — resolution, the three rules,
  and the sensitive floor.
- `crates/optimus-engineering/src/lib.rs` — re-exports.

## Relevant tests

- `crates/optimus-engineering/tests/repository_profile.rs`
- `crates/optimus-engineering/src/repository.rs` — unit tests for glob matching
  and instruction ordering.
