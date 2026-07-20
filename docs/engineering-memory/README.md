---
knowledge_type: engineering-memory-guide
status: current
covers:
  - AGENTS.md
  - scripts/engineering_memory.py
  - skills/update-engineering-memory/**
depends_on:
  - Cargo.toml
validated_by:
  - scripts/test_engineering_memory.py
last_verified_commit: null
---

# Optimus Engineering Memory

Optimus Engineering Memory is repository-local development knowledge for
building Optimus itself. It is not runtime memory, conversation memory,
project-content memory, or a production retrieval index.

## Claim labels

Every important claim uses one of these labels:

- **Confirmed current behaviour** — directly supported by current source or
  executable evidence.
- **Inferred behaviour** — a bounded interpretation of current structure that
  has not been verified as a runtime contract.
- **Planned behaviour** — a target, not an implemented capability.
- **Unknown or unresolved behaviour** — missing evidence or an open decision.

## Authority order

1. Current source and schemas.
2. Focused tests and real execution evidence.
3. Accepted ADRs and explicit behavioural contracts.
4. Generated repository maps.
5. Architecture summaries and lessons.
6. Plans and historical verification notes.

A lower layer never overrides contradictory current code. Contradictions must
be reported and resolved rather than blended.

## Surfaces

- `AGENTS.md` — concise laws relevant to nearly every task.
- `docs/architecture/system-overview.md` — current-vs-planned architecture.
- `docs/maps/` — ownership, memory/retrieval, model, security, and
  observability/evaluation maps.
- `docs/contracts/high-risk-contracts.md` — implemented coverage and dangerous
  gaps; not a claim that every contract is complete.
- `docs/decisions/` — historical ADRs.
- `docs/lessons/` — reusable findings only.
- `docs/plans/engineering-memory-phases.md` — incremental implementation plan.
- `skills/update-engineering-memory/` — refresh procedure.
- `.engineering-memory/` — deterministic indexes; never edit directly.

## Generated indexes

Run:

```text
python scripts/engineering_memory.py check
python scripts/engineering_memory.py generate
python scripts/engineering_memory.py validate
```

`check` compares covered code with the recorded tree hashes before changing
anything. `generate` refreshes deterministic maps. `validate` checks generated
integrity, frontmatter, local links, duplicate IDs, registry completeness, ADR
shape, contract/evaluation gaps, and staleness.

**Confirmed current behaviour:** this repository has no `.git` directory, so
`last_verified_commit` is `null` and tree SHA-256 values are the verification
basis. If Git is introduced, the generator must begin recording HEAD and stale
commit markers.

## Cost boundary

Tiny changes do not require a documentation rewrite. They do require `check`;
only affected knowledge is refreshed. A source change with no matching
frontmatter coverage is itself a coverage gap to report, not permission to add
speculative prose.
