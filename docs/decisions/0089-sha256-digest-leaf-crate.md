---
doc_id: decisions-0089-sha256-digest-leaf-crate
doc_type: decision
plane: decision
status: current
authority: record
summary: SHA-256 hex validation and digest computation move into a new dependency-free leaf crate `optimus-crypto` (validated `Sha256Digest` newtype, `is_sha256_hex` predicate, `sha256_hex` computation). The rule "a digest is exactly 64 hex digits" previously existed as ~14 hand-written copies across 10 crates, one of which had already drifted in its error text; every consumer now converts at the crate seam instead of re-deriving the rule. Serialized digest strings are byte-identical (the newtype is `#[serde(transparent)]`), so SQLite columns, wire fields, and persisted manifests are untouched.
reviewed_on: 2026-08-09
review_by: 2026-11-09
knowledge_type: decision
covers:
  - crates/optimus-crypto/**
depends_on:
  - docs/decisions/0034-control-plane-crate-peels.md
---

# ADR-0089: SHA-256 digest validation and computation in a leaf crate

## Status

Current.

## Context

`validate_sha256` / `validate_hash` / inline `len() != 64 ||
!bytes().all(|byte| byte.is_ascii_hexdigit())` checks existed in ~14 places
across 10 crates (`optimus-agent`, `optimus-artifacts`, `optimus-eval` ×2,
`optimus-host`, `optimus-kernel` ×3, `optimus-packs`, `optimus-policy` ×2,
`optimus-runtime`, `optimus-store`, `optimus-workflow`). Digest computation
(`format!("{:x}", Sha256::digest(...))`) was likewise duplicated (the eval
crate alone had two copies, one per module).

The copies were already drifting: `optimus-eval`'s replay and evaluation
modules disagreed on error wording for the same rule. A tightened rule (for
example rejecting upper-case hex, or checksum-aware IDs) would have needed
coordinated edits at every site.

The consumers share no existing common dependency: `optimus-policy` is a
deliberate dependency-free broker leaf, and `optimus-store` pulls `rusqlite`.
No existing crate could host the shared rule without either violating the
policy leaf property or dragging heavy dependencies into every consumer.

## Decision

Add `crates/optimus-crypto`, a dependency-free leaf crate (serde + sha2
only), owning:

- `Sha256Digest` — validated newtype; `parse(&str)`, `digest(&[u8])`,
  `as_str()`, `Display`; `#[serde(transparent)]` so serialized digests are
  byte-identical to the previous plain strings. Comparisons are symmetric:
  `PartialEq` impls for `str`/`&str`/`String` mirror the digest-first
  direction, so `literal == digest` compiles and both directions are
  case-insensitive and equivalent (confirmed by tests, 2026-08-11).
- `is_sha256_hex(&str) -> bool` — the single definition of "64 ASCII hex
  digits" (any case; computed digests are lowercase).
- `sha256_hex(&[u8]) -> String` — the single digest computation.

Every consumer converts at its seam: local `validate_sha256`/`validate_hash`
wrappers keep their crate-specific error types and messages but delegate the
rule to `is_sha256_hex`; inline checks call it directly; duplicated digest
helpers call `sha256_hex`. The crate-layer gate has no rule touching
`optimus-crypto`, and it is a leaf (no workspace dependencies), so no cycle
is possible.

## Consequences

- One definition of the digest rule, one test surface (the newtype's own
  tests), and no cross-crate drift on future changes.
- Error messages are preserved exactly at every call site; no wire or
  storage format changes (verified: serialization is transparent).
- The token budget re-baselines upward by the new crate's source
  (deliberate, committed growth wave; `just token-budget-update`).
- `optimus-policy` gains its first dependency — but on a dependency-free
  leaf that keeps the broker leaf property intact.

## Alternatives considered

- **Host the rule in `optimus-store`** — rejected: `optimus-policy` is a
  deliberate dependency-free broker leaf and cannot pull in `rusqlite`;
  `optimus-eval` and `optimus-workflow` do not depend on the store either.
- **Host the rule in `optimus-packs`** — rejected: `optimus-policy` and
  `optimus-eval` do not depend on packs, and packs carries tool-catalog
  knowledge that has nothing to do with digests.
- **Keep two types but generate one from the other** — rejected: the copies
  already drifted once; a generator is another hand-maintained artifact.

## Evaluation evidence

- Before: 14 hand-written `len() != 64 || is_ascii_hexdigit` checks across
  10 crates (grep-verified), plus 4+ duplicated digest computations; the
  eval crate's two copies disagreed on error wording.
- After: `optimus_crypto::is_sha256_hex` is the only rule; the crate's own
  tests cover parse/digest/serialize; all consumer crates' existing tests
  pass unchanged (policy, packs, artifacts, eval, host, runtime, store,
  workflow, agent, kernel).

## Conditions for reconsideration

- If the workspace gains a second crypto primitive (HMAC, signature
  verification), extend `optimus-crypto` rather than creating a sibling
  crate.
- If the newtype becomes the canonical storage type for digests (fields
  typed `Sha256Digest` instead of `String`), revisit serde defaults and
  SQLite bindings in one coordinated change.

## Reasons

- The consumers share no existing common dependency, so only a new leaf
  can hold the rule without violating the policy-leaf property or dragging
  heavy dependencies into every consumer.
- A validated newtype makes the digest rule part of the type system:
  callers cannot accidentally accept a non-digest at a seam that uses
  `Sha256Digest`, and the single test surface pins the rule once.

## Risks

- **Low**: behavior is preserved byte-for-byte (same predicate, same error
  messages, transparent serialization). The remaining risk is a future
  caller bypassing the crate with a new hand-written check — mitigated by
  code review and the crate's existence as the obvious home.

## Relevant code

- `crates/optimus-crypto/src/lib.rs` — the leaf crate (newtype, predicate,
  computation).
- The 10 consumer crates convert at their seams (see commit diff).

## Relevant tests

- `crates/optimus-crypto/src/lib.rs` — `sha256_hex_matches_sha2_direct`,
  `sha256_hex_handles_arbitrary_binary_bytes`,
  `is_sha256_hex_accepts_only_64_hex_digits`, `sha256_digest_parse_round_trips`,
  `sha256_digest_normalizes_case`, `sha256_digest_serializes_transparently`,
  `sha256_digest_symmetric_comparison_with_str_literals`,
  `sha256_digest_as_ref_str_borrows_canonical_form` (`AsRef<str>` for the
  canonical lowercase hex form, 2026-08-11).
- All pre-existing consumer-crate tests (unchanged, passing).
