---
doc_id: decisions-0064-temporal-project-knowledge-is-derived-provenance
doc_type: decision
plane: decision
status: current
authority: record
summary: Establishes a derived temporal project graph that separates Git event history, authored semantic lifecycle, and append-only local observations while forbidding deletion by age alone.
reviewed_on: 2026-08-01
review_by: 2026-11-01
knowledge_type: decision
covers:
  - scripts/project_knowledge.py
  - docs/project-knowledge.md
  - docs/repository-components.json
depends_on:
  - docs/decisions/0061-generated-engineering-memory-is-a-disposable-cache.md
  - docs/decisions/0062-source-and-development-are-separate-workspace-planes.md
  - docs/decisions/0063-documentation-is-a-governed-authority-plane.md
validated_by:
  - scripts/test_project_knowledge.py
  - scripts/repository_ontology.py
---

# ADR-0064: Temporal project knowledge is derived provenance

- **Status:** Accepted
- **Date:** 2026-08-01

## Context

The repository had source, generated caches, experiments, evidence, tools, and
abandoned worktrees whose meanings were reconstructed repeatedly from paths and
timestamps. Git registration cleanup could even leave physical worktree
directories behind when the workspace alias changed. Modification time alone
could show that something was old, but could not say whether it was stable
authority, recoverable work, generated output, or obsolete clutter.

## Decision

Optimus development maintains one derived temporal project graph over the
ancestry of `HEAD` and the current non-ignored tree. It records commit
activities, current and deleted file entities, change events, component
membership, and semantic lifecycle history.

Machine-local Development state is observed separately and may be captured in
append-only snapshots. It never changes repository identity. Cleanup reporting
requires explicit retention authority or a closed generated-output rule. Age
alone never authorizes deletion, and unregistered physical worktrees always go
through recovery-aware managed retirement.

The generated graph is a disposable Engineering Memory cache. Git, manifests,
source, tests, ADRs, and `docs/repository-components.json` remain authority.

## Reasons

- Git already supplies durable transaction time for repository paths.
- The component catalog supplies the lifecycle and retention meaning that Git
  cannot infer.
- Local snapshots expose disk reality without polluting the distributable
  source tree or making one machine's caches canonical.
- Exact-plan cleanup preserves autonomy while making destructive scope
  inspectable and race-safe.

## Alternatives considered

- **Use filesystem modification time:** rejected because checkout, extraction,
  and generated files make it neither semantic nor durable.
- **Put every local artifact in Git:** rejected because caches and private
  recovery evidence would pollute the distributable repository.
- **Adopt an external graph database first:** rejected because the current
  relationship set is small enough for deterministic JSON and local queries;
  an external service would add operational authority without improving truth.
- **Delete anything older than a threshold:** rejected because mature stable
  source is often intentionally unchanged and dirty worktrees may be unique.

## Consequences

Agents receive a bounded status surface and complete retained path history.
Deleted experiments remain explainable. Local disk clutter becomes visible
without making it repository content. Component lifecycle changes require
dated evidence. The graph must be regenerated with Engineering Memory and its
coverage is a required repository gate.

## Risks and mitigations

- Git ancestry does not include unreachable history; managed recovery refs and
  land receipts remain the authority for preserved abandoned work.
- Rename detection is deliberately disabled so events remain deterministic;
  a semantic move is represented through component lifecycle evidence.
- Local snapshots can grow; their metadata is small, append-only, and retained
  under Development rather than source.

## Evaluation evidence

- The graph currently retains 252 commits, more than 3,000 file events, and
  both current and deleted paths while deterministically reproducing identity.
- Fixture tests prove deleted-path retention, lifecycle/age separation,
  physical-orphan classification, exact-plan refusal after a target changes,
  and symlink rejection.
- The live observation distinguished inactive generated output, orphan
  worktrees, and active caches rather than flattening all large paths into one
  deletion list.

## Conditions for reconsideration

Reconsider the JSON representation when query volume or cross-repository links
cannot be answered within a bounded local process. Any replacement must retain
the three-clock separation and the no-deletion-by-age law.

## Representation addendum (2026-08-01)

ADR-0065 replaces the JSON representation with an embedded SQLite property
graph after historical and relationship queries became a first-class operator
surface. It does not replace this decision's provenance authorities, three-clock
separation, disposable-projection rule or cleanup safety laws.

## Relevant code

- `scripts/project_knowledge.py`
- `scripts/repository_ontology.py`
- `scripts/managed_project_cleanup.py`
- `scripts/managed_worktree_retirement.py`

## Relevant tests

- `scripts/test_project_knowledge.py`
- `scripts/test_managed_project_cleanup.py`
- `scripts/test_managed_worktree_retirement.py`
- `scripts/verify.sh`
