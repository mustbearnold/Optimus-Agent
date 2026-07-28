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
  - scripts/github_pr_branch.py
  - docs/decisions/0052-isolated-durable-engineering-runs.md
covers:
  - docs/plans/github-engineer-program.md
depends_on:
  - docs/decisions/0031-safe-project-work-loop.md
  - docs/decisions/0033-multi-agent-dag-execution.md
  - docs/decisions/0044-bounded-project-trust-and-capability-broker.md
  - docs/decisions/0052-isolated-durable-engineering-runs.md
  - docs/plans/reliability-autonomy-program.md
  - docs/plans/product-complete-program.md
validated_by:
  - scripts/check-crate-layers.py
  - scripts/check-project-bleed.py
  - scripts/check-module-size.py
last_verified_commit: null
---

# GitHub Engineer program — P40–P46

**Execution authority for making Optimus able to develop Optimus.** This
program supersedes feature-breadth work as the primary roadmap driver until
GITHUB-ENGINEER-V1 is reached.

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
| Delivery | `PR #N` / `pr/N-…` | GitHub (never force PR# = phase) |

`P40` ≠ `PR #40` ≠ `ADR-0040`. `E40.1` is a microtask, never a grade.

## The objective

> Convert one well-scoped GitHub issue in this repository into one
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
item of this program and is **half built**. The remaining P30 microtasks are
prerequisites for P40's exit, not separate work:

| ID | Status | Item |
|---|---|---|
| R30.5 | pending | Durable project trust grant store (outside repo) |
| R30.6 | pending | Structured package-manager capabilities |
| R30.7 | pending | Owned-localhost network lease |
| R30.8 | pending | Product release defaults without breaking offline tests |

Permission prompts must state the consequence (`Publish branch wip/fix-123 to
GitHub`), never the mechanism (`Allow shell command?`).

## Phase map

| Phase | Goal | Status |
|---|---|---|
| **program P40** | Isolated, durable, phased engineering runs | **in progress** |
| **program P41** | Repository policy resolution + issue triage | pending |
| **program P42** | Fast informative verification: impact selection + differential regression | pending |
| **program P43** | Separated navigator / implementer / test-specialist / reviewer roles + model routing | pending |
| **program P44** | GitHub delivery: safe push, evidence-backed draft PR, CI diagnosis | pending |
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
| E40.7 | pending | R30.5–R30.8 landed (prerequisite above) |
| E40.8 | **done** | `RunDriver` drives a run through the table, recording evidence from real commands |
| E40.9 | pending | Phase step catalogue: the actual `just` commands each phase runs (needs `test-changed`, P42) |

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
- `python3 scripts/check-crate-layers.py`
- `python3 scripts/check-project-bleed.py`
- `python3 scripts/check-module-size.py`
- A run interrupted by process kill during `IMPLEMENT` resumes in `IMPLEMENT`
  against the same worktree and base SHA, proven by test
- A run cannot transition `IMPLEMENT → READY_TO_PUBLISH`, proven by test
- The main checkout is never written by an engineering run, proven by test

### Explicit non-claims (P40)

- No GitHub API access of any kind
- No impact-selected or differential verification implementation
- No model routing changes
- No merge authority

---

## program P41 — Repository policy resolution + issue triage

Stop reconstructing repository facts inside prompts.

### Microtasks

| ID | Status | Item |
|---|---|---|
| E41.1 | pending | `RepositoryPolicyProfile`: default branch, branch rules, required checks, PR template, merge policy, code ownership. An **absent** protection ruleset resolves to "unprotected", recorded as such — never silently to "satisfied" |
| E41.2 | pending | Resolve effective `AGENTS.md` set and sensitive-file list into the profile |
| E41.3 | pending | Resolve focused and full verification commands into the profile |
| E41.4 | pending | `TRIAGE` output contract: problem statement, evidence, acceptance criteria, owning components, relevant tests, risk class, change scope, stop condition |
| E41.5 | pending | Reject or split issues that are too vague or too large, with a recorded reason |

### Exit gate (P41)

- Profile resolves for this repository with zero prompt-reconstructed fields
- Ten historical issues produce acceptance criteria a human accepts without edit
  in at least eight cases

---

## program P42 — Fast, informative verification

A slow gate reduces useful development cycles per day.

### Microtasks

| ID | Status | Item |
|---|---|---|
| E42.1 | pending | `just dev-check` — very fast static and targeted checks |
| E42.2 | pending | `just test-changed` — impact-selected tests for the current patch |
| E42.3 | pending | Impact engine: changed symbol → importers → package → unit/integration/UI tests |
| E42.4 | pending | Differential regression verification: prove the new test fails at base SHA and passes on the patch |
| E42.5 | pending | Per-stage duration and failure-rate telemetry |
| E42.6 | pending | Cache work: `sccache`, shared cargo/npm caches, reused Playwright browsers |

### Exit gate (P42)

- `just test-changed` selects a superset of the tests that actually fail for a
  seeded regression, across ten seeded cases
- Median focused-verification wall time recorded and lower than `just test`
- Differential verification refuses a "fix" whose test also passes at base SHA

---

## program P43 — Separated roles + model routing

| ID | Status | Item |
|---|---|---|
| E43.1 | pending | Read-only repository navigator producing the impact map |
| E43.2 | pending | Implementer with project write authority, no push/merge/approve |
| E43.3 | pending | Test specialist: adversarial regression coverage, confirms the test catches the original failure |
| E43.4 | pending | Independent reviewer, read-only, structured findings with severity and evidence |
| E43.5 | pending | Controller owns phase, budget and policy; writes no implementation code |
| E43.6 | pending | Model routing table: high effort for root cause / architecture / final review; medium for normal implementation; cheap for classification and summarization; deterministic code for all authority |

### Exit gate (P43)

- Implementation and review contexts are provably separate
- Cost per accepted task recorded by model and task class

---

## program P44 — GitHub delivery

Expert-level GitHub use, not minimum viable. Precise APIs over scraping:
`gh --json`/`--jq` for structured reads, GraphQL where REST cannot reach,
`gh run view --log-failed` rather than whole-log retrieval.

| ID | Status | Item |
|---|---|---|
| E44.1 | pending | Safe branch push behind an explicit consequence-stating approval; never rename or delete a remote PR head (GitHub closes the PR) |
| E44.2 | pending | Draft-PR creation with head-SHA and repository confirmation; PR number comes from GitHub, is never chosen |
| E44.3 | pending | PR body from `.github/pull_request_template.md` filled only from recorded run evidence; unsupported claims rejected |
| E44.4 | pending | Issue linkage with closing keywords, and canonical labels from `.github/labels.yml` applied to both issue and PR |
| E44.5 | pending | Read checks, workflow runs and **failed job logs only** (`gh pr checks`, `gh run view --log-failed`) |
| E44.6 | pending | Classify failures: introduced, flake, environmental, stale — keyed on the head SHA the check ran against |

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
