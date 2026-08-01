---
doc_id: project-knowledge
doc_type: reference
plane: current
status: current
authority: canonical
summary: Canonical SQLite temporal project-control database for file history, component lifecycle, code structure with dependency validity intervals, local workspace observations, graph traversal, and evidence-backed cleanup decisions.
reviewed_on: 2026-08-01
review_by: 2026-11-01
knowledge_type: temporal-project-knowledge
owns:
  - scripts/project_knowledge.py
  - scripts/project_knowledge_code.py
  - scripts/project_knowledge_db.py
  - scripts/test_project_knowledge.py
  - scripts/managed_project_cleanup.py
  - scripts/test_managed_project_cleanup.py
depends_on:
  - docs/repository-components.json
  - scripts/repository_ontology.py
  - docs/decisions/0064-temporal-project-knowledge-is-derived-provenance.md
  - docs/decisions/0065-temporal-project-knowledge-is-an-embedded-database.md
  - docs/decisions/0066-temporal-project-knowledge-is-a-code-aware-interval-graph.md
validated_by:
  - scripts/test_project_knowledge.py
  - scripts/verify.sh
---

# Temporal project knowledge

This is the project-wide answer to “what is here, why does it exist, when did
it change, who changed it, what depended on what when, and what may safely
go?” It joins three clocks without confusing their authority:

1. **Repository event time** — every file addition, modification, and deletion
   retained from the ancestry of `HEAD`, walked topologically and projected
   onto UTC so stored order is both ancestry order and instant order
   (ADR-0066).
2. **Semantic lifecycle time** — authored component states, review deadlines,
   retention policy, removal conditions, and dated lifecycle events.
3. **Observation time** — append-only snapshots of machine-local Development
   areas, physical worktrees, generated caches, and disk use.

The generated database is
`.engineering-memory/temporal-project-graph.sqlite3`. It is an ignored,
disposable SQLite projection; Git history and the authored component database
remain authority. Machine-local observations (disk use, worktree registration,
cache state) are never embedded into that deterministic projection because two
correct worktrees may have different build caches. The projection does,
deliberately, include the working overlay of the deriving checkout: untracked
non-ignored files are file nodes and per-file working state is recorded, so
two checkouts at the same `HEAD` with different uncommitted edits produce
different — and differently identified — databases.

The database is a normalized, indexed property graph rather than a serialized
document. Domain tables retain commits (with author identity), parents,
components, lifecycle events, files, file events, packages, dependency
validity intervals and current-tree code symbols. Generic `entities` and
`relations` tables expose `changed_in`, `classified_as`, `authored_by`,
`depends_on`, `declares`, component-parent, pairing, commit-parent and
lifecycle edges for bounded traversal. Dependency edges carry
`valid_from`/`valid_to` bounds in both ancestry order and UTC event time; a
removed dependency closes its interval and is never deleted. Foreign keys,
`CHECK` constraints, schema migrations, count reconciliation, exact content
digests, topological-order invariants, graph round trips and SQLite integrity
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
just project-neighbors cargo:optimus-desktop 1 5df1d567
just project-query "SELECT path, occurred_at, status FROM file_events ORDER BY occurred_at DESC LIMIT 10"
just project-query "SELECT dependency, valid_from_time, valid_to_time FROM package_dependencies WHERE package_id='cargo:optimus-desktop'"
just project-query "SELECT author_email, count(*) FROM commits GROUP BY author_email"
just project-query "SELECT path, line FROM code_symbols WHERE name='ToolDesc'"
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

`path-at` accepts an unambiguous commit id or ISO-8601 timestamp and
reconstructs path existence from indexed events; wildcard characters in a
prefix are literal, and timestamps with any UTC offset compare as instants.
Historical states report `component_now` — the present classification — never
a pretended historical one. `project-neighbors` traverses property-graph edges
to a maximum depth of four and accepts an optional third argument as an as-of
boundary; dependency edges respect their validity intervals at that boundary.
`project-query` accepts one arbitrary SQL statement through a query-only
SQLite connection; attempted writes are refused. These query commands
materialize or refresh the disposable database before reading it.

## Safety laws

- Age is evidence for review, never deletion authority.
- A path is removable only through explicit component policy or a closed,
  structurally verified generated-output convention.
- Destructive generated-output cleanup requires an exact metadata fingerprint;
  any change after planning refuses execution. Symlinks inside a candidate are
  fingerprinted by their own metadata and target string and are deleted as
  entries, never followed; a candidate whose root is a symlink is refused.
- Worktrees are retired only through the recovery-aware managed retirement
  command; a physical orphan may still contain uncommitted work.
- Historical paths remain graph nodes after deletion, so removing clutter does
  not erase why it existed.
- Snapshots are append-only local evidence under
  `Development/land/project-knowledge/snapshots/`; they are not repository
  source and are not pushed.

## Provenance model

File, component, commit, author, package, symbol and lifecycle nodes are
entities. Indexed relations carry predicates, event time, commit order,
validity bounds and typed properties. Commit order is topological — parents
always precede children, enforced by an executable invariant — so it answers
exact ancestry queries; timestamp queries compare UTC instants. Code symbols
are current-tree facts only: the graph records no historical symbol claim it
cannot prove from retained evidence.

Population records exact content digests over both the domain tables and the
property graph; validation re-derives them, so tampering that preserves row
counts is still detected, and an invalid database can never become authority
merely because it is persistent.

This is a deliberately bounded embedded project database, not a second
source-control system, an external graph service or a runtime Optimus memory
database.
