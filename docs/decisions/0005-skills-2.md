# ADR-0005: Skills 2.0 — outcome-gated procedural memory

## Status

Accepted — 2026-07-18

## Context

Hermes creates skills freely after complex tasks. Quality is unmeasured; skills can accumulate and cannot expand privilege in theory but markdown bodies are not permission-checked at load time. Optimus must exceed this with a **measured** learning loop.

## Decision

Ship `optimus-skills` with:

1. **Versioned skill records** (SQLite `skills.db`): id, name, version, status, body, permissions[], metrics.
2. **Statuses:** `candidate` → `proven` | `deprecated`; `pinned` is sticky and exempt from auto-demote.
3. **Create always as `candidate`** (unless human pin at create — still records provenance).
4. **Outcome recording:** each use records success/failure + optional token_cost; updates rolling counters.
5. **Auto-promote to `proven`** only when:
   - `uses >= min_uses` (default 3)
   - `success_rate >= min_success_rate` (default 0.8)
   - not deprecated
6. **Permissions are closed:**
   - declared at create time
   - `authorize(required)` succeeds iff `required ⊆ declared`
   - body/metadata updates **cannot add** permissions (only remove or keep subset)
7. **No skill text grants runtime capability** — skills never bypass SmartDeny; they only declare what the operator may request when following the skill.
8. **Resolve** returns proven+pinned first, then candidates (optional), never deprecated by default.

## Non-goals (Phase 3)

- Markdown skill hub / agentskills.io import (later)
- Executable check runners inside skill promote (optional hook later)
- LLM-authored skill body generation

## Public seam

```text
SkillRegistry::open(path)
create(SkillDraft) -> SkillId
record_outcome(id, Outcome)
try_promote(id) -> Status
pin(id) / deprecate(id)
authorize(id, required_perms) -> Result
list(filter) -> Vec<SkillView>
```
