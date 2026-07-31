---
doc_id: project-knowledge
doc_type: reference
plane: current
status: current
authority: canonical
summary: Canonical temporal project-control system for file history, component lifecycle, local workspace observations, and evidence-backed cleanup decisions.
reviewed_on: 2026-08-01
review_by: 2026-11-01
knowledge_type: temporal-project-knowledge
owns:
  - scripts/project_knowledge.py
  - scripts/test_project_knowledge.py
  - scripts/managed_project_cleanup.py
  - scripts/test_managed_project_cleanup.py
depends_on:
  - docs/repository-components.json
  - scripts/repository_ontology.py
  - docs/decisions/0064-temporal-project-knowledge-is-derived-provenance.md
validated_by:
  - scripts/test_project_knowledge.py
  - scripts/verify.sh
---

# Temporal project knowledge

This is the project-wide answer to “what is here, why does it exist, when did
it change, and what may safely go?” It joins three clocks without confusing
their authority:

1. **Repository event time** — every file addition, modification, and deletion
   retained from the ancestry of `HEAD`.
2. **Semantic lifecycle time** — authored component states, review deadlines,
   retention policy, removal conditions, and dated lifecycle events.
3. **Observation time** — append-only snapshots of machine-local Development
   areas, physical worktrees, generated caches, and disk use.

The generated graph is `.engineering-memory/temporal-project-graph.json`. It is
an ignored, deterministic cache; Git history and the authored component
database remain authority. Machine-local observations are never embedded into
that deterministic graph because two correct worktrees may have different
build caches.

## Operator commands

```text
just project-status
just cleanup-candidates
just path-history spikes/001-leptos-wry-csr
just project-snapshot
just project-graph
just project-cleanup-plan
just project-cleanup <plan-sha256>
```

`project-status` is the bounded daily view. `cleanup-candidates` distinguishes
inactive generated output recommended for cleanup, active but regenerable
caches, physical orphan worktrees requiring managed retirement, lifecycle
decisions due, and old stable source that must not be deleted merely because it
has not changed.

## Safety laws

- Age is evidence for review, never deletion authority.
- A path is removable only through explicit component policy or a closed,
  structurally verified generated-output convention.
- Destructive generated-output cleanup requires an exact metadata fingerprint;
  any change after planning refuses execution.
- Worktrees are retired only through the recovery-aware managed retirement
  command; a physical orphan may still contain uncommitted work.
- Historical paths remain graph nodes after deletion, so removing clutter does
  not erase why it existed.
- Snapshots are append-only local evidence under
  `Development/land/project-knowledge/snapshots/`; they are not repository
  source and are not pushed.

## Provenance model

File and component nodes are entities, commits are recorded activities, and
file events carry the derivation and time edge. This is a deliberately bounded
project graph, not a second source-control system and not a runtime Optimus
memory database.
