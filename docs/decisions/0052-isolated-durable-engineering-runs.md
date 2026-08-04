---
doc_id: decisions-0052-isolated-durable-engineering-runs
doc_type: decision
plane: decision
status: historical
authority: record
summary: "Superseded by ADR-0073 (2026-08-01): the optimus-engineering crate was removed unintegrated. Records the original design for isolated durable development-task runs — a sixteen-phase state machine advanced by recorded evidence, one git worktree per run, and the kernel-enforced session boundary that is retained."
reviewed_on: 2026-08-01
review_by: never
knowledge_type: decision
covers:
  - crates/optimus-kernel/src/project_authority.rs
depends_on:
  - docs/decisions/0073-an-unreachable-vertical-is-archived-not-carried.md
  - docs/decisions/0009-durable-sessions.md
  - docs/decisions/0021-owned-execution-and-causal-delivery.md
  - docs/decisions/0031-safe-project-work-loop.md
  - docs/decisions/0033-multi-agent-dag-execution.md
  - docs/decisions/0035-command-capability-envelope.md
  - docs/decisions/0044-bounded-project-trust-and-capability-broker.md
validated_by:
  - crates/optimus-kernel/tests/dev_run_containment.rs
---

# ADR-0052: Engineering runs are isolated, phased, and resumable

- **Status:** Accepted 2026-07-29 — superseded 2026-08-01 by [ADR-0073](0073-an-unreachable-vertical-is-archived-not-carried.md)
- **Date:** 2026-07-29
- **Program:** program P40

> **Superseded.** `crates/optimus-engineering` was removed from the workspace on
> 2026-08-01, never having been integrated by any consumer. Nothing below is
> rewritten: the reasoning is preserved because it, not the code, is what a
> future attempt would need. §2's kernel-side containment — a session bound to a
> worktree cannot reach outside it — was retained and re-anchored on ADR-0073.

## Context

Optimus can already authorize ordinary project work without a permission wall
([ADR-0044](0044-bounded-project-trust-and-capability-broker.md)), record every
effect exactly ([ADR-0031](0031-safe-project-work-loop.md)), and run a durable
multi-agent DAG ([ADR-0033](0033-multi-agent-dag-execution.md)).

It cannot yet run a *development task*. A development task is not a turn and not
a DAG node: it is a long-lived unit of engineering work that owns a branch, a
base commit, a body of evidence, and a position in a delivery process. Today
that unit exists only as a chat session, which means:

- There is no recorded base SHA, so "what did this run change" is answered by
  reading a dirty working tree rather than by a diff against a known point.
- There is no phase, so nothing prevents a run from writing a patch before it
  has a reproduction, or from calling itself finished after one green unit test.
- There is no exit criterion, so long runs decay into unstructured conversation
  and stop when the context window does, not when the work is done.
- There is no resume point, so an interrupted twenty-minute investigation is
  lost rather than continued.
- The agent edits the shared main checkout, which makes concurrent human work
  unsafe and makes isolation of a failed attempt impossible.

The product-facing symptom is that Optimus cannot be trusted with an issue. The
engineering symptom is that every run rediscovers the same repository context
and produces evidence that nobody can audit afterwards.

## Decision

### 1. A development task is a durable object, not a conversation

Introduce `DevTaskRun` in a new `crates/optimus-engineering`. It persists the
identity and the whole history of one engineering task:

| Field | Purpose |
|---|---|
| `task_id` | Stable identity across restart, resume, and fork |
| `origin` | Issue reference or explicit human request |
| `repo_root` | Resolved canonical project root |
| `base_sha` | Commit the run branched from |
| `branch` / `worktree_path` | The run's isolated checkout |
| `phase` | Current state machine position |
| `plan` | Accepted acceptance criteria and change scope |
| `evidence` | Append-only record: commands, results, diffs, findings |
| `budget` | Consumed retries, wall time, model cost |
| `stop_reason` | Why the run is not currently advancing |

The record is append-only for evidence and monotonic for phase. A run that
crashes mid-phase resumes from its last checkpoint, not from the beginning.

### 2. Every run owns an isolated checkout

Before any implementation phase, a run is given:

- a dedicated branch cut from a recorded `base_sha`;
- a dedicated `git worktree` under a repo-local runs directory;
- a recorded baseline verification result for that worktree;
- separate logs and artifacts, addressed by `task_id`.

**The main checkout is never written by an engineering run.** The worktree path
becomes the run's project root binding for
[ADR-0031](0031-safe-project-work-loop.md) containment; a run cannot escape into
the main checkout or a sibling run's worktree, and existing project-bleed gates
apply to the worktree exactly as they apply to the main root.

Worktrees are created, reused on resume, and removed on completion or on
explicit abandonment. An abandoned worktree that still has uncommitted work is
retained and reported, never silently discarded.

### 3. Phase progression is enforced by code, not by the model

```text
INTAKE → TRIAGE → INVESTIGATE → PLAN → PREPARE_WORKTREE → IMPLEMENT
       → FOCUSED_VERIFY → REVIEW → REPAIR → FULL_VERIFY → READY_TO_PUBLISH
       → PUBLISHED → WAITING_FOR_CI → ADDRESSING_FEEDBACK → READY_TO_MERGE
       → COMPLETE
```

Transitions are a fixed table in Rust. The model chooses *what* to do inside a
phase and may propose an exit; it cannot choose which phase comes next, cannot
skip a phase, and cannot re-enter a completed phase except through the declared
repair edges (`REVIEW → REPAIR → FOCUSED_VERIFY`,
`WAITING_FOR_CI → ADDRESSING_FEEDBACK → FOCUSED_VERIFY`).

`BLOCKED` and `ABANDONED` are terminal-ish states reachable from any phase; both
record the originating phase so a resume knows where to return.

### 4. Every phase declares its contract

Each phase carries, as data:

- **allowed capabilities** — resolved through the existing broker
  ([ADR-0044](0044-bounded-project-trust-and-capability-broker.md)); no second
  permission plane;
- **required evidence** — what must be recorded before the phase may exit;
- **maximum retries** and **timeout**;
- **exit criteria** and the **failure state** taken when they are not met;
- a **resume checkpoint**.

`INVESTIGATE` cannot write project files. `IMPLEMENT` cannot push. `REVIEW` is
read-only. These are properties of the phase, not instructions in a prompt.

### 5. Evidence, not assertion, advances a phase

A phase exits when its required evidence exists in the run record. A model
statement that tests passed is not evidence; a recorded command invocation with
its exit status, output digest, and the worktree SHA it ran against is. The
controller reads the record, not the transcript.

Consequently `FOCUSED_VERIFY` cannot be satisfied by a green unit test alone
when the phase contract requires a differential result, and `FULL_VERIFY`
cannot be satisfied by anything except a complete recorded `just verify`.

Evidence records **corroboration separately from exit status**, because the two
come apart. The differential proof runs the new regression test against the
base commit, where it must *fail* — a test that passes without the fix is not
testing the bug. So a step declares which outcome would prove its point, the
record keeps the raw exit status either way, and the phase contract is
satisfied by corroboration rather than by exit zero. This is the one place
where "the command failed" is the evidence, and inverting it silently would let
a test that catches nothing count as a proof.

### 6. The implementation model does not approve its own patch

`REVIEW` runs in a separate context with read-only authority and receives the
issue, the acceptance criteria, the base SHA, the final diff, and the test
evidence. Its findings enter the record as structured items with severity. A
run cannot reach `READY_TO_PUBLISH` while an unresolved finding above the
configured severity threshold exists.

### 7. Publication is a separate authority

Push, draft-PR creation, and merge are **not** in the auto-authorized set for
any autonomy profile. They remain broker decisions that ask, and they carry the
concrete consequence in the prompt (`Publish branch wip/fix-123 to GitHub`), not
an infrastructure-level question. `READY_TO_PUBLISH` is where an autonomous run
stops by default.

## Alternatives considered

**Reuse the Work Graph DAG for engineering tasks.**
[ADR-0033](0033-multi-agent-dag-execution.md) already gives durable multi-agent
execution, and an engineering run is superficially a DAG. Rejected: a DAG node
is a unit of *execution*, and its edges are data dependencies. An engineering
phase is a unit of *authority and evidence*, and its edges are policy. Encoding
"REVIEW is read-only and cannot be re-entered after REPAIR without a fresh
FOCUSED_VERIFY" as DAG edges hides a policy table inside a scheduler. The run
may still *use* the DAG to execute work inside a phase.

**Let the model drive the phases from a prompt.** Cheapest to build and the
common industry pattern. Rejected: it is exactly the failure this ADR exists to
fix. A model that can choose its next phase can choose to skip verification, and
under retry pressure it will.

**Branch without a worktree; use `git stash` / checkout switching on the main
tree.** Rejected: it serialises all engineering work behind one working tree,
loses uncommitted state on any crash, and makes concurrent human editing unsafe.
The disk cost of a worktree is small next to a `target/` directory that is
already shared.

**A separate clone per task instead of a worktree.** Rejected: a clone
duplicates object storage and breaks shared build caches, which directly
attacks the inner-loop speed this program exists to protect.

**Store the run record in the existing session store.** Deferred, not rejected:
a run outlives many sessions and must be readable when no session is open. The
record's home is decided in E40.3; the requirement here is that it is durable
and append-only, not which table it lands in.

## Reasons

1. **Authority belongs in code.** Every capability that a model can talk itself
   out of is a capability that will eventually be talked out of. Phases,
   retries, and publication authority are deterministic for the same reason the
   broker is.
2. **A base SHA turns opinion into diff.** Almost every unanswerable question
   about an agent run ("what did it actually change", "did the test really fail
   before") becomes mechanical once a run records where it started.
3. **Isolation is what makes failure cheap.** A failed attempt in its own
   worktree is deleted. A failed attempt in the main checkout is an incident.
4. **Resumability is worth more than speed here.** An interrupted twenty-minute
   investigation that resumes is strictly better than a faster one that must
   restart, because investigation cost dominates implementation cost.
5. **The loop must be measurable to compound.** A durable record is the
   precondition for every metric in program P40–P46; without it, improvement
   claims are anecdotes.

## Consequences

- One new crate, `optimus-engineering`, sits above `optimus-policy` and below
  the kernel. It shells out to `git` directly; the repository is the only
  external system it touches in this ADR.
- Engineering runs become inspectable and comparable. `fork_attempt` and
  `compare_attempts` are possible because two attempts differ only by branch,
  worktree, and record.
- Concurrency is bounded by worktrees rather than by hope: two runs cannot
  collide in the working tree.
- Cost: every engineering run now pays for a worktree (disk, one checkout) and
  for a durable record. Worktrees are removed on completion; the record is not.

## Risks

| Risk | Mitigation |
|---|---|
| Worktrees accumulate and consume disk | Removal on completion; dirty worktrees retained but reported, never silently kept forever |
| A worktree drifts from the main checkout's toolchain or caches | Worktrees share the repository, `rust-toolchain.toml`, and the cargo/npm caches; no separate clone |
| The phase table ossifies and blocks legitimate work | Transitions are data, versioned with the run record; a run pinned to an old table still resumes |
| `BLOCKED` becomes a dumping ground that hides failures | `BLOCKED` records the originating phase and a stop reason; a run with no stop reason is invalid |
| Evidence requirements become a checkbox a model learns to satisfy vacuously | Evidence is a recorded command invocation with exit status, output digest, and the SHA it ran against — not a model assertion. Program P46 adds adversarial checks on the evidence itself |
| Isolation is assumed rather than enforced | The worktree path is the ADR-0031 root binding; project-bleed gates run against it |

## Evaluation evidence

Not yet gathered — this ADR is accepted on design grounds and is falsifiable by
the program P40 exit gate, which requires:

- a run killed mid-`IMPLEMENT` resuming in `IMPLEMENT` against the same worktree
  and base SHA;
- a proven-impossible `IMPLEMENT → READY_TO_PUBLISH` transition;
- a proof that no engineering run writes the main checkout.

The decision is wrong if those tests cannot be written cheaply, or if worktree
setup measurably slows the inner loop.

## Conditions for reconsideration

Revisit if any of the following holds:

- worktree creation or cache sharing costs more than a few seconds per run,
  making isolation a throughput tax rather than a safety property;
- the phase table needs a new escape hatch more than about once a month, which
  would indicate the phases are cutting across the real work instead of along it;
- run records grow large enough that resume becomes slower than restart;
- a later decision gives the Work Graph genuine authority semantics, at which
  point the two spines should merge rather than coexist.

## Relevant code

- `crates/optimus-engineering/src/phase.rs` — phase table, transitions, contracts
- `crates/optimus-engineering/src/run.rs` — `DevTaskRun`, evidence, budget
- `crates/optimus-engineering/src/worktree.rs` — branch and worktree lifecycle
- `crates/optimus-engineering/src/command.rs` — `CommandRunner`, the injected
  execution surface, and `ProcessRunner`, its default child-process form
- `crates/optimus-engineering/src/controller.rs` — `RunDriver`, which earns
  evidence by running commands and never judges whether a phase is finished
- `crates/optimus-kernel/src/project_authority.rs` — `dev_run_scope`, which
  narrows an authorized project scope to one run's worktree
- `crates/optimus-kernel/src/lib.rs` — `Kernel::open_dev_run_session`
- `crates/optimus-policy/src/lib.rs` — capability broker (unchanged by this ADR)

## Relevant tests

- `crates/optimus-engineering/tests/phase_progression.rs`
- `crates/optimus-engineering/tests/driver_earns_evidence.rs`
- `crates/optimus-engineering/tests/worktree_lifecycle.rs`
- `crates/optimus-engineering/tests/resume_after_interrupt.rs`
- `crates/optimus-kernel/tests/dev_run_containment.rs`
- `scripts/gates/check-crate-layers.py`, `scripts/gates/check-project-bleed.py`

## Explicit non-claims

This ADR does **not** claim:

- GitHub reads or writes of any kind (checks, workflow runs, job logs, draft
  PRs, review threads) — those are later program P40 phases;
- impact-based test selection or differential verification *implementations* —
  this ADR only says the phase contract may require their evidence;
- that the reviewer role is model-independent, only that it is context- and
  authority-independent from the implementer;
- any change to autonomy profile defaults or to the broker's decision table;
- that a run may merge. Merge stays behind explicit human approval.
