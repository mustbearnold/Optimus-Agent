---
doc_id: decisions-0022-versioned-agent-and-workflow-contracts
doc_type: decision
plane: decision
status: current
authority: record
summary: Decision record for ADR-0022: Versioned agent and workflow contracts, including its context, consequences, and current documentary status.
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: decision
covers:
  - crates/optimus-eval/src/eval.rs
  - crates/optimus-kernel/tests/agent_contracts.rs
  - crates/optimus-kernel/tests/workflow_contracts.rs
  - crates/optimus-eval/tests/integrity_integration.rs
depends_on:
  - docs/decisions/0021-owned-execution-and-causal-delivery.md
validated_by:
  - crates/optimus-kernel/tests/agent_contracts.rs
  - crates/optimus-kernel/tests/workflow_contracts.rs
  - crates/optimus-eval/tests/integrity_integration.rs
last_verified_commit: b59b90766fd3b001725dd1542a05326a1d4b4894
---

# ADR-0022: Versioned agent and workflow contracts

- **Status:** Accepted
- **Date:** 2026-07-20

## Context

The kernel had durable jobs, campaigns, cron claims, gateway delivery, sessions,
and canonical tools, but no typed specialist-agent boundary or general workflow
definition. Calling campaigns “multi-agent” would have overstated deterministic
effect steps. Adding an agent prompt without durable identity, permissions,
cancellation, and terminal-state enforcement would have weakened existing
runtime guarantees. Collapsing jobs, campaigns, cron, and gateway into one store
would also have hidden real ownership differences and introduced unsafe
cross-database atomicity claims.

## Decision

1. `optimus-kernel::agent` owns versioned agent identities, descriptors,
   requests, results, permission envelopes, budgets, and immutable descriptor
   registration. There are no built-in specialist definitions yet; the
   implemented substrate is not evidence that a particular specialist exists.
2. Registry validation checks canonical available `optimus-packs::ToolId`
   values and exact host permission ceilings. A request must also remain within
   its descriptor. Registration grants no runtime effect permission.
3. `AgentInvocationStore` owns accepted invocation projections and ordered
   events. Begin, cancellation request, and settlement are transactional;
   storage permits exactly one terminal outcome: succeeded, failed, cancelled,
   or ambiguous. Retries receive new invocation identities and explicit lineage.
4. Durable effect links are accepted only after `Runtime` confirms the exact
   terminal job, node, attempt, effect hash, and receipt hash. The runtime and
   SmartDeny remain authoritative for effects and approvals.
5. `optimus-kernel::workflow` owns versioned triggers, typed JSON-schema ports,
   dependency graphs, bounded retry/timeout declarations, cancellation,
   approval, rollback, observability, exact terminal declarations, optional
   typed agent references, and immutable workflow registration.
6. Jobs, campaigns, cron, and gateway retain their existing stores and
   execution semantics. Capability descriptors and status adapters make support
   or non-support explicit. The general workflow contract is not a new universal
   scheduler and does not move effect execution out of the runtime.
7. Cross-contract tests verify causal identity and terminal agreement after
   independent store commits/reopens. No transaction is claimed across
   `sessions.db`, `optimus.db`, agent invocation storage, or workflow storage.
8. The offline integrity evaluation requires observed evidence for sensitivity
   denial, SmartDeny approval, route-policy denial, cooperative cancellation,
   stale-completion fencing, and gateway dead-letter behavior.

## Alternatives considered

- **Prompt-only specialist agents.** Rejected because prompts cannot enforce
  tool availability, permission ceilings, cancellation, or terminal uniqueness.
- **One global workflow database.** Rejected because existing subsystem owners
  have different authorities and migration/recovery rules.
- **Treat adapter capability gaps as supported defaults.** Rejected because it
  would invent retry, approval, rollback, or cancellation semantics.
- **Link effects by caller-provided IDs only.** Rejected because unverified
  provenance could claim effects that never committed.
- **Use one invocation ID for retries.** Rejected because it obscures attempt
  identity and can admit stale completion.

## Consequences

- Agent and workflow contracts are implemented and reusable, but no built-in
  specialist definition or general workflow executor is registered.
- Agent stores add local SQLite files chosen by their caller. Workflow registry
  storage is likewise caller-owned; neither is automatically opened by
  `Kernel` yet.
- Permission envelopes are bounded declarations. Runtime policy, filesystem
  confinement, and SmartDeny remain the actual effect authorization boundary.
- Workflow adapters preserve owner-specific states. Unknown persisted status
  strings fail closed rather than being coerced to success.
- Cross-store terminal agreement is reconciled by exact identity and tests, not
  by distributed transactions.

## Reasons

The chosen boundaries put deterministic validation and lifecycle state in code,
preserve existing subsystem authority, and make unsupported behavior explicit.
They permit future specialists and workflow definitions without granting prompts
new permissions or claiming one transaction across independent stores.

## Risks and unresolved boundaries

- A future orchestrator must poll/synchronize durable cancellation at bounded
  loop boundaries and must not retain a stale uncancelled token indefinitely.
- No built-in specialist routing policy, parallel agent scheduler, or child
  hierarchy exists.
- No general workflow executor, timer service, rollback engine, or cross-store
  recovery coordinator exists. Adapters document current capability only.
- Permission envelopes use exact declared strings; future path/domain matching
  must add deterministic normalization without broadening existing grants.
- Agent/workflow registries are immutable by identity/version. A future
  deprecation lifecycle needs a separate append-only decision rather than row
  mutation.

## Evaluation evidence

- Agent contract tests cover identity/schema bounds, canonical tool membership,
  permission non-escalation, immutable/reopenable/corruption-safe registration,
  cancellation, stale completion, retry lineage, ambiguity, exact effect links,
  and SmartDeny non-bypass.
- Workflow tests cover schema/graph/policy rejection, immutable registration,
  capability completeness, explicit unsupported behavior, and exact adapter
  status mapping.
- Integration tests cover tool-to-manifest-to-session-to-runtime-to-agent causal
  identity, workflow-to-agent identity, independently reopened terminal
  agreement, and the six-case offline integrity evaluation.

## Conditions for reconsideration

Reconsider store separation only if a concrete workflow needs atomic ownership
across subsystems and a tested migration/recovery design exists. Reconsider the
string-based permission envelope when canonical path/domain/effect capability
types are implemented. Add built-in specialists or general workflow execution
only with narrow owners, typed definitions, permission closure, cancellation,
terminal outcomes, evaluations, and refreshed Engineering Memory.

## Relevant code

- `crates/optimus-kernel/src/agent.rs`
- `crates/optimus-kernel/src/workflow.rs`
- `crates/optimus-eval/src/eval.rs`

## Relevant tests

- `crates/optimus-kernel/tests/agent_contracts.rs`
- `crates/optimus-kernel/tests/workflow_contracts.rs`
- `crates/optimus-eval/tests/integrity_integration.rs`
