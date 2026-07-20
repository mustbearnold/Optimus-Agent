# Phase 2 verification — 2026-07-18

## Scope delivered

Per ADR-0002 / ADR-0004 — crate `optimus-memory`:

| Capability | Behavior |
|---|---|
| Append-only ledger | `ledger` table for remember/correct/conflict events |
| Bitemporal claims | `valid_from`/`valid_to` + `tx_from`/`tx_to`; correction closes prior knowledge version and inserts post-T snapshot + new fact |
| EvidencePacket | Fenced recall with current / historical / conflicts / citations / abstain |
| Action purpose | `RecallPurpose::ActionAuthorize` **fails closed** (no capability service) |
| Write gate | Untrusted origin → Untrusted trust; **never** `AllowedUse::Action` |
| Scope-before-limit | tenant/user/project filters before `limit` |
| Conflict semantics | Disagreeing open claims → conflicts partition, not silent LWW |

No embeddings (RTX headroom; correctness first).

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all --check` | pass |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | pass |
| `cargo test --workspace` | **15 passed** |
| `optimus doctor` | phase 2 metamemory |

### MetaMemory tests (7)

- remember + recall current + fence label
- bitemporal correction matrix (post-T Feb→vim, post-T Apr→helix, pre-T Apr→vim)
- conflict ≠ last-write-wins
- action authorize fails closed even with “authorization” text
- untrusted cannot elevate trust/action use
- cross-project scope isolation under limit=1
- empty recall abstains

Phase 0 + Phase 1 suites remain green.

## Not yet

- Privacy cryptographic erasure / WAL freelist wipe
- Dense/vector retrieval
- Runtime auto-injection of EvidencePackets into the agent loop
- Core pin files / skills procedural registry (Phase 3)
