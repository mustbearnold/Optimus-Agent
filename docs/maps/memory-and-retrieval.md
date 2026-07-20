---
knowledge_type: memory-map
status: current
covers:
  - crates/optimus-memory/src/**
  - crates/optimus-kernel/src/session.rs
  - crates/optimus-kernel/src/gateway.rs
  - crates/optimus-kernel/src/cron.rs
  - crates/optimus-skills/src/**
  - crates/optimus-runtime/src/campaign.rs
depends_on:
  - docs/decisions/0002-memory-invariants.md
  - docs/decisions/0004-metamemory-mvp.md
validated_by:
  - crates/optimus-memory/tests/**
  - crates/optimus-kernel/tests/kernel_turn.rs
  - apps/optimus-desktop/e2e/03-runtime-and-sessions.spec.js
  - crates/optimus-skills/tests/**
last_verified_commit: null
---

# Memory and retrieval map

## Separation

| System | State | Owner | Purpose |
|---|---|---|---|
| Kernel working state | Confirmed current behaviour | `optimus-kernel::Kernel` | Current message history, capability session, and system prompt for a turn. |
| Session state | Confirmed current behaviour | `SessionStore` in `sessions.db` | Durable title, loaded pack names, serialized messages, and exact hash-only links from durable tool calls to Work Graph attempts. |
| Runtime semantic/episodic memory | Confirmed current behaviour | `optimus-memory` in `memory.db` | Evidence-backed claims, corrections, conflict sets, temporal views, and recall packets. |
| Procedural runtime skills | Confirmed current behaviour | `optimus-skills` in `skills.db` | Versioned reusable procedures with permissions and outcome evidence. |
| Work/campaign state | Confirmed current behaviour | runtime/store campaign and Work Graph tables in `optimus.db` | Durable operational progress with job-derived campaign status; not semantic memory. |
| Gateway state | Confirmed current behaviour | `gateway.db` plus JSON adapter directories | Authoritative leased message attempts and terminal outbox JSON with reconciled file materializations; not semantic memory. |
| Engineering Memory | Confirmed current behaviour | repository `docs/`, `skills/`, `.engineering-memory/` | Development knowledge for building Optimus; never loaded as an authorization source. |
| Project knowledge | Unknown or unresolved behaviour | no implemented owner | Source-backed Aipedia or other target-project knowledge is not implemented as a distinct subsystem. |
| Retrieval indexes | Unknown or unresolved behaviour | no implemented owner | No vector, embedding, full-text, graph, reranking, or GPU index exists in the workspace. |

## Runtime memory contract

**Confirmed current behaviour:** claims carry tenant/user/project scope, subject,
predicate, object JSON, memory type, origin, trust, allowed use, valid-time and
transaction-time bounds, evidence records, provenance, and optional correction
links.

**Confirmed current behaviour:** a `MemoryWriteContext` supplies authenticated
scope, source class, and a maximum trust ceiling. Claimed trust is derived from
origin and capped by the context. Unknown tenant/user/project fields cannot be
selected during recall.

**Confirmed current behaviour:** recall performs exact optional
subject/predicate filtering, applies the requested valid-time and
transaction-time view, groups live claims by fact key, exposes conflict sets,
and returns an evidence packet with citation IDs. Action authorization is
denied as an allowed use.

**Confirmed current behaviour:** correction closes the superseded claim's
transaction-time interval and inserts a new claim linked to the old one.
Feedback records external outcomes; it does not silently rewrite the claim.

## Retrieval behavior

**Confirmed current behaviour:** retrieval is SQLite query/filter/order logic.
It is deterministic for a fixed database and query. The kernel's
`memory_recall` tool passes subject and predicate and serializes the returned
packet into tool text.

**Unknown or unresolved behaviour:** relevance ranking, fuzzy semantic search,
query expansion, vector similarity, temporal scoring, deduplication across
paraphrases, source ranking, context packing, stale-claim detection, and
knowledge-graph traversal are not implemented.

**Planned behaviour:** add replaceable CPU-first adapters and optional GPU
acceleration only after fixture-backed relevance/correctness evaluation. GPU and
CPU results need tolerance/equivalence tests and explicit fallback telemetry.

## Write/read/retention rules

| Concern | Current state |
|---|---|
| Ownership | Confirmed: runtime claims belong to `optimus-memory`; sessions, gateway delivery, cron schedules, and skills use separate stores. |
| Writes | Confirmed: typed ledger operations enforce authenticated scope/trust ceilings. |
| Reads | Confirmed: scoped recall with temporal modes and allowed-use filtering. |
| Provenance | Confirmed: claim origin/evidence/citation IDs are retained. |
| Sensitivity | Unknown: no field-level sensitivity or encryption policy is implemented. |
| Retention | Unknown: no retention, compaction, archival, or quota policy is implemented. |
| Invalidation | Partial: corrections are bitemporal; source-driven invalidation is absent. |
| Deduplication | Partial: exact claim identity/conflict grouping exists; semantic deduplication is absent. |
| Deletion | Partial: tombstone/privacy erase operations exist; repository-wide deletion guarantees are not documented. |
| Evaluation | Partial: memory unit/integration tests exist; precision/recall/temporal benchmark suites do not. |

## Known risks and debt

1. **Confirmed current behaviour:** memory ledger events are stamped with the
   fixed value `2026-01-01T00:00:00Z`; this is unsuitable for production audit
   chronology.
2. **Unknown or unresolved behaviour:** SQLite files, sessions, and `auth.json`
   have no documented at-rest encryption, OS ACL hardening, backup, or retention
   contract.
3. **Unknown or unresolved behaviour:** no transaction spans memory, sessions,
   workflow state, and tool effects. Session progression after a successful
   durable tool result atomically includes an exact attempt/effect/receipt-hash
   link, but the authoritative attempt commits first in `optimus.db`.
4. **Unknown or unresolved behaviour:** no source-revocation process propagates
   into affected claims or generated artifacts.
5. **Unknown or unresolved behaviour:** no retrieval evaluation dataset or
   quality/cost/latency baseline exists.
