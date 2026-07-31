---
doc_id: decisions-0065-temporal-project-knowledge-is-an-embedded-database
doc_type: decision
plane: decision
status: current
authority: record
summary: Establishes the temporal project graph as an embedded transactional SQLite database with normalized domain tables, property-graph relations, indexed historical queries, migrations, and integrity gates.
reviewed_on: 2026-08-01
review_by: 2026-11-01
knowledge_type: decision
covers:
  - scripts/project_knowledge.py
  - scripts/project_knowledge_db.py
  - scripts/test_project_knowledge.py
  - docs/project-knowledge.md
depends_on:
  - docs/decisions/0061-generated-engineering-memory-is-a-disposable-cache.md
  - docs/decisions/0064-temporal-project-knowledge-is-derived-provenance.md
validated_by:
  - scripts/test_project_knowledge.py
  - scripts/verify.sh
---

# ADR-0065: Temporal project knowledge is an embedded database

- **Status:** Accepted
- **Date:** 2026-08-01

## Context

ADR-0064 intentionally began with deterministic JSON because the initial
relationship set and operator surface were small. That projection proved the
provenance and cleanup laws, but every query rebuilt and scanned the full
document. It offered no indexed as-of query, schema migration, foreign-key
integrity, ad hoc query surface or graph traversal. Calling the document a
database would therefore overstate the implementation.

Historical path queries and component/commit relationship exploration are now
first-class operator needs. They require durable local indexes and explicit
query semantics without turning a generated cache into project authority or
introducing a service dependency.

## Decision

The temporal project projection is an embedded SQLite database at
`.engineering-memory/temporal-project-graph.sqlite3`.

1. Normalized tables store commits and parents, components and lifecycle
   events, files and ordered change events.
2. Indexed `entities` and `relations` tables expose the same facts as a
   property graph for bounded traversal.
3. Exact-commit as-of queries use Git ancestry position. ISO-8601 queries use
   event wall-clock time. Those orders are explicit and never substituted for
   each other.
4. A query-only connection exposes arbitrary SQL. Mutation attempts are
   refused.
5. Versioned migrations, foreign keys, check constraints, SQLite integrity,
   count reconciliation and exact graph round trips are mandatory gates.
6. Rebuilds populate and validate a new database, flush it, then atomically
   replace the prior projection.
7. Git, the component catalog, source, tests and decisions remain authority.
   The database remains ignored and disposable; stale, corrupt or unsupported
   projections are rebuilt rather than trusted.
8. Machine-local observations remain separate append-only snapshots. They do
   not enter repository identity or the distributable database.

## Reasons

- SQLite supplies transactions, indexes, constraints and mature local query
  semantics without an external process or credential boundary.
- Domain tables make temporal meaning inspectable; generic graph relations
  make cross-entity exploration possible without flattening every fact into an
  opaque edge payload.
- Atomic replacement fits a fully derived cache better than incremental writes:
  readers see either the old complete graph or the new complete graph.
- Read-only SQL makes unanticipated local questions answerable without adding
  a bespoke command for each one.

## Alternatives considered

- **Keep JSON and add more Python scans:** rejected because persistence without
  indexes, constraints or query semantics is still a document cache.
- **Adopt Neo4j, Memgraph or another graph service:** rejected because this
  bounded repository graph does not justify a daemon, network authority,
  credentials or a second operational lifecycle.
- **Make SQLite incrementally authoritative:** rejected because Git and the
  authored component catalog already own the facts. Incremental authority would
  create reconciliation and recovery ambiguity.
- **Store only generic triples:** rejected because ordered file events and
  commit ancestry deserve typed constraints and efficient domain queries.

## Consequences

Cold generation uses more disk than compact JSON, but current repository scale
remains comfortably local. Queries stop reparsing Git once the projection is
current. Database bytes are not required to be reproducible across SQLite
versions; queried content and graph identity are deterministic. Schema changes
must add a migration and round-trip coverage.

## Risks and mitigations

- **Projection mistaken for authority:** the path remains ignored, query
  commands verify source identity and HEAD, and documentation names the
  authoritative inputs.
- **Partial or corrupt rebuild:** population occurs in a temporary database;
  integrity and round-trip checks precede flush and atomic replacement.
- **Temporal ambiguity:** commit queries use recorded ancestry position while
  timestamp queries explicitly use wall-clock event time.
- **Graph explosion:** traversal depth is bounded to four and every relation
  direction is indexed.
- **SQL mutation:** the public SQL connection sets SQLite `query_only` before
  executing caller text.

## Evaluation evidence

- A live projection covers more than 250 commits, 700 current or retired paths,
  3,000 file events and 4,000 typed relations.
- Fixture tests prove schema migration, exact graph round trip, stale-identity
  replacement, commit and timestamp as-of deletion semantics, graph traversal,
  read-only SQL refusal and existing cleanup separation.
- The repository gate constructs and validates the complete database in memory,
  so a clean checkout needs no generated artifact.

## Conditions for reconsideration

Reconsider the embedded database when cross-repository federation, concurrent
writers or query volume cannot remain bounded in one local process. Any
replacement must preserve authoritative-source separation, explicit temporal
orders, atomic visibility and cleanup safety.

## Relevant code

- `scripts/project_knowledge.py`
- `scripts/project_knowledge_db.py`

## Relevant tests

- `scripts/test_project_knowledge.py`
- `scripts/verify.sh`
