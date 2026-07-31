---
doc_id: decision-0062-source-and-development-are-separate-workspace-planes
doc_type: decision
plane: decision
status: current
authority: canonical
summary: Decision record for ADR-0062: Source and Development are separate workspace planes, including its context, consequences, and current documentary status.
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: decision
covers:
  - AGENTS.md
  - justfile
  - scripts/workspace_layout.py
  - scripts/managed_delivery.py
  - scripts/project_hygiene.py
depends_on:
  - docs/decisions/0052-isolated-durable-engineering-runs.md
validated_by:
  - scripts/test_workspace_layout.py
  - scripts/test_managed_delivery.py
  - scripts/test_project_hygiene.py
---

# ADR-0062: Source and Development are separate workspace planes

- **Status:** Accepted
- **Date:** 2026-07-31

> **Current naming (2026-08-01):** the clean view is now `Repository/`, not
> `Source/`. The original name below is preserved as decision history.
> `Repository/` means the complete reproducible GitHub content—product source,
> tests, evaluation definitions, docs, and build logic. `Development/` remains
> machine-local worktrees, evidence, caches, tools, and delivery records.

## Context

The Optimus project root accumulated two different kinds of state in one view:
the source people expect to find on GitHub, and machine-local development state
such as a bare Git control store, linked worktrees, immutable land receipts,
downloaded tools, build output, raw test evidence, caches, and stale source
snapshots. The mixed root made stale files look authoritative and made it hard
for a person or coding agent to tell where work should happen.

Moving or deleting pieces ad hoc is unsafe. Linked-worktree pointers contain
absolute paths, managed checkpoint identity depends on the invoking worktree,
and the delivery system must remain the only path to remote `main`.

## Decision

The host workspace wrapper has exactly two user-facing planes:

1. `Source/` is a clean detached linked worktree at the locally verified
   `origin/main`. It is for reading and orientation, never development.
2. `Development/` owns the bare Git store, isolated agent worktrees, managed
   delivery records, tools, raw evidence, caches, build artifacts, and a
   recoverable archive of the former mixed-root snapshot.

Development continues only in `Development/worktrees/*`. `just land` remains
the only main-delivery path. Compatibility links `.git -> Development/git` and
`local -> Development` preserve existing absolute worktree pointers and older
automation during the transition.

The migration is a fail-closed repository command. It requires a clean assigned
worktree, preserves stable managed-worktree identities, uses same-filesystem
renames rather than copies, deletes nothing, creates Source from an observed
local main ref, and refuses a second or partial application.

## Alternatives considered

### Move all development documentation and tests outside GitHub

Rejected. Reproducible tests, architecture, decisions, harness code, and coding
agent laws are part of the source needed to build and maintain Optimus. Only
machine-local instances and outputs belong in Development.

### Keep the mixed root and rely on ignore rules

Rejected. Ignore rules prevent Git pollution but do not make stale root files,
150 GiB of local state, or multiple worktrees understandable to a person.

### Move the Git store with manual filesystem and raw Git commands

Rejected. It provides no atomic preflight, identity preservation, fixture, or
repeatable refusal path and violates the repository-managed VCS boundary.

## Consequences

- The project opens to a clean Source view and an explicit Development area.
- GitHub keeps every reproducible engineering asset but no local evidence,
  cache, toolchain, or worktree instance.
- Existing scripts may continue through compatibility links while new
  instructions name Development directly.
- The pre-migration root shadow is retained under Development/Archive until a
  later, separately reviewed cleanup proves each entry disposable.
- Source is intentionally detached so no coding agent can mistake it for its
  assigned task worktree.

## Risks

- A mid-migration process failure could leave the wrapper partly reorganized.
  Mitigation: all large moves are atomic same-filesystem renames; compatibility
  links are restored before Source creation; the disposable integration fixture
  executes the complete transition.
- Compatibility aliases could outlive their usefulness. Mitigation: they are
  explicitly documented as transitional and resolve to one canonical plane.
- A stale local remote-tracking ref could seed Source. Mitigation: migration is
  run immediately after a successful managed land and records the exact ref;
  Source is an orientation view, never delivery authority.

## Evaluation evidence

- A disposable real Git fixture migrates a bare store and assigned linked
  worktree, preserves a clean task branch and managed identity, archives the
  stale root, creates detached Source, and refuses reapplication.
- Managed delivery and hygiene tests cover both legacy and migrated discovery.
- Full `just verify` remains required before the migration command can land.

## Conditions for reconsideration

Remove compatibility links after all supported tooling and remembered paths
use Development directly. Reconsider detached Source only if a different clean
orientation mechanism proves equally understandable without becoming a writable
main checkout.

## Documentation completion addendum (2026-07-31)

## Reasons

The decision makes the invariant in the Decision section explicit and testable. It is preferred because the failure described in Context cannot be managed reliably through prompt convention or caller discipline alone.

## Relevant code

- `AGENTS.md`
- `justfile`
- `scripts/workspace_layout.py`
- `scripts/managed_delivery.py`
- `scripts/project_hygiene.py`

## Relevant tests

- `scripts/test_workspace_layout.py`
- `scripts/test_managed_delivery.py`
- `scripts/test_project_hygiene.py`

## Repository naming addendum (2026-08-01)

`Source/` was technically accurate but semantically misleading: people and
agents repeatedly inferred that tests, evaluations, docs, and development
automation did not belong there. The managed workspace command therefore
renames the clean view to `Repository/`, synchronizes it to exact remote-main
identity, and leaves checked-out local `main` untouched. Compatibility with the
old name is input-only for path explanation and one-time managed migration.
