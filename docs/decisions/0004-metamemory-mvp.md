# ADR-0004: MetaMemory MVP (Phase 2)

## Status

Accepted — 2026-07-18

## Context

ADR-0002 locked memory invariants. Phase 0/1 seeded an append-only Work Graph ledger. Phase 2 must make **semantic memory** evidence-native so Optimus exceeds Hermes’ thin pins + bolt-on providers on correctness and security—not on embedding fashion.

## Decision

Ship crate `optimus-memory` with:

1. **Append-only experience ledger** (memory-scoped events, separate DB file `memory.db` under home).
2. **Bitemporal claims** with `valid_from`/`valid_to` (world) and `tx_from`/`tx_to` (knowledge).
3. **Correction** supersedes prior claim in valid-time without destroying history; pre-correction knowledge views remain queryable.
4. **Conflict preservation** when two active claims contradict; no silent last-write-wins.
5. **`EvidencePacket` recall** returning fenced data: origin, trust, authority, allowed_uses, current/historical/conflict partitions, citations.
6. **Purpose enum** on recall: `Inform` | `Constraint` | `ProcedureLookup` | `ActionAuthorize`.
   - `ActionAuthorize` **always fails closed** in MVP (no capability service yet).
7. **Write gate**: untrusted origins cannot self-assign `Trusted` trust or `Action` allowed-use; authority is capped by authenticated `WriteContext`.
8. **Scope before ranking**: tenant/user/project filters applied before candidate limit.
9. **No vectors/embeddings in MVP** — lexical subject/predicate match only (RTX headroom; correctness first).

## Non-goals (Phase 2)

- Full privacy cryptographic erasure across WAL/freelist
- Graph expansion / dense retrieval
- Hermes MEMORY.md auto-mirror
- Skills procedural registry (Phase 3)

## Public seam

```text
Memory::open(path)
Memory::remember(ctx, ClaimDraft) -> ClaimId
Memory::correct(ctx, Correction) -> ClaimId
Memory::recall(ctx, RecallQuery) -> EvidencePacket
```

## Consequences

- CLI may later expose `optimus memory …`; Phase 2 is library + tests.
- Runtime integration optional; memory is independently correct.
