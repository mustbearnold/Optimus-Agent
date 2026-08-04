---
doc_id: decisions-0061-generated-engineering-memory-is-a-disposable-cache
doc_type: decision
plane: decision
status: current
authority: record
summary: Decision record for ADR-0061: Generated Engineering Memory is a disposable cache, including its context, consequences, and current documentary status.
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: decision
covers:
  - scripts/engineering_memory.py
  - scripts/test_engineering_memory.py
  - docs/runbooks/engineering-memory.md
  - .gitignore
depends_on:
  - docs/decisions/0017-engineering-memory-separation.md
  - docs/decisions/0032-engineering-memory-compact-lenses.md
validated_by:
  - scripts/test_engineering_memory.py
---

# ADR-0061: Generated Engineering Memory is a disposable cache

- **Status:** Accepted
- **Date:** 2026-07-31

## Context

ADR-0017 separated repository Engineering Memory from product and session
memory. ADR-0032 made its generated facts deterministic and exposed bounded
lenses. The remaining tracked `.engineering-memory/*.json` files nevertheless
turned a derived projection into routine repository churn:

- any authoritative edit rewrote many tracked JSON files;
- validation depended on artifacts already being present on disk;
- generated diffs obscured the source, test, and documentation change;
- a clean checkout without those artifacts looked incomplete even though the
  generator had every authoritative input required to compute them.

Determinism does not require committing a projection. It requires that the same
authoritative tree produces the same canonical bytes and identity.

## Decision

Generated `.engineering-memory/` JSON is an ignored, disposable local cache.

1. Current source, executable tests, curated documentation, accepted ADRs, and
   Git delivery history remain authority.
2. `.engineering-memory/` is ignored in full. No generated file is required in
   a commit, review, checkout, archive, or release.
3. `check` remains read-only and uses the current authoritative tree as an
   in-memory baseline when cache is absent. Bounded lenses materialize a
   missing or structurally unusable cache automatically.
4. A complete but stale cache may provide a local before/after comparison.
   Current lens facts are computed from the authoritative tree rather than
   served as current from that stale cache.
5. `validate` builds the deterministic maps in memory and validates those
   computed values. Missing, stale, partial, or corrupt cache files cannot
   weaken or block validation.
6. `generate` remains an explicit cache warm/rebuild operation only.
7. Canonical sorted JSON, normalized text bytes, binary preservation, and the
   aggregate `tree_sha256` remain unchanged.
8. Engineering Memory remains development knowledge. It is not
   `optimus-memory`, session state, project content, retrieval data, or product
   runtime authority.

## Alternatives considered

### Keep generated JSON tracked

Rejected. It adds review and merge churn without adding authority or
reproducibility.

### Remove generated maps and compute every lens independently

Rejected. Repeated cargo metadata and source extraction is avoidable local
cost, and a prior local projection is useful for explaining changes.

### Treat cache validation as sufficient

Rejected. A cache can be stale or corrupt. Validation must prove the projection
computed from current authority.

## Reasons

- Review focuses on intentional source, test, decision, and documentation
  changes.
- Clean clones and archives remain complete without derived artifacts.
- Bounded lenses keep their low-context interface and gain a safe cold start.
- Content-addressed reproducibility is preserved and directly tested.
- Product/session memory boundaries do not change.

## Consequences

- Existing tracked `.engineering-memory/*.json` files are removed from Git by
  repository maintenance; deleting local copies is always safe.
- The first lens on a cold checkout may pay the full projection cost.
- `check` compares against the last local cache when present; without one it
  establishes the current tree as the local baseline.
- CI validates computation rather than generated-file cleanliness.
- Documentation and skills must no longer describe `generate` as a mandatory
  delivery step.

## Risks

- Automatic cache creation could hide extractor failure. Mitigation: generation
  remains fail-closed and lenses return the error.
- A stale cache could be mistaken for current facts. Mitigation:
  `maps_for_lens` compares content-addressed tree identity and computes current
  maps in memory on mismatch.
- Ignoring the directory could permit silent format drift. Mitigation:
  deterministic in-memory equality and cold-cache tests cover schema markers
  and exact canonical content.

## Evaluation evidence

- Unit validation with a missing cache proves no generated artifact is needed.
- A bounded tools lens on a missing cache recreates every generated artifact.
- Existing deterministic-generation and ambient-Git-independence tests remain
  green.
- Full and quick validation exercise the same computed source/doc authority;
  full mode additionally checks all references.

## Conditions for reconsideration

Reconsider persistence if an external consumer needs a versioned published
artifact with an explicit distribution contract. Do not restore tracked cache
files merely to avoid cold-start computation, and do not merge Engineering
Memory with runtime or session memory.

## Relevant code

- `scripts/engineering_memory.py`
- `.gitignore`

## Relevant tests

- `scripts/test_engineering_memory.py`
