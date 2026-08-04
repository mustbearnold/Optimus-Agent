---
doc_id: decisions-0066-temporal-project-knowledge-is-a-code-aware-interval-graph
doc_type: decision
plane: decision
status: current
authority: record
summary: Upgrades the temporal project database to schema 2 with UTC-projected event time, topologically ordered ancestry, interval-valid package dependency edges, current-tree code symbols, author identity, and exact content digests.
reviewed_on: 2026-08-01
review_by: 2026-11-01
knowledge_type: decision
covers:
  - scripts/project_knowledge.py
  - scripts/project_knowledge_code.py
  - scripts/project_knowledge_db.py
  - scripts/test_project_knowledge.py
  - specs/009-project-knowledge/project-knowledge.md
depends_on:
  - docs/decisions/0064-temporal-project-knowledge-is-derived-provenance.md
  - docs/decisions/0065-temporal-project-knowledge-is-an-embedded-database.md
validated_by:
  - scripts/test_project_knowledge.py
  - scripts/verify.sh
---

# ADR-0066: Temporal project knowledge is a code-aware interval graph

- **Status:** Accepted
- **Date:** 2026-08-01

## Context

ADR-0064 fixed the three-clock separation and the no-deletion-by-age law;
ADR-0065 made the projection an embedded SQLite property graph. Operational
use then exposed correctness defects and one structural absence:

1. **Timestamp comparisons were offset-unsafe.** `%cI` was stored raw and
   compared lexically. This repository's real history mixes `+12:00` and `Z`
   committer offsets, so an as-of timestamp query could exclude an event whose
   instant preceded the boundary merely because its offset string sorted
   later.
2. **Ancestry positions were not ancestry.** `git log --reverse` without
   `--topo-order` walks by commit date, so a skewed clock could place a parent
   after its child while the documentation claimed commit order answered
   ancestry questions.
3. **Historical answers leaked current classification.** As-of results
   reported the file's present component as if it were historical fact.
4. **Integrity gates compared counts, not content.** Tampering that preserved
   per-kind and per-predicate counts (edited `properties_json`, a swapped
   same-predicate target) passed validation.
5. **Two gates could not do their jobs.** The unclassified-file check was
   unreachable behind the root catch-all component, and the retired-path check
   demanded at least one deletion in history rather than verifying retention.
6. **The graph knew files but no code.** No packages, no dependencies, no
   symbols, no authorship — so "which crates depended on X in July", "who
   wrote most of this subsystem", and "where is symbol Y declared" were
   unanswerable.

A dated best-practice check (2026-08-01, by web search per the development
workflow) confirmed the current bar for temporal knowledge graphs: bi-temporal
modelling with explicit per-edge validity intervals, and invalidation that
closes an interval rather than deleting the fact — see the Zep/Graphiti
temporal knowledge-graph model (getzep.com, github.com/getzep/graphiti), the
CIKM 2025 temporal-validity benchmark (dl.acm.org/doi/10.1145/3746252.3761648),
and the OpenAI temporal-agents cookbook (developers.openai.com). For a fully
derived, atomically replaced cache, ingestion time inside the artifact is
meaningless by construction; the honest equivalents recorded here are the
derivation identity (`graph_identity`, `head`) plus append-only observation
snapshots.

## Decision

Database schema 2, in the same disposable projection, with the same
authorities:

1. **Event time is projected onto UTC at derivation.** Every stored instant
   (`committed_at`, `authored_at`, file events, lifecycle events, interval
   bounds) is normalized so lexical order equals instant order. Offsets remain
   recoverable from Git, which stays authority.
2. **History is walked topologically.** `--topo-order` guarantees parents
   precede children; validation enforces `parent.position < child.position`
   as an executable invariant, so "ancestry position" is now a proven claim.
3. **Package dependencies are interval-valid edges.** Every `Cargo.toml` and
   `package.json` state in retained history is parsed; a dependency opens an
   interval at the commit that introduced it and closes at the commit that
   removed it (`valid_from`/`valid_to` in both ancestry order and UTC event
   time). Removal closes intervals — it never deletes rows, extending the
   no-deletion-by-age law to edges. Traversal honours intervals: a closed
   dependency is visible at points inside its interval and invisible after.
4. **Code symbols are current-tree facts only.** Top-level Rust items and
   exported script symbols are extracted deterministically from the current
   tree and linked with `declares` edges carrying no event time. History
   stores no symbol claim it cannot prove; historical symbol queries remain
   out of scope rather than guessed.
5. **Authorship is first-class.** Commits carry author identity; `authored_by`
   edges make ownership queries answerable from the graph.
6. **Integrity is content-exact.** `property_graph_sha256` (all entities and
   relations) and `domain_sha256` (the full domain round trip) are recorded at
   population and re-derived at validation, so count-preserving tampering is
   detected.
7. **As-of answers are honest about classification.** Historical states report
   `component_now`, naming the present classification instead of implying a
   historical one. Component classification history remains unrecorded by
   authority (the catalog is current-state), and the label says so.
8. **Boundary prefixes are literal.** Commit-prefix resolution escapes SQL
   wildcard characters; `path-at <path> "5d%"` is an error, not a wildcard.
9. **Three further closed cleanup conventions.** Duplicate Playwright browser
   payloads under `Development/tmp` proven by the canonical `tools/` payload,
   generated per-run Optimus homes under `compiled-workbench`, and regenerable
   `node_modules` trees beside retained manifests inside the preserved
   pre-migration snapshot. Each is structural proof, never age.

## Consequences

- The three-clock separation and the no-deletion-by-age law required by
  ADR-0064's reconsideration clause are retained unchanged; this ADR replaces
  representation and coverage, not authority.
- `just project-neighbors` forwards an `at` boundary, so time-travel traversal
  is operator-reachable.
- Schema-1 databases fail validation on the missing digest metadata and are
  rebuilt from authority automatically; no migration of disposable state is
  attempted.
- Symbol extraction is a deterministic line-level parser, not a compiler.
  It claims top-level declarations only; anything deeper (call graphs, type
  resolution, historical symbols) requires new evidence and a new decision.
- Manifest states that fail to parse neither open nor close intervals; the
  previous proven state carries forward, so a broken intermediate commit
  cannot sever dependency continuity.

## Evaluation evidence

On this repository at decision time: 254 commits, 657 files, 3,080 file
events, 89 packages, 262 dependency intervals of which 8 are closed by real
historical removals, 2,786 current-tree symbols, 4 author identities, 3,923
entities and 7,513 relations; full check plus regeneration completes in under
one second. The regression suite exercises offset-mixed as-of boundaries,
interval-respecting traversal, non-topological rejection, count-preserving
tamper detection, retention of dropped historical paths, and structural proofs
for all cleanup conventions.

## Reconsider when

- Component classification gains authored history, enabling true historical
  classification answers.
- Symbol needs outgrow a line-level parser (call graphs, references), which
  would justify a real parsing dependency and its own decision.
- The projection needs cross-repository federation or concurrent writers,
  which ADR-0065 already names as out of scope.
