---
doc_id: architecture-state-memory
doc_type: explanation
plane: current
status: current
authority: canonical
summary: State and persistence, memory/retrieval planes, and model routing — current behaviour.
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: architecture
owns:
  - crates/optimus-memory/src/lib.rs
  - crates/optimus-store/src/lib.rs
  - crates/optimus-graph/src/lib.rs
---

# State, memory, and model routing

## State and persistence

**Confirmed current behaviour:** state is split across these stores under an
Optimus home:

| Store | Owner | Contents |
|---|---|---|
| `optimus.db` | store/runtime | Jobs, nodes, approvals, ordered events, campaign plans, schema metadata, and campaign projection caches. |
| `memory.db` | memory | Evidence ledger and bitemporal claims. |
| `skills.db` | skills | Skill versions, permissions, outcomes, and events. |
| `sessions.db` | kernel/session | Session title, loaded pack names, serialized messages, and hash-only durable effect causal links. |
| `execution.db` | kernel/execution | Versioned execution manifests, exact model/tool hashes and outcomes, ordered full tool lifecycle events, monotonic duration fields, ordered timing events, terminal status, trace links, and replay classification. |
| `project-authority.json` | kernel/project authority | Versioned canonical project roots, primary-root selection, and consumed native selection grants; renderer project state is not authority. |
| `cron.db` | kernel/cron | Interval schedules, exact lease owner/generation/token/deadline state, and latest status. |
| `gateway/gateway.db` plus files | kernel/gateway | Authoritative message claims/attempts/terminal outbox JSON plus reconciled inbox/outbox/processed/failed adapter files. |
| caller-selected agent registry/invocation DBs | kernel/agent | Immutable descriptor versions plus accepted invocation projections, ordered events, retry lineage, cancellation, terminal results, and validated effect links. |
| caller-selected workflow registry DB | kernel/workflow | Immutable validated workflow definitions; not workflow execution state. |
| caller-selected replay DB | kernel/replay | Immutable content-addressed fixture bundles and one terminal replay report per bundle. |
| caller-selected trace DB | kernel/trace | Canonical spans, ordered events, and one terminal span outcome. |
| `routing.db` telemetry tables | kernel/telemetry | Provenance-bound provider/model outcome, latency, and cost observations. |
| caller-selected evaluation DB | kernel/evaluation | Immutable candidate-bound baseline reports. |
| `workspace/.optimus/browser_state.json` | kernel/browser | Last HTTP page and bounded navigation history. |
| renderer local storage | React presentation | Versioned pane geometry, theme/density, local project `rootPaths[]`/primary root, session-to-project assignment, pins, and expansion state. This is not runtime permission authority. |

**Confirmed current behaviour (P18):** process-local durability is multi-file
SQLite under one home. There is **no** distributed transaction across those DBs.
Operators use `optimus doctor` (schema inventory + Work Graph quarantine) and
`optimus doctor backup-list` / durability-and-backup.md (merged)
for the backup path set (including workflow/agent ledgers). Universal retention
and a single migration framework across all stores remain residual.

**Confirmed residual:** Campaigns and Work Graph jobs share `optimus.db`.
Cross-contract agent/session links reconcile committed identities; they are not
distributed transactions.

**Confirmed current behaviour:** complete Work Graph job creation and later
projection/event transitions are transactional. Legacy partial state is
diagnosed and quarantined before execution. Terminal-event uniqueness is
enforced in storage. Effect intent is durable before I/O; `WriteFile` uses
temporary replacement and receipts, while a crashed command attempt is marked
ambiguous and cannot be blindly replayed.

**Confirmed current behaviour:** cron due work is claimed transactionally and
stale owners cannot complete after expiry, takeover, disable, or release.
Gateway message UUIDs are ingested idempotently; exact leased attempts own one
terminal outcome and deterministic outbox/archive materialization is reconciled
without rerunning the turn. Durable session tool messages are persisted with an
exact job/node/attempt/effect hash and receipt hash in the same session
transaction before the next model step.

## Memory and retrieval

**Confirmed current behaviour:** runtime MetaMemory is separate from sessions,
skills, gateway state, and this Engineering Memory. Claim writes derive trust
from origin and cap it by the authenticated write context. Recall is scoped by
tenant/user/project before selection, supports valid-time and transaction-time
views, detects conflicting objects, returns citations, and rejects
`ActionAuthorize`.

**Confirmed current behaviour:** current recall is deterministic SQLite
filtering and ordering by scope, optional exact subject/predicate, and temporal
fields. Free-text recall adds a `claims_fts` FTS5 lexical index that supplies
candidate ids only: every candidate is re-read from `claims` and re-gated, and
each hit carries the bitemporal standing that ranks stale below current
(ADR-0072). There is no embedding, vector search, reranker, knowledge graph, or
GPU retrieval implementation in the workspace.

**Confirmed current behaviour:** memory default transaction and event times use
an injected UTC monotonic clock. Sensitivity and allowed-use filters apply before
recall limiting; correction preserves sensitivity; retention, tombstone, and
privacy erase are idempotent scoped transitions with sanitized audit records.

## Model routing

**Confirmed current behaviour:** a canonical typed route resolver validates
provider/model ownership, required capabilities, local-only privacy, cost
budget, and explicitly bounded fallback before optional fresh telemetry
filtering/ranking, then persists route decisions. Telemetry is tied to exact
route provider/model/trace identity and cannot authorize a statically denied
candidate. Auto is a request selector that chooses connected Codex, configured
OpenAI-compatible, or offline in fixed order; it is never persisted as the
provider/model that executed. Expiring Codex access without refresh capability
is not connected for this selection. CLI, desktop, cron, and gateway use the
same resolver. Provider-specific wire parsing remains in the adapters.

**Confirmed current behaviour:** Codex retries once after an HTTP failure with
system plus last-user messages and no reasoning effort. This provider-local
retry is distinct from cross-provider fallback.

**Known routing debt:** normalized reasoning effort and fast mode are sent by the
Codex adapter; the OpenAI-compatible request mapper does not transmit them.

**Unknown or unresolved behaviour:** token accounting, live billing integration,
automatic runtime-failure fallback, evaluation-report-driven selection, and
local-model/GPU adapters are not implemented.
