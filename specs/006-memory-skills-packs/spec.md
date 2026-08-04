---
doc_id: spec-006-memory-skills-packs
doc_type: reference
plane: work
status: current
authority: canonical
summary: Runtime semantic memory, procedural skills, and canonical pack/tool descriptors owned by optimus-memory, optimus-skills, and optimus-packs, distinct from Engineering Memory and project knowledge.
reviewed_on: 2026-08-05
review_by: 2026-10-31
knowledge_type: specification
covers:
  - crates/optimus-memory/src/**
  - crates/optimus-skills/src/**
  - crates/optimus-packs/src/**
depends_on:
  - docs/decisions/0054-a-selector-may-only-over-select.md

validated_by:
  - crates/optimus-memory/tests/**
  - crates/optimus-skills/tests/**
  - crates/optimus-packs/tests/**
---

# 006 — Memory, skills, packs

Status: active
Owner: development agents (main-only)

## Purpose

The runtime knowledge systems: semantic memory, procedural skills, and the
canonical pack/tool descriptor contract. These are distinct systems — runtime
memory, session state, skills, project knowledge, retrieval indexes, and
Engineering Memory must never be conflated.

## Requirements

- R1. `optimus-packs::ToolDesc` MUST be the canonical implemented tool
  contract; packs carry provider-visible input schemas, policy/invocation
  identity, availability, validation, and schema-token budgets.
- R2. Memory operations (`memory_list`, `memory_recall`, `memory_search`,
  `memory_correct`, `memory_forget`) MUST be runtime-owned and durable.
- R3. Skills MUST be procedural, pinnable, and deprecatable
  (`skills_list`, `skills_pin`, `skills_deprecate`); a merged change with a
  stale skill is a defect.
- R4. Packs MUST be verifiable (`packs_verify_signed`) and activatable per
  surface (`packs_state`, `packs_activate`, `packs_deactivate`).

## Acceptance criteria
- [ ] A1. Given the memory/skills/packs crate suites, when they run, then all tests pass.
- [ ] A2. Given the retrieval map, when it is compared with the implemented memory surface, then it matches.

## Out of scope

- Engineering Memory (developer tooling, spec 011) and temporal project
  knowledge (spec 009).

## Open questions

- None.

## Links

Code: crates/optimus-memory, optimus-skills, optimus-packs · ADRs: 0054 ·
Ontology: optimus-memory, optimus-skills, optimus-packs
