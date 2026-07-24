---
knowledge_type: decision
status: current
covers:
  - scripts/engineering_memory.py
  - scripts/test_engineering_memory.py
  - docs/engineering-memory/README.md
  - skills/update-engineering-memory/**
  - AGENTS.md
depends_on:
  - docs/decisions/0017-engineering-memory-separation.md
  - docs/specifications/git-stable-engineering-memory.md
validated_by:
  - scripts/test_engineering_memory.py
last_verified_commit: null
---

# ADR-0032: Compact Engineering Memory facts and budgeted agent lenses

- **Status:** Accepted
- **Date:** 2026-07-25

## Context

ADR-0017 established repository-local Engineering Memory with curated authority,
deterministic generated indexes, and a check/generate/validate loop. That design
preserved accuracy, but the serving shape became expensive:

- `knowledge-staleness.json` embedded full covered file records per document
- `change-impact.json` expanded every source path into reverse maps
- agents were directed at whole JSON artifacts instead of task-scoped views
- every check/validate cold-path rehashed the repository without a cache

The result was high token load and multi-second routine commands without any
accuracy benefit.

## Decision

Keep the four-layer authority model from ADR-0017, and redesign the generated
and agent-facing surfaces into three planes:

1. **Authority plane** — curated laws, maps, contracts, ADRs, skills
2. **Fact plane** — compact deterministic generated indexes under
   `.engineering-memory/`
3. **Lens plane** — budgeted query commands that project only task-relevant
   facts into agent context

Concrete rules:

- Schema version advances to `2`.
- Staleness stores hashes, counts, and patterns only. No `covered_files`
  payloads.
- Impact stores `pattern_to_knowledge` and expands paths at query time. No
  `source_to_knowledge` dump.
- Generated JSON is compact canonical encoding.
- Frontmatter may use `owns` (hard stale) and `watches` (warn only). Existing
  `covers` remains valid and means owns.
- Agent interface is:
  `check`, `context`, `impact`, `stale`, `tools`, `owner`, `report`, `stat`,
  plus `generate` / `validate` / `binding`.
- `validate --quick` checks tree identity, staleness, impact patterns, and
  structural authority without full rebuild compare.
- A local `.engineering-memory/.hash-cache.json` may accelerate hashing. It is
  not authority, not committed, and must not affect deterministic outputs.
- Raw generated JSON is machine truth, not a prompt-loading surface.

## Alternatives considered

### Keep pretty full dumps and ask agents to be careful

Rejected. Experience showed agents load large maps and burn context.

### Replace deterministic facts with embeddings/LLM summaries

Rejected. Authority must remain fail-closed and source-derived.

### Merge Engineering Memory into runtime `optimus-memory`

Rejected. Development knowledge has different trust, retention, and deployment
boundaries.

## Reasons

- Preserves source supremacy and fail-closed extraction.
- Cuts token cost by removing duplicated path records.
- Makes tiny changes cheap through lenses and optional owns/watches precision.
- Speeds repeated commands with a non-authoritative hash cache.
- Keeps CI-grade full validation available.

## Consequences

- Consumers must stop reading legacy `covered_files` /
  `source_to_knowledge` fields.
- Skills and AGENTS workflow point at `context`/`report` instead of raw JSON.
- Documentation maintenance can adopt `owns`/`watches` incrementally.
- Full validate remains the release/CI gate; quick validate is a developer hot
  path.

## Risks

- Query-time pattern expansion could miss a relation if pattern syntax drifts.
  Mitigation: unit tests for impact resolution and full validate rebuilds.
- Hash cache corruption could serve stale hashes. Mitigation: fingerprint
  includes mtime/size/inode; output still content-addressed; cache ignored by
  tree identity.
- Agents may still open raw JSON. Mitigation: manifest serving flags, skill
  hard rules, and `stat` guidance.

## Evaluation evidence

- Compact staleness/impact structural tests.
- Deterministic generation equality.
- Context lens budget test.
- Hash-cache hit/invalidate test.
- Full generate + validate markers.
- Size/`stat` comparison against pre-redesign artifacts.

## Conditions for reconsideration

Reconsider storage format if the repository needs multi-package independent
release indexes, or if a typed compiler extract API replaces source parsing.
Do not weaken claim labels or merge with runtime memory.

## Relevant code

- `scripts/engineering_memory.py`
- `scripts/test_engineering_memory.py`
- `skills/update-engineering-memory/SKILL.md`
- `docs/engineering-memory/README.md`

## Relevant tests

- `scripts/test_engineering_memory.py`
- `python scripts/engineering_memory.py validate`
- `python scripts/engineering_memory.py context --budget 3000`
