---
doc_id: decisions-0006-capability-packs
doc_type: decision
plane: decision
status: current
authority: record
summary: Decision record for ADR-0006: Progressive capability packs and skill→approval bridge, including its context, consequences, and current documentary status.
reviewed_on: 2026-07-31
review_by: 2026-10-31
---

# ADR-0006: Progressive capability packs and skill→approval bridge

## Status

Accepted — 2026-07-18

## Context

Hermes tends to expose large tool schemas every turn (context tax). Optimus Phase 4 introduces progressive **capability packs** with a hard schema-token budget, and wires Skills 2.0 permissions into SmartDeny job grants without letting skill text invent privileges.

## Decision

1. **Crate `optimus-packs`**
   - Built-in packs: `core` (always on), `browser`, `desktop`, `media`, `devex`, `social`.
   - Each pack exposes `ToolDesc { name, description, schema_tokens }`.
   - `CapabilitySession` starts with `core` only.
   - `activate(pack)` is an explicit segment boundary:
     - fails if pack unknown
     - fails if on-demand pack count would exceed `max_on_demand_packs` (default **2**)
     - fails if total schema tokens would exceed `max_schema_tokens` (default **2800**; ratchet — Core + pack-management tools + the two heaviest on-demand packs must fit, currently Browser 600 + Media 180 + Core ~1950 = 2730)
   - `schema_tokens()` sums loaded packs only (core + activated).
   - `deactivate(pack)` allowed for on-demand packs only (not core).
   - **Rotation (spec-006 R5, 2026-08-07)**: the agent-facing path is `provision(pack)` — at the count ceiling it atomically swaps the least-recently-*used* on-demand pack (recency = `touch` on every successful tool dispatch) and loads the new one; a schema-budget overflow leaves the session untouched. `release_pack` frees a slot and contracts the advertised schema in-turn. Packs with no available tools (ADR-0068 placeholders) answer a typed `pack_empty` error and can never evict a real pack.

2. **Schema tokens are estimates**, not tokenizer-exact — stable integers for budgeting tests and operator UX. Real model tokenizers come later.

3. **Skill → Runtime bridge** (`Runtime::grant_from_skill`)
   - Looks up skill; `authorize(skill, [Terminal])` must pass.
   - On success, inserts durable job `ApprovalGrant` (same as human grant).
   - Skill **without** Terminal cannot unlock `RunCommand`.
   - Deprecated skills cannot authorize.
   - Does not bypass path jails or budgets.

4. **CLI**
   - `optimus packs list`
   - `optimus packs demo-budget` — prints core vs core+browser+media budget math

## Non-goals

- Actual LLM tool JSON schema emission
- MCP dynamic packs
- Mid-turn silent pack activation without API call

## Consequences

- Long sessions can keep a thin waist by default.
- Skills become measured *and* permission-bounded relative to SmartDeny.
