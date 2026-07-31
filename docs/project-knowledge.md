---
doc_id: project-knowledge
doc_type: reference
plane: current
status: current
authority: canonical
summary: Canonical SQLite temporal project-control database for file history, component lifecycle, local workspace observations, graph traversal, and evidence-backed cleanup decisions.
reviewed_on: 2026-08-01
review_by: 2026-11-01
knowledge_type: temporal-project-knowledge
owns:
  - scripts/project_knowledge.py
  - scripts/project_knowledge_db.py
  - scripts/test_project_knowledge.py
  - scripts/managed_project_cleanup.py
  - scripts/test_managed_project_cleanup.py
depends_on:
  - docs/repository-components.json
  - scripts/repository_ontology.py
  - docs/decisions/0064-temporal-project-knowledge-is-derived-provenance.md
  - docs/decisions/0065-temporal-project-knowledge-is-an-embedded-database.md
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

The generated database is
`.engineering-memory/temporal-project-graph.sqlite3`. It is an ignored,
disposable SQLite projection; Git history and the authored component database
remain authority. Machine-local observations are never embedded into that
deterministic projection because two correct worktrees may have different build
caches.

The database is a normalized, indexed property graph rather than a serialized
document. Domain tables retain commits, parents, components, lifecycle events,
files and file events. Generic `entities` and `relations` tables expose
`changed_in`, `classified_as`, component-parent, pairing, commit-parent and
lifecycle edges for bounded traversal. Foreign keys, `CHECK` constraints,
schema migrations, count reconciliation, graph round trips and SQLite integrity
checks are executable gates.

Generation happens in a private new database, with fact population committed in
one transaction. The complete file is validated, flushed and atomically renamed
over the previous projection.
Missing, stale, corrupt or unsupported local projections are rebuilt from
authority; an invalid database can never become authority merely because it is
persistent.

## Operator commands

```text
just project-status
just cleanup-candidates
just path-history spikes/001-leptos-wry-csr
just path-at scripts/project_knowledge.py 5df1d567
just project-neighbors scripts/project_knowledge.py 2
just project-query "SELECT path, occurred_at, status FROM file_events ORDER BY occurred_at DESC LIMIT 10"
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

`path-at` accepts an unambiguous commit id or ISO-8601 timestamp and reconstructs
path existence from indexed events. `project-neighbors` traverses property-graph
edges to a maximum depth of four. `project-query` accepts one arbitrary SQL
statement through a query-only SQLite connection; attempted writes are refused.
These query commands materialize or refresh the disposable database before
reading it.

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

File, component, commit and lifecycle nodes are entities. Indexed relations
carry predicates, event time, commit order and typed properties. Commit order
answers exact historical queries without pretending wall-clock timestamps are
an ancestry order; timestamp queries retain their wall-clock meaning.

This is a deliberately bounded embedded project database, not a second
source-control system, an external graph service or a runtime Optimus memory
database.
