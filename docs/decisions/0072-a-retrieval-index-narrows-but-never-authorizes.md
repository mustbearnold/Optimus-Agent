---
doc_id: decisions-0072-a-retrieval-index-narrows-but-never-authorizes
doc_type: decision
plane: decision
status: current
authority: record
summary: Free-text memory recall is SQLite FTS5 over claim text, and the index is a candidate list only — every hit is re-read from claims and re-authorized, erasure deletes the index row in the same transaction that closes the claim, and a stale claim is returned labelled with its standing rather than silently dropped or silently trusted.
reviewed_on: 2026-08-01
review_by: 2026-11-01
knowledge_type: decision
covers:
  - crates/optimus-memory/src/text_recall.rs
  - crates/optimus-memory/src/redaction.rs
  - crates/optimus-host/src/consoles.rs
  - crates/optimus-host/src/scope.rs
depends_on:
  - docs/decisions/0002-memory-invariants.md
  - docs/decisions/0004-metamemory-mvp.md
validated_by:
  - crates/optimus-memory/src/text_recall.rs
  - crates/optimus-memory/tests/text_recall_contracts.rs
---

# ADR-0072: A retrieval index narrows, but never authorizes

- **Status:** Accepted
- **Date:** 2026-08-01

## Context

ADR-0004 shipped `Memory::recall` as an exact `(subject, predicate)` lookup,
deliberately: clause 9 chose "lexical subject/predicate match only" over
embeddings, correctness before retrieval fashion. What that leaves is a store
whose contents are unreachable unless the caller already knows the phrasing used
when the claim was written. `subject = "deploy key"` does not answer a question
about "the deployment credential". A memory nobody can address is, in practice,
a memory nobody has.

The competitive audit's B-CAP-08 records the same gap from the outside: the
comparable products ship default full-text recall over sessions and memory, and
the one that ships recall without provenance has a documented memory-pollution
problem — old facts resurfacing as current answers. Both halves of that are the
brief. Reach is table stakes. Reach that cannot say how old an answer is, is the
failure mode.

Adding an index to this particular store is not a neutral act, because three of
ADR-0002's non-negotiable invariants are stated about the store and are trivially
violated by a projection of it:

- **"Scope before top-k"** — a `MATCH … LIMIT k` over a shared table ranks
  before it filters, so the cap is spent on rows the caller may not read.
- **"Privacy erasure covers all projections"** — `privacy_erase` overwrites a
  person's words with `[erased]` in `claims`. A second copy of those exact words
  in an index is not a stale cache; it is the thing the method exists to destroy,
  still present and still searchable.
- **"Evidence is not instruction"** — the fence has to survive the new path, or
  the new path becomes the way around it.

There is also a bitemporal question that exact recall answers by omission. A
claim closed in knowledge time (`tx_to` set by a `correct`) is dropped in SQL by
`recall`. For an exact lookup that is right — the caller asked what is true. For
a search it is not: the words being searched for are frequently the words of the
superseded version, and returning nothing reads as amnesia while returning it
unmarked reads as truth.

## Decision

**1. Free-text recall is SQLite FTS5 over claim text, and nothing else is
added.** `claims_fts` is an external-content-free FTS5 virtual table over
`(subject, predicate, object)` with `claim_id UNINDEXED`, tokenized
`porter unicode61`. `rusqlite` is already built `bundled` with FTS5 available and
`optimus-kernel::session` already uses it, so this is a new table, not a new
dependency, a new service, or a GPU path. ADR-0004 clause 9's refusal of vectors
and embeddings stands unchanged.

**2. The index narrows; it never authorizes.** A MATCH yields candidate
`claim_id`s and nothing more. Every candidate is re-read from `claims` and put
through the same scope, clearance, and allowed-use gates as exact recall, in the
same order: tenant/user/project, knowledge-start, tombstone, and erasure in SQL
before the candidate cap; sensitivity and allowed-use in Rust after it. A stale,
corrupted, or hand-edited index therefore cannot surface a claim above the
caller's clearance, and cannot resurrect a forgotten one.

**3. A derived index must not outlive the erasure of its source.** `tombstone`,
`privacy_erase`, and `apply_retention` each delete the claim's index row inside
the transaction that closes the claim, so there is no window in which the store
has forgotten something the index still knows. The backfill that builds the index
on an existing store skips tombstoned and erased rows rather than indexing them
and deleting them afterwards — a store erased before this index existed must not
get its text back by being opened by a newer build.

**4. Staleness is labelled, not dropped and not hidden.** Text recall
deliberately omits `recall`'s `tx_to IS NULL OR tx_to > as_of_tx` filter. Every
hit carries a `standing` of `Current`, `NotYetValid`, `Expired`, or `Superseded`,
computed from knowledge time first and world time second, plus a `retention_due`
flag when the claim is past a retention deadline the sweep has not yet applied.
`ClaimStanding::weight` makes the ordering a property of the type: a claim that is
not believed now can never outrank one that is, however well it matches the
words.

**5. Search refuses `ActionAuthorize` at every door.** `recall_text` returns
`ActionAuthorizeUnsupported`, and the console entry points share one
`console_purpose` gate so the refusal cannot hold on `memory_recall` and drift on
`memory_search`. The returned packet carries the same
`EVIDENCE_DATA_NOT_INSTRUCTION_NOT_CAPABILITY` fence as `EvidencePacket`.

**6. The packet reports truncation, never candidate counts.** `truncated` is
computed after the clearance filter. A count of index matches would let a
low-clearance caller learn how many claims above their clearance mention a word,
which is a disclosure the row filter otherwise prevents.

**7. Reach is a separate console method, not an overload.** `memory_search` is a
new method beside `memory_recall`, because one method returning two packet shapes
depending on which parameter was supplied is a worse contract than two methods.

**8. The method declares `ScopePolicy::Host` at birth.** The console handlers
read the claim ledger through a fixed `WriteContext` that never consults
`project_id`, so a caller passing one would get an answer from a different scope
than it asked for. Declaring `Host` makes dispatch refuse the parameter instead
of ignoring it. This is also the first non-`None` row in `METHOD_DOMAINS`: the
scope allowlist was seeded before `memory_search` existed and may only shrink, so
a method born after the seed has no honest way to stay undeclared.

## Alternatives considered

- **Embeddings and a vector index.** Rejected on the same grounds ADR-0004
  rejected it and ADR-0002 named as an alternative: it adds a model dependency
  and GPU headroom pressure to answer a question a lexical index answers, and
  vector-as-truth is exactly the shape that produced the competitor's pollution
  problem. It also makes clause 3 much harder — an erased claim's contribution to
  a shared index structure is not a row deletion.
- **`LIKE '%text%'` over `claims`.** No new table, no backfill, no erasure
  obligation. Also no ranking, no stemming, a full scan per query, and no
  tokenization, so "deploying" would not find "deploy". The absence of a
  projection is genuinely simpler; it is simpler than the feature.
- **Trigger-maintained index.** SQLite triggers on `claims` would keep the index
  synchronized without touching each write path. Rejected because the
  synchronization obligation would then live in schema DDL rather than beside the
  erasure code that owes it, and a future write path added outside the trigger's
  conditions would fail silently. The three redaction methods now name their
  index deletion in the same function that closes the claim.
- **Filter knowledge time in SQL, as `recall` does.** Consistent, and wrong for
  this surface: it turns the most common search — the words of the answer someone
  remembers being given — into an empty result, with no signal that a superseded
  version exists.
- **Return a candidate count for observability.** Useful for tuning, and a
  side channel for the existence of claims above the caller's clearance.
  `truncated` gives the caller the one thing they can act on without giving them
  anything they may not see.
- **Overload `memory_recall` with an optional `text` parameter.** Fewer methods,
  at the cost of a response shape that depends on which parameters were sent.

## Reasons

- The invariants that make this store worth having are stated about `claims`.
  The only way a projection keeps them is by holding no authority of its own, so
  that the guarantees are enforced once, in the place they were written, and the
  index is free to be wrong without being dangerous.
- Erasure is the invariant with the shortest fuse. Every other index defect
  degrades quality; this one silently retains what someone asked to have
  destroyed, and it does so invisibly, because the source row looks erased.
- Provenance UX is the actual differentiator here, and it is not a UI concern —
  `standing` and `retention_due` are computed where the temporal truth lives and
  travel on the hit, so no consumer can render a search result that omits how old
  it is.
- Ranking staleness below currency in the type, rather than in a caller's sort,
  means the refusal cannot be forgotten by the next surface that consumes these
  hits.

## Consequences

- `memory.db` gains a `claims_fts` table. Existing stores backfill once, on the
  first open by a build that has this code; the backfill is skipped thereafter by
  a non-empty check.
- Every claim write pays one additional index insert inside the existing write.
- A caller can now reach claims without knowing their phrasing, which is the
  point, and is also a real widening of what one `WriteContext` can surface
  within its own scope and clearance. Nothing crosses a scope or clearance line
  that could not already be crossed by guessing the exact subject.
- `optimus-memory/src/lib.rs` shrank from 1066 to 966 production lines by moving
  the three redaction methods to `redaction.rs`; the module-size baseline
  ratchets down accordingly and cannot return.
- `docs/maps/memory-and-retrieval.md`'s statement that no full-text index exists
  in the workspace is now false for `optimus-memory` and is updated with this
  change. No vector, embedding, reranking, or GPU index exists.

## Risks

- **An authorized match ranked below the 512th lexical candidate within one
  scope is not returned.** Accepted, and documented on the constant. The cap is
  applied in SQL before clearance is known — the same ordering `recall` uses —
  because the alternative is loading a whole scope into memory per query. The
  bound is far past any realistic per-project claim count.
- **The index can drift from `claims` if a future write path bypasses
  `insert_claim`.** Mitigated by clause 2 rather than by hope: drift costs recall,
  never authorization. A drifted index can fail to find a claim, or offer a
  candidate that the re-read then rejects.
- **A superseded claim is returned where exact recall would return nothing.**
  Deliberate. It is labelled, sorted last, and `abstained` is true whenever
  nothing `Current` matched, so a caller that reads only `abstained` still
  behaves conservatively.
- **FTS5 syntax in user text.** `match_expression` strips everything that is not
  alphanumeric, `_`, or `-`, then quotes and prefix-matches each token, so a
  search box cannot become a query console and a malformed expression cannot
  become an error instead of an empty result.

## Evaluation evidence

- `crates/optimus-memory/tests/text_recall_contracts.rs` (14 tests) — erase,
  tombstone, and a retention sweep each leave nothing searchable; a store erased
  before the index existed does not get its text back when it backfills; an
  above-clearance claim never matches and does not set `truncated`; a claim in
  another project never matches; a corrected claim is returned labelled
  `Superseded` beside its `Expired` post-correction snapshot rather than dropped;
  a stale claim that matches *better* still sorts below a current one; a
  not-yet-valid claim is labelled rather than presented as true; a
  punctuation-only query returns an empty packet rather than a SQLite error; FTS5
  operators in user text reach the store as ordinary words.
- `crates/optimus-memory/src/text_recall.rs` inline tests — MATCH expression
  construction, including FTS5 operator stripping.
- `crates/optimus-host/src/consoles.rs` inline tests — `memory_search` refuses
  `ActionAuthorize` through the same gate as `memory_recall`, returns fenced hits
  carrying `standing` and `retention_due`, and treats absent or unmatchable text
  as an abstention rather than an error.
- `crates/optimus-host/src/router.rs` inline tests — dispatch refuses
  `memory_search` with a `project_id` before the handler runs, and the frozen
  registry contract carries the declaration.

## Conditions for reconsideration

- Lexical recall measurably misses questions users actually ask, in a way
  stemming and prefix matching cannot close. That is the argument for dense
  retrieval, and it should be made with a benchmark rather than by analogy to
  what other products ship.
- A second derived projection of claim text appears, at which point clause 3's
  per-method deletion should become one enumerated set of projections rather than
  three hand-written calls.
- The candidate ceiling is reached in a real store, rather than in principle.
- Claim scope gains a dimension the SQL filter does not carry, which would move
  authorization work from the fast path into the Rust pass.

## Relevant code

- `crates/optimus-memory/src/text_recall.rs`
- `crates/optimus-memory/src/redaction.rs`
- `crates/optimus-memory/src/lib.rs`
- `crates/optimus-host/src/consoles.rs`

## Relevant tests

- `crates/optimus-memory/tests/text_recall_contracts.rs`
- `crates/optimus-memory/src/text_recall.rs` (inline)
- `crates/optimus-host/src/consoles.rs` (inline)
