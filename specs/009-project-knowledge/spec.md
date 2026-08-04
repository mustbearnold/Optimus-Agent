---
doc_id: spec-009-project-knowledge
doc_type: reference
plane: work
status: current
authority: canonical
summary: The temporal project-knowledge database — a derived, embedded, code-aware interval graph generated from the repository, distinct from Engineering Memory.
reviewed_on: 2026-08-05
review_by: 2026-10-31
knowledge_type: specification
covers:
  - scripts/project_knowledge.py
  - scripts/project_knowledge_code.py
  - scripts/project_knowledge_db.py
validated_by:
  - scripts/test_project_knowledge.py
---

# 009 — Temporal project knowledge

Status: active
Owner: development agents (main-only)

## Purpose

A disposable, regenerable code-aware interval graph of the repository (who
touched what when), used for staleness and impact reasoning. It is derived
provenance — generated, never hand-edited — and is distinct from Engineering
Memory, runtime memory, and retrieval indexes.

## Requirements

- R1. The database MUST be generated (`scripts/project_knowledge.py
  generate`), content-addressed by its identity, and gitignored; generated
  JSON is not delivery state.
- R2. The generator MUST NOT depend on machine-local state
  (`Development/` is excluded) and MUST be deterministic for a given tree.
- R3. The `temporal-project-knowledge` gate MUST fail on drift between the
  generated database and the tree.
- R4. The ADR cluster 0064–0069 MUST stay the design authority for the
  interval graph semantics.

## Acceptance criteria
- [ ] A1. Given the current tree, when `scripts/project_knowledge.py generate` runs, then the temporal database is produced with a stable identity and `scripts/test_project_knowledge.py` passes.

## Out of scope

- Engineering Memory (spec 011).

## Open questions

- None.

## Links

Code: scripts/project_knowledge.py · ADRs: 0064–0069 · Ontology:
temporal-project-knowledge
