---
doc_id: decisions-0073-an-unreachable-vertical-is-archived-not-carried
doc_type: decision
plane: decision
status: current
authority: record
summary: The optimus-engineering crate is removed from the tree because nothing in the workspace could reach it; ADRs 0052, 0053, 0055, 0056, 0057 and 0058 are superseded and kept in place rather than deleted, ADR-0054 stays current because its implementation was never in the crate, and the kernel's 50-line dev-run containment primitive is retained on its own security merit.
reviewed_on: 2026-08-01
review_by: 2026-11-01
knowledge_type: decision
covers:
  - Cargo.toml
  - docs/repository-components.json
  - crates/optimus-kernel/src/dev_run.rs
  - crates/optimus-kernel/src/project_authority.rs
depends_on:
  - docs/decisions/0052-isolated-durable-engineering-runs.md
  - docs/decisions/0068-a-catalog-row-must-dispatch-or-not-exist.md
validated_by:
  - crates/optimus-kernel/tests/dev_run_containment.rs
  - crates/optimus-kernel/tests/dev_run_trust.rs
  - scripts/tests/test_repository_ontology.py
---

# ADR-0073: An unreachable vertical is archived, not carried

- **Status:** Accepted
- **Date:** 2026-08-01

## Context

`crates/optimus-engineering` was 9,917 lines across 25 files — fourteen source
modules and ten integration tests — implementing the durable development-task
substrate designed in [ADR-0052](0052-isolated-durable-engineering-runs.md) and
extended by five later decisions. It was well built. It was also unreachable:
no crate, no app, and no binary in this workspace depended on it. Its only
mention outside its own directory was a doc comment in a kernel test.

Its component row already recorded the honest state — `lifecycle: incubating`,
`distribution: repository-only`, summary "Unintegrated durable development-task
substrate" — together with exactly two exits and a deadline:

> `removal_when`: It is integrated through an approved product route or its ADR
> series is explicitly superseded.
> `review_by`: 2026-10-31

That deadline is not decorative. `validate_database` in
`scripts/tools/repository_ontology.py` raises `lifecycle review expired on <date>` for
an incubating row past its review date, so the repository-wide gate turns red on
2026-11-01 whether or not anyone decides. The row was written by someone who
understood that an unmade decision is still a decision, and arranged for it to
stop being free.

The cost of carrying it was never the disk. It was that six accepted ADRs and a
program of prose described a vertical that a reader could reasonably believe
shipped. [ADR-0068](0068-a-catalog-row-must-dispatch-or-not-exist.md) settled
the general form of this for tool catalogs: a declared row that does not
dispatch teaches a false affordance and spends prompt budget for nothing. A
crate is the same claim at larger scale. An agent orienting in this repository
read seven ADRs' worth of engineering-run design and found nothing that could
invoke it — and the repository had to keep a dedicated orientation-eval case
whose whole job was to correct the impression its own documentation created.

The competitive audit recorded this as B-ARCH-02 and named the two exits the
component row already named. This ADR takes the second one.

## Decision

### 1. The crate is removed from the tree

`crates/optimus-engineering` is deleted, along with its workspace `members`
entry, its `[workspace.dependencies]` entry, and its component-database row.
The layer rule in `scripts/gates/check-crate-layers.py` that kept it below the control
plane is removed with the crate it constrained.

Removal is from the working tree, not from history. The crate remains readable
at any commit before this one, and the reasoning that produced it remains in
`docs/decisions/` — see clause 3.

### 2. Six ADRs are superseded; one is not

Superseded by this decision, and marked `status: historical`:

| ADR | Bound implementation |
|---|---|
| 0052 | `crates/optimus-engineering/src/{phase,run,worktree,command,controller}.rs` |
| 0053 | `crates/optimus-engineering/src/repository.rs` |
| 0055 | `crates/optimus-engineering/src/differential.rs` |
| 0056 | `crates/optimus-engineering/src/roles.rs` |
| 0057 | `crates/optimus-engineering/src/triage.rs` |
| 0058 | `crates/optimus-engineering/src/{delivery,publish_plan,pr_body}.rs` |

**[ADR-0054](0054-a-selector-may-only-over-select.md) is not superseded and
remains current.** It sits inside the same numeric run and reads as part of the
same series, but its decision was never implemented in the crate: it covers
`scripts/tools/impact_select.py` and the `justfile`, and it is validated by
`scripts/tests/test_impact_select.py`. That selector is live, gated, and used by every
`just check` in this repository. Superseding it because of its neighbours would
have retired a working invariant on the strength of an adjacency.

The audit counted seven ADRs for this vertical. Six is the correct number.

### 3. Superseded ADRs are kept, not deleted

Each superseded ADR keeps its Context, Decision, Alternatives, Reasons, Risks
and non-claims exactly as written. Only three things change: the front-matter
`status` becomes `historical`, the `summary` and body `## Status` record the
supersession and point here, and source bindings that name deleted files are
dropped so the documentation contract binds to what exists.

This is the repository's standing rule — decisions preserve history and are not
rewritten to hide superseded reasoning — and it is also the retention
mechanism. A future implementer does not need the crate to recover the design;
they need the argument, and the argument is what the ADRs are.

### 4. The kernel's containment primitive is retained on its own merit

`crates/optimus-kernel/src/dev_run.rs` (50 lines) and `dev_run_scope` in
`project_authority.rs` are kept, with `dev_run_containment.rs` and
`dev_run_trust.rs`. ADR-0052 §2 deliberately split this in two: the crate
created the worktree, the kernel proved that binding a session to a worktree
actually narrows what its tools can touch. Only the first half was
crate-shaped. The second is a general property — a session may be confined to a
subtree of an authorized project and cannot escape into the main checkout — and
it is worth the same whether or not an engineering run is the thing being
confined. It re-anchors on this ADR rather than on the superseded one.

### 5. What is retained is the reasoning, named

The durable ideas from the superseded set, recorded here so a future attempt
starts from the argument rather than rediscovering it:

- **A base SHA turns opinion into diff** (0052). Almost every unanswerable
  question about an agent run — what did it change, did the test really fail
  before — becomes mechanical once the run records where it started.
- **Evidence, not assertion, advances a phase** (0052 §5). A model statement
  that tests passed is not evidence; a recorded invocation with exit status,
  output digest and the SHA it ran against is. Corroboration is recorded
  *separately* from exit status, because the differential proof needs a command
  that fails.
- **A fix is proven at the commit it fixes** (0055). The regression test runs at
  the base commit with only the test carried across; a base run that never
  reached the test is `Inconclusive`, not `NotFixed`.
- **A reviewer that wrote the patch is not a reviewer** (0056). Asserted
  evidence carries the role *and the context* that asserted it. Command
  outcomes are exempt: a process exit status makes no claim.
- **Absent is not satisfied; unknown is not absent** (0053). A repository's
  defaults, verification commands and sensitive-path floor are resolved from
  git and the tree, and a repository cannot weaken its own floor.
- **An issue earns its way in, or is refused in the reporter's own words**
  (0057). Triage produces a checkable contract or a grounded refusal; closing
  an issue takes an explicit refusal held to the same evidentiary standard.
- **Approval is the exact consequence sentence** (0058). The recorded approval
  is the sentence describing the effect, character for character; the push
  publishes `<sha>:refs/heads/<branch>` rather than the branch tip, so a commit
  that landed after approval stays local; delete, rename and force are
  unconstructible rather than filtered.

Three of these are already load-bearing elsewhere in this repository and are
therefore not at risk: worktree isolation and a recorded base are how
`just worktree-new` and managed delivery already work; over-selection is
ADR-0054's live selector; and the approval-sentence discipline is the shape
SmartDeny already enforces for durable effects.

## Alternatives considered

**Integrate the crate through a product route.** The other exit the component
row named. Rejected for now on cost and on honesty about what integration would
require: a named product surface, a program to build it, and — for the P44
publication phases — a GitHub read/write path this repository does not have and
whose ceremony its own delivery contract forbids. Choosing integration would
have meant committing a program to a vertical whose demand is still unproven,
which is how the crate reached 9,917 unreachable lines in the first place.

**Extend `review_by` and keep incubating.** Cheapest action, and the one the
gate was designed to make uncomfortable. Rejected: the row has been incubating
since it was written, and a deadline that moves whenever it arrives is not a
deadline. Nothing about the situation would have been different in three months
except the size of the thing not being decided.

**Delete the ADRs along with the code.** Rejected on the standing rule and on
utility. The code is recoverable from history and would need rewriting anyway
against a future runtime; the design argument is the expensive part and does not
rot the same way.

**Keep the crate, delete only the ADRs.** Rejected as the worst of both: code
with no caller and no recorded reason to exist.

## Reasons

1. **A declared thing that cannot be reached teaches a false affordance.**
   ADR-0068's finding for catalog rows applies unchanged to a crate: the reader
   pays attention proportional to what is declared, not to what dispatches.
2. **Unreachable code cannot rot loudly.** Nothing in the workspace compiles
   against it, so no gate would notice the day its assumptions stopped matching
   the kernel it was designed beside. Its tests would stay green while its
   design silently went stale — the self-serving green the north-star criteria
   exist to ban.
3. **The decision was going to be forced anyway.** The only question the
   2026-10-31 deadline left open was whether it would be made deliberately or
   under a red gate.
4. **The knowledge was never in the code.** Six ADRs of reasoning survive this
   change intact; what is lost is an implementation that would need rewriting
   against whatever runtime eventually wanted it.

## Consequences

- The workspace has 18 members rather than 19. `optimus-engineering` no longer
  appears in `Cargo.toml`, `docs/repository-components.json`, the generated
  catalog views, or the crate-layer rules.
- The orientation eval loses its `engineering-is-incubating` case, because the
  misconception it corrected no longer has a subject. A case asserting the
  removal replaces it, so an agent that has read stale material is corrected by
  the same mechanism.
- `scripts/tests/test_impact_select.py` no longer uses the crate as its leaf-package
  fixture; the ADR-0054 selector it validates is unchanged.
- The `2026-10-31` incubation deadline is discharged. No component row is
  waiting on a lifecycle decision.
- Recovering the implementation means reading history at or before this
  decision's delivery SHA. That is a deliberate cost, accepted because the
  design record does not require it.

## Risks

| Risk | Mitigation |
|---|---|
| A future engineering-run feature restarts from zero | The six ADRs stay in `docs/decisions/` with their reasoning intact, and clause 5 names the load-bearing invariants explicitly |
| The kernel dev-run primitive becomes orphaned prose | It is re-anchored on this ADR, and its two test files are this ADR's `validated_by` bindings |
| Someone reads a superseded ADR as current design | Front-matter `status: historical`, a supersession line at the top of each body, and a pointer here |
| The archive is mistaken for a judgement that the design was wrong | It was not. The design is preserved because it is worth preserving; what failed was integration, not reasoning |

## Evaluation evidence

- `crates/optimus-kernel/tests/dev_run_containment.rs` and
  `dev_run_trust.rs` — the retained containment property still holds with the
  crate gone.
- `scripts/tests/test_repository_ontology.py` — the component database validates with
  no row for the removed crate and no unclassified path left behind.
- `just verify` — the full gate passes with 18 workspace members.

## Conditions for reconsideration

Reconsider when a *product* surface wants durable development tasks — a user
asking Optimus to take an issue and return a reviewed change. At that point the
question is not whether to restore this crate but whether the runtime that
exists then wants a separate engineering spine at all, or whether the Work Graph
has by then acquired the authority semantics ADR-0052 said it lacked. Restoring
9,917 lines written against a 2026-07 kernel would be the wrong starting move
either way; the ADRs are the right one.

## Relevant code

- `Cargo.toml` — workspace membership
- `docs/repository-components.json` — component authority
- `crates/optimus-kernel/src/dev_run.rs` — retained session-to-worktree binding
- `crates/optimus-kernel/src/project_authority.rs` — `dev_run_scope`
- `scripts/gates/check-crate-layers.py` — layer rules, with the crate's rule removed

## Relevant tests

- `crates/optimus-kernel/tests/dev_run_containment.rs`
- `crates/optimus-kernel/tests/dev_run_trust.rs`
- `scripts/tests/test_repository_ontology.py`
- `evals/repository-orientation/questions-v1.json`

## Explicit non-claims

This ADR does **not** claim:

- that durable development tasks are a bad idea, or that Optimus will not have
  them;
- that ADR-0054's over-selection invariant is affected in any way;
- that the kernel's dev-run session binding is deprecated — clause 4 retains it
  deliberately;
- that the removed implementation was defective. It was unintegrated, which is
  a different failure and the only one being acted on here.
