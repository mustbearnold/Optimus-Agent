---
doc_id: plans-engineering-memory-phases
doc_type: history
plane: history
status: historical
authority: historical
summary: Status: completed in the initial foundation.
reviewed_on: 2026-07-31
review_by: never
knowledge_type: implementation-plan
owns:
  - AGENTS.md
  - docs/architecture/system-overview.md
  - docs/engineering-memory/README.md
  - docs/decisions/0017-engineering-memory-separation.md
  - docs/decisions/0032-engineering-memory-compact-lenses.md
  - docs/decisions/0061-generated-engineering-memory-is-a-disposable-cache.md
  - scripts/engineering_memory.py
  - scripts/test_engineering_memory.py
  - skills/update-engineering-memory/SKILL.md
watches:
  - docs/maps/**
  - docs/contracts/**
  - docs/decisions/**
covers:
  - AGENTS.md
  - docs/architecture/system-overview.md
  - docs/engineering-memory/README.md
  - docs/decisions/0017-engineering-memory-separation.md
  - docs/decisions/0032-engineering-memory-compact-lenses.md
  - scripts/engineering_memory.py
  - scripts/test_engineering_memory.py
  - skills/update-engineering-memory/SKILL.md
depends_on:
  - Cargo.toml
validated_by:
  - scripts/test_engineering_memory.py
---

# Engineering Memory phased implementation plan

## Phase 1 — repository audit

**Status: completed in the initial foundation.**

- Inventory Cargo packages, applications, source, tests, and existing ADRs.
- Map implemented runtime, tools, persistence, memory, routing, security,
  observability, and evaluation behavior.
- Record absent specialist-agent, router, replay, GPU, provenance, and general
  workflow layers as gaps rather than implemented features.
- Identify conflicting concepts and unsafe documentation drift.

**Exit evidence:** system overview and five focused maps cite real owners and use
claim-status labels.

## Phase 2 — foundation

**Status: completed for the initial repository-local foundation.**

- Add concise root laws.
- Add current system/repository maps and Engineering Memory guide.
- Create canonical generated agent/tool/workflow/prompt/model registries.
- Preserve existing ADRs and add ADR-0017 for Engineering Memory separation.
- Add the high-risk contract register.
- Add the first update skill.

**Exit evidence:** required files exist, local links/frontmatter validate, and no
registry invents a specialist agent.

## Phase 3 — behavioural contracts

**Status: planned behaviour.**

Implement contracts in risk order:

1. cancellation and exactly-one terminal outcome;
2. action-bound approvals and loopback authorization;
3. runtime filesystem confinement and fail-closed campaign decoding;
4. universal tool output/error/cancel/replay envelope;
5. workflow and agent lifecycle;
6. model routing, provenance, and deterministic replay;
7. memory clock/sensitivity/retention/erasure.

Each contract gets focused tests, integration tests, event semantics, and a
coverage entry. Documentation does not grant implementation status.

## Phase 4 — deterministic indexes and validation

**Status: implemented and superseded in storage authority by ADR-0061.**

- Generate package/dependency/source/test/registry maps.
- Compare covered-tree hashes before generation.
- Add strict mode for unresolved schema/contract/evaluation gaps.
- Replace regex extraction with `cargo metadata`, rustdoc JSON, or another typed
  compiler surface when stable for this toolchain.
- Validate the computed projection in CI without requiring generated artifacts
  to be committed.

## Phase 5 — specialist architecture and evaluations

**Status: planned behaviour.**

- Define the universal agent contract before adding agent definitions.
- Add a specialist only when responsibility/permissions/evals do not fit an
  existing owner.
- Define typed workflow schemas before adding Aipedia/publishing automation.
- Add baseline comparison for quality, reliability, cost, latency, security, and
  human corrections.
- Add project integration docs only alongside real adapters/workflows.

## Phase 6 — compact facts and agent lenses

**Status: implemented in ADR-0032 (2026-07-25 worktree redesign).**

- Compact staleness to hash/count/pattern storage.
- Compact impact to pattern→document relations with query-time expansion.
- Add budgeted lenses: `context`, `impact`, `owner`, `tools`, `stale`, `report`,
  `stat`.
- Add `validate --quick`, local hash cache, and `manifest.json` serving metadata.
- Keep full rebuild validate for CI/release.

**Exit evidence:** schema v2 generated artifacts, lens budget tests, and size
reduction versus pre-redesign dumps while preserving fail-closed validation.

## Phase 7 — retrieval and optional acceleration

**Status: planned behaviour.**

- Build CPU fixture baselines for relevance, recall, deduplication, temporal
  scoring, and context packing on top of lenses.
- Use replaceable established vector/embedding/reranking backends only as
  non-authoritative discovery aids.
- Benchmark transfer cost, batching, VRAM, latency, and quality before enabling
  GPU acceleration.
- Fit the development target (RTX 5070 12 GB) with headroom and retain a tested
  CPU fallback.

## Phase 8 — disposable local projection

**Status: implemented in ADR-0061 (2026-07-31).**

- Make `.engineering-memory/` an ignored local cache rather than tracked facts.
- Keep source, tests, curated docs, and accepted decisions authoritative.
- Auto-materialize a missing cache through `check` and bounded lenses.
- Serve current lens facts from deterministic computation when a local cache is
  stale, while retaining the old cache long enough to explain local change.
- Validate computed maps in memory without requiring, reading, or comparing
  generated cache files.
- Preserve sorted canonical JSON and content-addressed tree identity.
- Keep Engineering Memory separate from product/session/project memory.

**Exit evidence:** validation passes with no generated cache files; a bounded
lens recreates all cache artifacts; two in-memory projections remain byte
identical.

## Change protocol

For every phase:

1. run `check` before edits (it creates a missing local cache);
2. review affected documents/contracts/tests;
3. implement the smallest coherent slice;
4. run focused then integration/evaluation gates;
5. update curated knowledge;
6. run `validate`; optionally run `generate` only to warm/rebuild the cache;
7. report unresolved stale knowledge and known gaps.

No phase authorizes a wholesale repository restructure or production publishing.
