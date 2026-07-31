---
knowledge_type: engineering-memory-guide
status: current
covers:
  - AGENTS.md
  - OPTIMUS_AGENTS.md
  - scripts/engineering_memory.py
  - skills/update-engineering-memory/**
depends_on:
  - Cargo.toml
  - docs/decisions/0017-engineering-memory-separation.md
  - docs/decisions/0032-engineering-memory-compact-lenses.md
  - docs/decisions/0061-generated-engineering-memory-is-a-disposable-cache.md
validated_by:
  - scripts/test_engineering_memory.py
last_verified_commit: null
---

# Optimus Engineering Memory

Optimus Engineering Memory is repository-local development knowledge for
building Optimus itself. It is not runtime memory, conversation memory,
project-content memory, or a production retrieval index.

## Planes

1. **Authority** — curated laws, maps, contracts, ADRs, lessons, skills
2. **Cache** — disposable deterministic indexes in `.engineering-memory/`
3. **Lenses** — budgeted query views for agents and humans

Source, executable tests, curated docs, and accepted decisions remain
authority. Raw generated JSON is an ignored local acceleration cache: it may be
deleted at any time and is never delivery evidence. Agents should load lenses,
not whole map files, into prompts.

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
4. Deterministic maps computed from the current authoritative tree.
5. Architecture summaries and lessons.
6. Plans and historical verification notes.

A lower layer never overrides contradictory current code. Contradictions must
be reported and resolved rather than blended.

## Surfaces

- `AGENTS.md` — concise development laws for the Optimus source tree. Not loaded
  into product chat.
- `OPTIMUS_AGENTS.md` — product runtime constitution loaded into Optimus chat
  system prompts.
- `docs/architecture/system-overview.md` — current-vs-planned architecture.
- `docs/maps/` — ownership, memory/retrieval, model, security, and
  observability/evaluation maps.
- `docs/contracts/high-risk-contracts.md` — implemented coverage and dangerous
  gaps; not a claim that every contract is complete.
- `docs/decisions/` — historical ADRs.
- `docs/lessons/` — reusable findings only.
- `docs/plans/engineering-memory-phases.md` — incremental implementation plan.
- `skills/update-engineering-memory/` — refresh procedure.
- `.engineering-memory/` — ignored disposable cache; never edit or commit.

## Commands

```text
python scripts/engineering_memory.py check
python scripts/engineering_memory.py context --budget 3000
python scripts/engineering_memory.py impact --path crates/optimus-kernel/src/lib.rs
python scripts/engineering_memory.py owner --path crates/optimus-packs/src/lib.rs
python scripts/engineering_memory.py tools --available
python scripts/engineering_memory.py stale
python scripts/engineering_memory.py report
python scripts/engineering_memory.py stat
python scripts/engineering_memory.py generate
python scripts/engineering_memory.py validate
python scripts/engineering_memory.py validate --quick
python scripts/engineering_memory.py binding > ../optimus-binding.json
```

Hot path for coding agents:

1. `check`
2. `context --budget 3000` when anything is stale/changed
3. update only owned docs
4. `validate --quick` (use full `validate` before delivery/release)
5. `report`

`check` and the bounded lenses materialize a missing cache automatically.
`generate` is an optional cache warm/rebuild command, not a source change or a
delivery step. `validate` computes and validates the deterministic maps in
memory and therefore succeeds on a clean checkout with no
`.engineering-memory/` directory.

## Frontmatter ownership

```yaml
---
knowledge_type: map
status: current
covers:                 # legacy alias of owns
  - crates/owner/src/file.rs
owns:                   # preferred hard invalidation set
  - crates/owner/src/file.rs
watches:                # warn only; do not auto-stale
  - crates/owner/src/**
depends_on:
  - docs/decisions/NNNN-decision.md
validated_by:
  - crates/owner/tests/**
last_verified_commit: <historical source commit or null>
---
```

`owns`/`covers` + `depends_on` drive staleness. `watches` is advisory impact only.

## Disposable computed facts (schema v2)

- Compact canonical JSON (sorted keys, no pretty indent).
- `knowledge-staleness.json` stores hashes/counts/patterns only.
- `change-impact.json` stores pattern→document relations; path expansion is
  query-time.
- `manifest.json` summarizes counts, artifact hashes, and serving policy.
- The entire `.engineering-memory/` directory, including `.hash-cache.json`, is
  local cache, not authority, and is gitignored.

**Confirmed current behaviour:** computed indexes use sorted source-file
SHA-256 records and an aggregate `tree_sha256` as their deterministic identity.
UTF-8 text is canonicalized to LF for cross-platform identity; binary bytes are
retained exactly. They do not embed ambient `.git`, branch, worktree, `HEAD`, or
remote state.

Deleting the cache changes neither computed bytes nor tree identity. A stale
cache may be used only to explain change since the last local projection;
current lens facts are recomputed from authority.

Curated `last_verified_commit` values are historical provenance only.

## Cost boundary

Tiny changes do not require a documentation rewrite. They do require `check`;
only affected owned knowledge is refreshed. A source change with no matching
ownership coverage is itself a coverage gap to report, not permission to add
speculative prose.
