---
knowledge_type: decision
status: current
covers:
  - crates/optimus-runtime/src/lib.rs
  - crates/optimus-runtime/src/campaign.rs
  - crates/optimus-runtime/tests/path_confinement.rs
depends_on:
  - docs/decisions/0016-fs-sandbox-allowlist.md
  - docs/contracts/high-risk-contracts.md
validated_by:
  - crates/optimus-runtime/tests/path_confinement.rs
  - crates/optimus-runtime/src/campaign.rs
last_verified_commit: null
---

# ADR-0018: Fail-closed runtime path and campaign decoding

- **Status:** Accepted
- **Date:** 2026-07-20

## Context

The Work Graph runtime accepted relative paths after checking only an existing
immediate parent. For a path such as `linked/new/file.txt`, where `linked` was a
workspace directory symlink or Windows junction to an external directory and
`new` did not yet exist, `create_dir_all` followed the linked ancestor and wrote
outside the workspace.

Campaign persistence also decoded malformed data permissively. Unknown statuses
became `Pending`, invalid UUIDs became the nil UUID, invalid optional job IDs
became `None`, integer casts could wrap, and malformed `kind_json` became an
executable empty `WriteFile("lost.txt")` step.

Both behaviours violated the rule that model-derived or persisted invalid input
must fail before effects.

## Decision

1. Runtime effect paths accept only normal components; empty, current-directory,
   absolute, parent, root, and platform-prefix components are rejected.
2. Before directory creation, the runtime walks from the requested target to its
   nearest existing ancestor, canonicalizes it, and requires it to remain under
   the canonical workspace root.
3. Existing targets, dangling links, directory links, Windows junctions, and
   missing descendants below linked ancestors fail closed when confinement
   cannot be proven.
4. The same path resolver protects `WriteFile` and `AssertFileEquals`.
5. Campaign rows decode through exact fallible conversions. Invalid campaign or
   step UUIDs, statuses, timestamps, indices, optional job IDs, and step JSON
   return typed errors.
6. Each campaign persists its immutable expected step count. Legacy databases
   gain the column in a transaction and initialize it from existing rows.
7. Loading requires exact cardinality and contiguous indices `0..step_count`;
   missing, partially reassigned, reordered-gap, or empty plans are corruption.
8. `CampaignStore::run` performs no runtime/database/workspace effect until the
   complete persisted campaign view has decoded successfully.

## Reasons

- Confinement must be proven before `create_dir_all`, because directory creation
  is itself an external effect.
- Persisted corruption is neither user intent nor a safe source of executable
  defaults.
- Exact typed errors preserve evidence for future diagnostics and migrations.
- One resolver for read assertions and writes avoids divergent path policy.
- A count and contiguous index set independently prove that a filtered query did
  not silently return only part of the persisted plan.

## Alternatives considered

### Keep immediate-parent canonicalization

Rejected because a missing immediate parent can hide an already-existing linked
ancestor and create external directories before the write.

### Sanitize corrupt values to defaults

Rejected because a default can change identity, state, ownership, or executable
behavior. Corruption is not user intent.

### Skip only the corrupt step

Rejected because silently changing an ordered durable plan destroys auditability
and can make later steps run under false assumptions.

### Immediately adopt handle-relative `openat`/Windows NT handle traversal

Deferred. It is the stronger answer to adversarial concurrent path mutation, but
requires a platform abstraction and broader compatibility work. The current
change closes the deterministic pre-existing-link escape without claiming race-
free filesystem transactions.

## Consequences

### Positive

- Pre-existing symlink/junction escapes through missing descendants are denied
  before `create_dir_all`.
- Corrupt campaign data cannot synthesize a `WriteFile` job.
- A partially reassigned step cannot silently shorten a multi-step plan.
- Callers receive explicit errors instead of plausible default state.
- Tests cover the public runtime effect seam on Windows junctions and Unix
  symlinks where available.

### Negative

- Previously tolerated corrupt campaign rows now fail inspection and execution.
  Operators need a future diagnostic/repair command rather than implicit repair.
- Canonicalization adds filesystem metadata operations to file effects.
- This does not make multiple database writes transactional.

## Risks and limitations

- Path checking remains preflight and is not handle-relative; a hostile concurrent
  actor can still attempt a time-of-check/time-of-use swap.
- Runtime writes do not yet share `FsRoots` secret-basename policy.
- Campaign records have no general schema/version envelope or repair tool; only
  the expected-step-count column has a targeted legacy migration.
- Campaign and Work Graph databases remain independently committed.

## Superseded limitations

**Confirmed current behaviour (2026-07-20):** ADR-0019 supersedes the four
limitations above with retained workspace directory capabilities, a shared
secret-basename predicate, campaign schema v3 plus diagnose/repair tooling, and
one `optimus.db` campaign/Work Graph authority with deterministic handoff IDs.
The original limitations remain here as decision history.

## Evaluation evidence

- `path_confinement::write_rejects_missing_parent_below_linked_ancestor`
- `path_confinement::assert_file_rejects_linked_ancestor`
- traversal, drive-relative, and normal nested-write tests
- leading-current-directory rejection for write and assertion effects
- campaign corrupt-kind no-effect regression
- campaign UUID/status/time/index/job-ID corruption matrix
- orphaned and partially reassigned step relationship regressions
- expected-count/index integrity and legacy-schema migration tests
- strict `optimus-runtime` Clippy and package tests
- full workspace test suite

## Relevant code

- `crates/optimus-runtime/src/lib.rs`
- `crates/optimus-runtime/src/campaign.rs`

## Relevant tests

- `crates/optimus-runtime/tests/path_confinement.rs`
- inline campaign corruption tests in
  `crates/optimus-runtime/src/campaign.rs`

## Conditions for reconsideration

Revisit this decision when adding runtime rename/delete, concurrent untrusted
filesystem actors, a unified filesystem capability layer, campaign schema
versions/migrations, or transactional campaign-to-job handoffs.

Those reconsideration conditions were met by ADR-0019. Further work is tracked
there and in C-15.
