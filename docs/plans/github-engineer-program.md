---
knowledge_type: plan
status: current
owns:
  - docs/plans/github-engineer-program.md
watches:
  - crates/optimus-engineering/**
  - crates/optimus-policy/src/lib.rs
  - crates/optimus-workflow/**
  - justfile
  - docs/decisions/0052-isolated-durable-engineering-runs.md
  - docs/decisions/0053-a-repository-is-asked-not-assumed.md
  - docs/decisions/0054-a-selector-may-only-over-select.md
  - docs/decisions/0055-a-fix-is-proven-at-the-commit-it-fixes.md
  - docs/decisions/0056-a-reviewer-that-wrote-the-patch-is-not-a-reviewer.md
  - docs/decisions/0057-an-issue-earns-its-way-into-a-run.md
  - docs/decisions/0058-a-run-publishes-the-sentence-a-human-approved.md
covers:
  - docs/plans/github-engineer-program.md
depends_on:
  - docs/decisions/0031-safe-project-work-loop.md
  - docs/decisions/0033-multi-agent-dag-execution.md
  - docs/decisions/0044-bounded-project-trust-and-capability-broker.md
  - docs/decisions/0052-isolated-durable-engineering-runs.md
  - docs/decisions/0053-a-repository-is-asked-not-assumed.md
  - docs/decisions/0054-a-selector-may-only-over-select.md
  - docs/decisions/0055-a-fix-is-proven-at-the-commit-it-fixes.md
  - docs/decisions/0056-a-reviewer-that-wrote-the-patch-is-not-a-reviewer.md
  - docs/decisions/0057-an-issue-earns-its-way-into-a-run.md
  - docs/decisions/0058-a-run-publishes-the-sentence-a-human-approved.md
  - docs/plans/reliability-autonomy-program.md
  - docs/plans/product-complete-program.md
validated_by:
  - scripts/check-crate-layers.py
  - scripts/check-project-bleed.py
  - scripts/check-module-size.py
last_verified_commit: null
---

# GitHub Engineer program — P40–P46

> **Installed-product capability roadmap.** This specifies how Optimus Agent
> may perform engineering work for a selected project. It is not a VCS or
> workflow instruction for humans, Codex, Claude, or other coding agents
> developing this repository.

**Execution authority for making the Optimus product capable of completing
durable engineering runs.** This program supersedes feature-breadth work as
the primary product roadmap driver until GITHUB-ENGINEER-V1 is reached.

| Authority | Document | Role |
|---|---|---|
| **This program** | `github-engineer-program.md` | Phase exit gates P40–P46 → GITHUB-ENGINEER-V1 |
| Isolation decision | [ADR-0052](../decisions/0052-isolated-durable-engineering-runs.md) | Durable phased runs in isolated worktrees |
| Trust decision | [ADR-0044](../decisions/0044-bounded-project-trust-and-capability-broker.md) | Capability broker; prerequisite grants |
| Architecture hold | [architecture-marks.md](../architecture/architecture-marks.md) | Marks stay S+++ |
| Parked breadth | [reliability-autonomy-program.md](./reliability-autonomy-program.md) | P31–P35 resume after P46 |
| Closed | [product-complete-program.md](./product-complete-program.md) | P20–P29 PRODUCT-COMPLETE; residuals only |

## Naming planes (mandatory)

Always say **program P40** (etc.) in new prose.

| Plane | Token | Authority |
|---|---|---|
| Program | **program** `P40`…`P46` | this document |
| Plan microtask | `E40.1`…`E46.n` | this document |
| Decision | `ADR-NNNN` | `docs/decisions/` |
| Repository implementation delivery | managed task id + full SHA on `origin/main` | `just land` |
| Product forge artifact | issue / draft PR produced by an installed Optimus run | product run record + forge |

`P40` ≠ repository task id ≠ product `PR #40` ≠ `ADR-0040`. `E40.1` is a
microtask, never a grade.

## The objective

> Convert one well-scoped GitHub issue in a selected repository into one
> independently reviewed, fully verified **draft** pull request with minimal
> human involvement.

Not "add the most features". Every phase below exists because the loop above
cannot close without it.

## What is frozen

Deprioritized until GITHUB-ENGINEER-V1, maintained but not roadmap-driving:

1. Additional messaging platforms and channel depth
2. Voice capabilities
3. Provider breadth beyond actively used providers
4. Plugin marketplaces and Hermes feature-for-feature parity
5. Large visual redesigns and speculative architecture without an observed blocker

Critical reliability work continues. Breadth no longer controls the roadmap.

## Prerequisite (carried from program P30)

Project-scoped automatic permission for routine engineering work is the first
item of this program and has one remaining prerequisite. The remaining P30
microtask is a prerequisite for P40's exit, not separate work:

| ID | Status | Item |
|---|---|---|
| R30.5 | **done** | Durable project trust grant store (outside repo) |
| R30.6 | **done** | Structured package-manager capabilities |
| R30.7 | in progress | Owned-localhost network lease — exact boundary and default-inactive live registry landed; verified serve/issuer/browser lifecycle remains |
| R30.8 | **done** | Auto provider/model release defaults preserve deterministic offline tests |

Permission prompts must state the consequence (`Publish branch wip/fix-123 to
GitHub`), never the mechanism (`Allow shell command?`).

## Phase map

| Phase | Goal | Status |
|---|---|---|
| **program P40** | Isolated, durable, phased engineering runs | **in progress** |
| **program P41** | Repository policy resolution + issue triage | **items done** (E41.1–E41.5; exit-gate measurement waits on E43.1) |
| **program P42** | Fast informative verification: impact selection + differential regression | **in progress** (E42.1–E42.4 done) |
| **program P43** | Separated navigator / implementer / test-specialist / reviewer roles + model routing | **in progress** (E43.4–E43.6 done) |
| **program P44** | GitHub delivery: safe push, evidence-backed draft PR, CI diagnosis | **in progress** (E44.1–E44.2 done, E44.3 core done) |
| **program P45** | Feedback ingestion + durable recovery and replay | pending |
| **program P46** | Anti-weakening enforcement + historical-task benchmark → GITHUB-ENGINEER-V1 | pending |

---

## program P40 — Isolated, durable, phased engineering runs

**Decision:** [ADR-0052](../decisions/0052-isolated-durable-engineering-runs.md)

Nothing later in this program is testable without a run object to hang it on.
This phase builds the spine.

### Microtasks

| ID | Status | Item |
|---|---|---|
| E40.1 | **done** | ADR-0052 accepted |
| E40.2 | **done** | `crates/optimus-engineering`: `DevPhase` table, transitions, phase contracts |
| E40.3 | **done** | `DevTaskRun` record: identity, base SHA, plan, append-only evidence, budget, stop reason |
| E40.4 | **done** | Branch + worktree lifecycle: create, reuse on resume, remove on completion, retain-and-report on dirty abandon |
| E40.5 | **done** | Worktree path becomes the run's ADR-0031 root binding (`Kernel::open_dev_run_session`) |
| E40.6 | **done** | Durable persistence + `resume` from last checkpoint after process restart |
| E40.7 | partial | R30.5 + R30.6 + R30.8 landed; R30.7 (localhost lease) remains open |
| E40.8 | **done** | `RunDriver` drives a run through the table, recording evidence from real commands |
| E40.9 | **done** | Phase step catalogue (`catalogue::plan_for`): the commands each phase runs, sourced from `RepositoryPolicyProfile::verification` (E41.3) rather than hard-coded |

**What E40.7 gives a run, and what it deliberately withholds.** A run inside a
worktree the user already authorized no longer pauses on every ordinary write:
`Kernel::open_dev_run_session` reads the project's durable trust grant and
adopts its profile. That is the *only* place a grant is read — a chat session
on the same project still asks, because authorizing engineering runs is not the
same statement as "stop showing me edits". The grant also stopped covering
things it never should have: `cargo install ripgrep` and `npm install -g` now
classify as host changes rather than project execution, so a project-scoped
grant does not quietly become a host-scoped one. Verified by
`crates/optimus-kernel/tests/dev_run_trust.rs` (the same write pauses without a
grant and lands with one) and `crates/optimus-policy/tests/command_classification.rs`.

**Where a phase's commands come from (E40.9).** `catalogue::plan_for` turns a
phase plus a resolved `RepositoryPolicyProfile` into the steps that phase runs.
Two distinctions it refuses to collapse. *"No command to run" is not "nothing
needs running"*: a phase whose evidence is a problem statement or a human
approval legitimately has no steps, while a phase that should run the
repository's gate in a repository that never named one has no steps and is
blocked — both would be an empty `Vec`, so they are separate fields and
`can_drive()` is false only for the second. And *a differential proof is never a
step*: one command at one commit cannot establish that a test fails at base and
passes on the patch, so `FocusedVerify` declares `DifferentialProof` as owed
elsewhere rather than emitting a plausible single command that would let the
phase satisfy its hardest contract with its easiest step. A run's full gate is
never substituted with its focused one. Verified against this repository by
`crates/optimus-engineering/tests/phase_catalogue.rs` — real `just --summary`,
real `git`, only `gh` stubbed.

**A distinction E40.8 forced into the model.** Evidence now records whether an
observation *corroborated*, separately from its exit status. The two are not
the same: the differential proof runs the new regression test against the base
commit, where it must **fail** — a test that passes without the fix is not
testing the bug. So `EvidenceItem::observed` (must pass) and
`observed_failing` (must fail) are distinct, and `satisfied_evidence` filters
on corroboration rather than on exit zero. A green run at base is recorded and
proves nothing.

### Exit gate (P40)

- `cargo test -p optimus-engineering`
- `cargo test -p optimus-kernel --test dev_run_containment`
- `cargo test -p optimus-kernel --test dev_run_trust`
- `cargo test -p optimus-engineering --test phase_catalogue` — a run started in
  this repository can name every command its phases need
- `python3 scripts/check-crate-layers.py`
- `python3 scripts/check-project-bleed.py`
- `python3 scripts/check-module-size.py`
- A run interrupted by process kill during `IMPLEMENT` resumes in `IMPLEMENT`
  against the same worktree and base SHA, proven by test
- A run cannot transition `IMPLEMENT → READY_TO_PUBLISH`, proven by test
- The main checkout is never written by an engineering run, proven by test

### Explicit non-claims (P40)

- No GitHub API access of any kind
- ~~No impact-selected or differential verification implementation~~ — landed
  in P42 (E42.1–E42.4) ahead of this phase closing, because E40.9 needed the
  commands to exist before it could name them
- No model routing changes
- No merge authority

---

## program P41 — Repository policy resolution + issue triage

Stop reconstructing repository facts inside prompts. The decision and its three
honesty rules are recorded in
[ADR-0053](../decisions/0053-a-repository-is-asked-not-assumed.md).

### Microtasks

| ID | Status | Item |
|---|---|---|
| E41.1 | **done** | `RepositoryPolicyProfile`: default branch, three-state protection, required checks, PR template. An **absent** ruleset resolves to `Unprotected` — recorded as such, never silently to "satisfied" |
| E41.2 | **done** | Effective `AGENTS.md`/`CLAUDE.md` chain and sensitive-path floor resolved into the profile |
| E41.3 | **done** | Focused and full verification commands resolved into the profile |
| E41.4 | ✅ done | `TRIAGE` output contract: problem statement, evidence, acceptance criteria, owning components, relevant tests, risk class, change scope, stop condition |
| E41.5 | ✅ done | Reject or split issues that are too vague or too large, with a recorded reason |

**Three states, not two (E41.1).** `Unprotected` means the forge answered and
there is no ruleset. `Unknown` means the forge was not reachable — no `gh`, no
network, expired token, insufficient permission. Collapsing the second into the
first is how an expired token becomes a green light, so a non-404 failure never
resolves to "unprotected" and `required_checks()` is empty for `Unknown` without
that emptiness reading as "nothing is required".

**A repository cannot weaken its own floor (E41.2).** Declared configuration is
*unioned* with the built-in sensitive set — `.github/**`, `scripts/verify.sh`,
`scripts/check-*.py`, the justfile, every `AGENTS.md`/`CLAUDE.md`, `.optimus/**`,
key and env files — and can never subtract from it. Otherwise the first thing a
bad patch does is edit the file that decides whether patches get reviewed. This
is program P46 §1's protection, enforced at resolution time rather than at
review time.

**Detection never invents (E41.3).** Declared commands win; otherwise recipes
are read from the repository's own task runner. If neither yields anything the
field stays `None` and `unresolved()` says so. A run that reports "verified"
from a command nobody declared has proven nothing. `focused = []` in
configuration resolves to no command, not to "no checks needed".

**Nothing here carries authority.** `DeclaredPolicy` can name commands and add
sensitive paths. It has no field for credentials, outside-project access, or an
autonomy profile, so ADR-0044 Decision 5 is enforced by the type rather than by
validation — a field that cannot be written cannot be abused.

**A verdict blames the triage, never the issue (E41.4/E41.5).**
[ADR-0057](../decisions/0057-an-issue-earns-its-way-into-a-run.md): triage
produces a checkable contract or a refusal, and a deterministic checker decides
admissibility. Quotes must be findable in the issue body, named components must
exist, a component under the sensitive floor cannot be filed `Low` risk, a stop
condition that restates a criterion is refused, and past 3 components / 6
criteria / 20 files the verdict names the remedy: a `too_large` refusal with a
proposed split of at least two parts. A refusal is held to the same standard —
"too vague" needs the reporter's own words, so a model cannot close a report by
inventing what it said. `Admissible` means *nothing here is demonstrably
wrong*, not that the criteria are right; that judgement is E43.1's navigator
and, at the exit gate, a human. `evidence_drafts` takes the verdict as an
argument and refuses everything except `Admissible`, so unchecked triage output
has no path into the run record.

### Exit gate (P41)

- `cargo test -p optimus-engineering --test repository_profile` — 13
- Profile resolution is proven against neutral temporary-repository fixtures,
  including optional pull-request templates, labels, and either `AGENTS.md` or
  `CLAUDE.md`; this repository's own development ceremony is not a product
  fixture.
- Ten historical issues produce acceptance criteria a human accepts without edit
  in at least eight cases — **contract built** (E41.4/E41.5,
  `cargo test -p optimus-engineering --test triage_contract` — 6); the
  measurement itself needs E43.1's navigator to produce the ten triages

---

## program P42 — Fast, informative verification

A slow gate reduces useful development cycles per day. The decision and its
four rules are recorded in
[ADR-0054](../decisions/0054-a-selector-may-only-over-select.md).

### Microtasks

| ID | Status | Item |
|---|---|---|
| E42.1 | **done** | `just dev-check` — static gates plus the tests this patch can break. `just impact` reports the selection without running anything |
| E42.2 | **done** | `just test-changed` — impact-selected tests, non-zero when nothing is selected |
| E42.3 | **done** | Impact engine (`scripts/impact_select.py`): changed path → package → reverse-dependency closure → packages and non-cargo suites |
| E42.4 | **done** | Differential regression verification (`DifferentialProver`): the new test runs at the base SHA with only the test carried across, and only fail-then-pass proves the fix |
| E42.5 | pending | Per-stage duration and failure-rate telemetry |
| E42.6 | pending | Cache work: `sccache`, shared cargo/npm caches, reused Playwright browsers |

**Over-selection is the only safe error (E42.3).** A selector that runs too
much costs seconds. A selector that runs too little reports *success* from a
run that never executed the failing test — the signal and the bug are
indistinguishable from outside. So an unclassified path escalates to the whole
workspace, one unclassified path escalates the whole plan, and a change to the
justfile, `verify.sh`, any `check-*.py`, the selector itself, the manifests or
`.github/**` always selects everything. The cheapest way to make a patch pass
must not be to edit the thing that decides what passing means.

**Nothing selected is not a pass (E42.2).** An empty selection reports
`nothing-selected` and exits non-zero under `--require-selection`. This is
P41's *absent is not satisfied* one layer down: "no tests ran" and "the tests
passed" are different sentences.

**Impact is read, not remembered (E42.3).** The closure is computed by
inverting the workspace manifests, including dev- and build-dependencies, so a
change to `optimus-store` reaches `optimus-cli` without anything stating that
path. A hand-maintained table would be correct the day it was written and wrong
thereafter — `.engineering-memory/source-to-test-map.json` says as much in its
own `limitations` field.

**`just verify` is untouched.** Focused verification serves the inner loop; the
pre-push hook still runs all 38 checks. A wrong answer in the selector costs
cycles, never a missed regression at the boundary that matters.

**A green suite is not a proof (E42.4).** It establishes that the tests pass,
not that any of them would have caught the bug. A regression test that passes
at the base commit would not have caught it and will not catch its return, and
every signal from a single-commit run says the patch is good. So the test runs
at base too — with **only the test** carried across, never the fix, because a
harness asked to run a test that is not there exits non-zero and that false red
is the most convincing wrong answer available. Four combinations, four named
verdicts, one of which proves the fix; and a fifth state, `Inconclusive`, for a
base run that timed out or did not build. Reading that as a genuine failure
would manufacture a proof out of a broken build — P41's *unknown is not absent*,
again. Recorded in
[ADR-0055](../decisions/0055-a-fix-is-proven-at-the-commit-it-fixes.md).

### Exit gate (P42)

- ✅ `just test-changed` selects a superset of the tests that actually fail for
  a seeded regression, across ten seeded cases —
  `test_every_seeded_regression_is_selected`, plus the exhaustive
  `test_every_workspace_package_selects_itself`
- ✅ Median focused-verification wall time recorded and lower than `just test` —
  warm cache, `cargo test -p optimus-engineering --all-targets` **0.67s**
  against `cargo test --workspace --all-targets` **24.9s**
- ✅ Differential verification refuses a "fix" whose test also passes at base
  SHA — `a_test_that_passes_without_the_fix_is_refused`

---

## program P43 — Separated roles + model routing

| ID | Status | Item |
|---|---|---|
| E43.1 | pending | Read-only repository navigator producing the impact map |
| E43.2 | pending | Implementer with project write authority, no push/merge/approve |
| E43.3 | pending | Test specialist: adversarial regression coverage, confirms the test catches the original failure |
| E43.4 | ✅ done | Independent reviewer, read-only, structured findings with severity and evidence |
| E43.5 | ✅ done | Controller owns phase, budget and policy; writes no implementation code |
| E43.6 | ✅ done | Model routing table: high effort for root cause / architecture / final review; medium for normal implementation; cheap for classification and summarization; deterministic code for all authority |

E43.4–E43.6 are the *boundary*, not the agents that fill it.
[ADR-0056](../decisions/0056-a-reviewer-that-wrote-the-patch-is-not-a-reviewer.md)
turns "the implementation model does not approve its own patch" from a sentence
in ADR-0052 into a refusal inside `DevTaskRun::record`. Asserted evidence now
carries the role **and the context** that asserted it; `ReviewFindings` from a
context that produced a `Diff` in the same run is refused before it reaches the
log, and the set of diff authors accumulates, so a repair author is still an
author when review comes round again. Changing the role label does not help,
which is the whole point — prompting cannot fix a problem where the instruction
and the violation are written by the same process.

Command outcomes are exempt, deliberately. `just verify` exiting zero carries
its command, its commit, its exit status and a digest of its output; who
pressed enter changes none of them. Drawing the line at "is there a command
behind this?" puts the check exactly where models enter and nowhere else — and
it is what lets the rule be strict, because everything it applies to is
something a model said.

E43.6 lands as `routing_for(phase)` returning a role and an `Effort`, not a
model name. This crate cannot reach the router and should not: picking a
provider is the kernel's decision, made with telemetry this crate never sees.
What the phase table knows — and what a router is missing today — is which work
is expensive to get wrong.

E43.1–E43.3 are the contexts that fill these roles by calling a model. Until
they exist, a single-context run *stalls* at `REVIEW` rather than
self-approving. That is the intended behaviour, and it is visible now rather
than after a PR claims a review that never happened.

### Exit gate (P43)

- ✅ Implementation and review contexts are provably separate —
  `crates/optimus-engineering/tests/role_separation.rs`, 10 tests;
  `the_context_that_wrote_the_patch_cannot_review_it` and
  `changing_the_label_does_not_change_the_reasoning` are the two that matter
- ⬜ Cost per accepted task recorded by model and task class — needs E43.1–E43.3
  and P42's E42.5 telemetry

---

## program P44 — GitHub delivery

Expert-level GitHub use, not minimum viable. Precise APIs over scraping:
`gh --json`/`--jq` for structured reads, GraphQL where REST cannot reach,
`gh run view --log-failed` rather than whole-log retrieval.

| ID | Status | Item |
|---|---|---|
| E44.1 | ✅ done | Safe branch push behind an explicit consequence-stating approval; never rename or delete a remote PR head (GitHub closes the PR) |
| E44.2 | ✅ done | Draft-PR creation with head-SHA and repository confirmation; PR number comes from GitHub, is never chosen |
| E44.3 | core done | PR body from `.github/pull_request_template.md` filled only from recorded run evidence; unsupported claims rejected — evidence-only rendering landed with E44.1/E44.2; template-section mapping still pending |
| E44.4 | pending | Issue linkage with closing keywords, and canonical labels from `.github/labels.yml` applied to both issue and PR |
| E44.5 | pending | Read checks, workflow runs and **failed job logs only** (`gh pr checks`, `gh run view --log-failed`) |
| E44.6 | pending | Classify failures: introduced, flake, environmental, stale — keyed on the head SHA the check ran against |

E44.1/E44.2 land as `delivery.rs`, recorded in
[ADR-0058](../decisions/0058-a-run-publishes-the-sentence-a-human-approved.md).
The consequence-stating approval this program's preamble demands now has a
mechanism: a `PublishPlan` renders its consequence as one sentence — commit,
branch, repository, base — and the human's yes is recorded as `HumanApproval`
evidence whose summary **is** that sentence. Publishing refuses unless the
record holds those exact words, and because the sentence embeds the commit, a
worktree that moved after approval produces a different sentence and the old
approval covers nothing. The push itself publishes the approved commit as the
refspec source (`<sha>:refs/heads/<branch>`), not the branch tip, so there is
no gap between what was approved and what lands.

**"Never rename or delete a remote PR head" is unconstructible, not
policed.** The refspec is built, never accepted; a branch name that would
smuggle a second meaning — a colon, a leading `-` or `+`, emptiness, a
wildcard — is refused at plan construction with the reason named; there is no
force field and no delete function. And no receipt is believed on exit status
alone: `git ls-remote` must report the approved commit at the branch, `gh pr
view --json` must report that head on the created PR, and only the confirmed
pair corroborates — the differential-proof shape from P42, applied to effects
on a forge. The PR number is parsed from GitHub's own output and confirmed
against `gh pr view`; output without one is a refusal, not a guess.

E44.3's core landed with it: `pr_body.rs` renders the PR body from the run
record and nowhere else. There is no prose parameter — every claim line cites
the evidence row that backs it, items that did not corroborate never render as
achievements, and "unsupported claims rejected" is not a checker but the
absence of anything to write an unsupported claim with. What remains of E44.3
is mapping the rendered record onto `.github/pull_request_template.md`'s own
sections; a template's sections are a request for prose, and filling them
honestly needs more record structure than a run holds today.

### Exit gate (P44)

- A draft PR is produced end to end from one issue with no hand-written prose
- Every claim in the body traces to a recorded evidence item
- The PR carries type/area/size/risk labels and a closing reference to its issue
- No GitHub read in the loop retrieves a whole log where a filtered one exists

### Repository hygiene this phase depends on

Measured 2026-07-29 against `mustbearnold/Optimus-Agent`. These are
prerequisites for the phase, not opinions about tidiness:

| Gap | Why it blocks the loop |
|---|---|
| `main` has **no branch protection and no required checks** | E41.1 resolves "required checks" to an empty set, so a draft PR cannot be judged mergeable by anything except a human, and program P46's "removed required checks" detector has nothing to protect |
| Open issues **113, 106, 105, 99 carry no labels** | Triage (E41.4) and routing key off `area:` and `risk:`; unlabelled issues fall back to reading the whole repository, which is the cost this program exists to remove |
| One workflow (`verify.yml`), no merge queue | CI classification (E44.6) has a single signal, so "introduced vs environmental" rests on log text alone |

Each requires an explicit human decision (repository settings, and writes to
issues) and is therefore **not** something an Optimus run may do for itself.

---

## program P45 — Feedback ingestion + durable recovery

| ID | Status | Item |
|---|---|---|
| E45.1 | pending | Read PR review threads via **GraphQL** (`reviewThreads`), resolve them explicitly; REST cannot see or resolve a thread |
| E45.2 | pending | Convert actionable comments into bounded repair tasks; re-run focused and full verification with exact head-SHA tracking |
| E45.3 | pending | `resume`, `retry_phase`, `retry_with_stronger_model`, `fork_attempt`, `compare_attempts`, `hand_to_human` |
| E45.4 | pending | Failure replay from the persisted record |
| E45.5 | pending | Cost and timing telemetry per run |

### Exit gate (P45)

- A run survives Optimus restart, desktop restart, model failure, tool failure
  and CI delay without losing its investigation

---

## program P46 — Anti-weakening enforcement + benchmark

Any self-modifying system will eventually find that weakening evaluation is
cheaper than satisfying it, unless prevented structurally.

### Microtasks

| ID | Status | Item |
|---|---|---|
| E46.1 | pending | Elevate review automatically when a patch touches `.github/**`, verification scripts, test runners, coverage config, permissions, policy, architecture gates, agent instructions, credential handling, or generated-memory validation |
| E46.2 | pending | Flag deleted tests, new skips/ignores, widened allowlists, reduced assertions, weakened coverage, raised timeouts without evidence, snapshot rewrites, removed required checks, broadened permissions |
| E46.3 | pending | Require a reviewer outside the implementation run for all of the above |
| E46.4 | pending | Historical-task benchmark harness over ten selected past issues |

### Exit gate (P46) — GITHUB-ENGINEER-V1

Ten historical issues (five straightforward bugs, two UI bugs, one CI failure,
one architecture-sensitive change, one cross-component defect). **At least eight
must complete** the full sequence:

1. Read one issue
2. Produce valid acceptance criteria
3. Identify the correct code and tests
4. Create a clean isolated worktree
5. Implement the change
6. Add a regression test
7. Prove the test catches the prior failure
8. Pass focused verification
9. Survive an independent review
10. Repair valid findings
11. Pass full verification
12. Produce a correct draft PR
13. State risks and uncertainty honestly
14. Resume if interrupted
15. Avoid weakening its own evaluation system

## Success metric

```text
accepted merge-ready tasks
────────────────────────────────
human correction time + model cost
```

Tracked alongside: draft-PR completion rate, first-push CI pass rate, median
human interventions per task, review findings per patch, escaped defects,
unrelated-diff percentage, test-selection precision, recovery rate after
interruption, percentage of runs stopped by permission friction, cost by model
and task class, revert rate after merge.

**Not** tracked as success: commits, lines changed, tool calls, token volume.

## Rules (non-negotiable)

1. Models reason; code controls authority. Permissions, branch rules, required
   checks, scope, retry ceilings, diff limits and merge eligibility are
   deterministic.
2. No engineering run writes the main checkout.
3. The implementation model never approves its own patch.
4. A green unit test alone is never completion.
5. At most three concurrent development lanes: one compounding-capability task,
   one observed reliability defect, one high-value user friction item.
6. Architecture marks stay S+++; product speed never demotes a mark.

## Explicit non-claims

This program does not claim merge autonomy, CI authorship, cross-repository
operation, or that Optimus outperforms generic coding agents on repositories it
has not indexed.

## Immediate next action

1. Land **program P40** — `optimus-engineering` phase table, run record and
   worktree lifecycle (this wave).
2. Then **program P41** repository policy resolution.
