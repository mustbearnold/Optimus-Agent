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
- R5. Pack rotation (task-driven): `activate_pack` MUST provision with
  least-recently-used swap at the on-demand count ceiling — the ceiling is
  a footprint guard, never a usage wall; `release_pack` MUST contract the
  advertised schema in-turn. Both MUST rebuild the system prompt so
  subsequent steps of the same turn see the change, and the swap MUST be
  atomic (a schema-budget failure leaves the session untouched). Recency
  is measured on USE: the kernel touches a pack after every successful
  tool dispatch, so eviction targets the pack the agent is NOT working
  with, not merely the one activated longest ago. Packs with no available
  tools (ADR-0068 placeholders) MUST NOT be provisionable — rotating a
  real pack out for zero capability is destructive — and MUST answer a
  typed `pack_empty` error.
- R6. The default schema budget is a ratchet (currently 2800): Core +
  pack-management tools + the two HEAVIEST co-required on-demand packs
  must fit (the web+vision workflow: Browser 600 + Media 180 + Core ~1950
  = 2730). Raises are spec amendments with evidence; the acceptance test
  is worst-case pack pairs, never averages over empty packs.

## Acceptance criteria
- [ ] A1. Given the memory/skills/packs crate suites, when they run, then all tests pass.
- [ ] A2. Given the retrieval map, when it is compared with the implemented memory surface, then it matches.
- [ ] A3. Given a session at the on-demand ceiling, when the agent provisions
  a further pack, then the least-recently-USED pack is swapped out and the
  provision succeeds (never `pack_on_demand_limit_exceeded`).
- [ ] A4. Given an activated pack, when `release_pack` runs, then the slot
  and schema tokens are freed and the next step's advertised tools no
  longer include the pack's tools.
- [ ] A5. Given a provision that would exceed the schema budget even after
  eviction, then the session is left untouched (no half-rotation).
- [ ] A6. Given the default budget, then Core + pack-management tools + the
  two heaviest on-demand packs fit within `max_schema_tokens` (2800).
- [ ] A7. Given a pack with no available tools, when the agent provisions it,
  then a typed `pack_empty` error returns and the session is untouched.

## Out of scope

- Engineering Memory (developer tooling, spec 011) and temporal project
  knowledge (spec 009).

## Open questions

- None.

## Links

Code: crates/optimus-memory, optimus-skills, optimus-packs · ADRs: 0054 ·
Ontology: optimus-memory, optimus-skills, optimus-packs
