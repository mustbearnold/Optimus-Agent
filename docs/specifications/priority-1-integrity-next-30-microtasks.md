---
knowledge_type: specification
status: historical
owns:
  - docs/specifications/priority-1-integrity-next-30-microtasks.md
  - docs/contracts/high-risk-contracts.md
  - docs/decisions/0021-owned-execution-and-causal-delivery.md
  - Cargo.toml
watches:
  - crates/optimus-packs/src/**
  - crates/optimus-kernel/src/**
  - crates/optimus-memory/src/**
  - crates/optimus-runtime/src/**
  - crates/optimus-store/src/**
  - apps/optimus-cli/src/**
  - apps/optimus-desktop/src/**
covers:
  - docs/specifications/priority-1-integrity-next-30-microtasks.md
  - docs/contracts/high-risk-contracts.md
  - docs/decisions/0021-owned-execution-and-causal-delivery.md
  - Cargo.toml
depends_on:
  - docs/contracts/high-risk-contracts.md
  - docs/decisions/0021-owned-execution-and-causal-delivery.md
validated_by:
  - crates/optimus-eval/tests/integrity_integration.rs
  - apps/optimus-cli/tests/gateway_http.rs
  - scripts/test_engineering_memory.py
last_verified_commit: b59b90766fd3b001725dd1542a05326a1d4b4894
---

# Priority-1 integrity tranche: next 30 micro-tasks

- **Status:** Completed through m56; decomposed execution recorded in the 100-nano-task specification
- **Task range:** m27–m56
- **Execution authority:** complete all thirty tasks locally, in order, without
  subagents. The later explicit GitHub workflow authorization superseded the
  original no-commit/no-push delivery restriction; verified milestones were
  pushed directly to `main` without branches or pull requests.
- **Baseline tree:**
  `3e5c42b15665724596084cf21f40720bea3e9e133ccb0db96d470bba0677f769`
  over 162 indexed files.

## Problem Statement

Optimus now has a durable Work Graph, exact approvals, owned Windows commands,
cooperative model cancellation, leased campaigns/cron/gateway work, authenticated
loopback APIs, and successful-tool session causality. The remaining Priority-1
contracts are still fragmented:

1. Tool results are tool-specific strings or JSON rather than one typed envelope.
2. Failed/cancelled turns and restart continuation are incomplete.
3. Provider interpretation differs by CLI, desktop, cron, and gateway; policy and
   fallback are not centralized or durably explained.
4. Replay evidence does not bind every model/tool dependency into a versioned
   execution manifest.
5. Codex credentials remain plaintext JSON.
6. Memory uses a fixed audit timestamp and lacks sensitivity/retention policy.
7. There is no typed agent invocation substrate or general workflow contract.
8. Cron/gateway cancellation, delivery acknowledgements, retry, and dead-letter
   semantics remain partial.

Building agents or general workflows before stabilizing outcomes, routing,
provenance, credentials, and memory policy would amplify these inconsistencies.

## Solution

Implement one dependency-ordered integrity tranche:

1. Establish a canonical tool outcome protocol and migrate every available tool
   plus all provider/UI/API consumers.
2. Persist failed/cancelled turns and continue an interrupted turn without
   duplicating accepted user/tool segments.
3. Finish cron/gateway terminal attempt semantics.
4. Centralize provider/model identity, routing constraints, decision evidence,
   and bounded fallback.
5. Persist versioned execution manifests and honest replay classifications.
6. Encrypt user credentials on Windows with safe plaintext migration.
7. Add real clocks, sensitivity, and retention to evidence memory.
8. Introduce narrow typed agent and workflow substrates whose lifecycle rules
   reuse the existing runtime rather than bypass it.
9. Update architecture authority, regenerate Engineering Memory, run every local
   gate, and freeze the exact candidate.

## User Stories

1. As a model adapter, I want every tool result to use one typed envelope so that
   success, failure, cancellation, ambiguity, and artifacts are unambiguous.
2. As a UI consumer, I want stable outcome fields so that rendering does not parse
   tool-specific prose.
3. As a runtime operator, I want replay/idempotency classification attached to
   every tool result so that retries cannot be guessed.
4. As a session user, I want failed and cancelled turns retained so that errors do
   not erase what happened.
5. As a returning user, I want an interrupted accepted turn to continue without
   duplicating its user message or durable effects.
6. As a scheduler operator, I want cancellation to fence cron completion so that
   cancelled work cannot advance schedules.
7. As a delivery operator, I want bounded retries, acknowledgement, and dead-letter
   state so that poison messages do not loop forever.
8. As a surface implementer, I want one provider/model resolver so that CLI,
   desktop, cron, and gateway interpret names identically.
9. As a privacy-conscious user, I want locality/privacy constraints enforced
   before any provider network call.
10. As a budget owner, I want token/cost limits checked before routing and fallback.
11. As an auditor, I want the selected route and fallback reason persisted.
12. As an evaluator, I want a versioned execution manifest containing hashes of
   prompts, tools, models, workflows, and relevant inputs.
13. As an operator, I want replay reports to say deterministic, fixture-replayable,
   model-nondeterministic, external-nondeterministic, destructive, or ambiguous.
14. As a Windows user, I want OAuth credentials encrypted for my account and
   legacy plaintext migrated without token loss.
15. As a memory user, I want real event times so chronology is meaningful.
16. As a memory owner, I want sensitivity labels to constrain allowed use.
17. As a privacy owner, I want deterministic retention, tombstone, and erase audit
   behavior.
18. As an agent author, I want versioned typed request/result contracts so agents
   cannot invent ad-hoc envelopes.
19. As a security reviewer, I want registered agents to declare exact permissions
   and tool availability.
20. As an orchestrator, I want every invocation to have one durable terminal
   result and cooperative cancellation.
21. As a workflow author, I want one versioned lifecycle schema covering triggers,
   inputs, dependencies, retry, timeout, cancellation, approval, and outputs.
22. As an operator, I want jobs, campaigns, cron, and gateway represented through
   explicit adapters rather than falsely treated as identical implementations.
23. As an evaluator, I want cross-contract tests proving outcomes, routing,
   provenance, agents, and workflows compose without bypassing SmartDeny.
24. As an engineer, I want current ADRs and generated Engineering Memory to match
   source and tests exactly.

## Implementation Decisions

### Canonical tool outcome protocol

- The canonical envelope is versioned and discriminated by terminal kind:
  `succeeded`, `failed`, `cancelled`, or `ambiguous`.
- Every envelope carries stable tool-call ID, canonical tool ID, summary, typed
  data, bounded artifacts, error details when applicable, replay class, and
  optional durable effect provenance.
- Errors have stable machine codes and bounded/redacted public messages.
- Artifact references carry stable identity, media type, optional path/URI, hash,
  and byte count; they do not embed unbounded bytes.
- `ToolDesc` owns output schema and replay declaration beside its input schema.
- Kernel dispatch returns the canonical type; string serialization occurs only at
  provider/transport boundaries.
- Runtime job status and effect-attempt identity are mapped into the envelope.
- Output validation is fail-closed before a tool message advances the session.

### Turn lifecycle and restart

- `sessions.db` gains a typed turn projection and append-only turn events.
- Accepted user segments, tool segments, failures, cancellation, and final text
  have explicit states and exactly one terminal turn outcome.
- A resumable turn stores the next model step and canonical transcript segment.
- Continuing a turn uses its identity and does not append a second user message.
- Runtime effect links remain authoritative references; no cross-database atomicity
  is claimed.

### Cron and gateway

- Cron cancellation is durable, invalidates the lease generation, records one
  terminal attempt, and never advances `next_run_unix`.
- Gateway messages have bounded retry policy, explicit acknowledgement state, and
  dead-letter terminal state.
- Cancellation/release is exact-owner fenced.
- Reconciliation may materialize committed state but must never rerun model work.

### Shared routing

- Provider and model IDs are canonical parsed types, not arbitrary branch strings.
- One catalog describes adapter, locality, capabilities, context limit, and
  supported controls.
- One resolver is used by all surfaces and rejects unknown IDs.
- A route request includes required capabilities, privacy/locality, input budget,
  optional cost ceiling, and allowed fallback chain.
- Route decisions are persisted with policy/catalog version, request hash,
  selected route, and reason.
- Fallback is finite, policy-approved, and never crosses locality/privacy bounds.

### Provenance and replay

- Every turn gets a versioned execution manifest with hashes for system prompt,
  messages, tool descriptors, loaded packs, provider/model settings, workflow or
  agent identity, and relevant policy versions.
- Model calls and canonical tool outcomes link to the manifest.
- Replay reports classify each stage honestly and list missing dependencies;
  model/external stages are never called deterministic without fixtures.

### Credential storage

- Authentication persistence is behind a narrow store interface.
- Writes use temporary sibling replacement and restrictive Windows ACL checks.
- On Windows, token payloads are encrypted with user-scoped DPAPI.
- Legacy plaintext is read once, encrypted atomically, reread/verified, and only
  then removed from the new authoritative payload.
- Status and error surfaces never expose token plaintext or encrypted blobs.
- Non-Windows behavior remains explicit and fail-closed; this Windows-first tranche
  does not invent a portable encryption guarantee.

### Memory policy

- Memory uses an injected clock; production uses system UTC and tests use a fake.
- Event times cannot move backwards within one store operation sequence.
- Sensitivity is explicit and defaults conservatively for migrated records.
- Allowed-use checks combine trust, source, sensitivity, and request purpose.
- Retention evaluation is deterministic at an injected time.
- Privacy erase removes protected content while retaining the minimum non-secret
  audit tombstone required to prove erasure.

### Agent substrate

- Agent request/result types are versioned, typed, and intentionally narrow.
- Requests include task, context references, constraints, permission envelope,
  available canonical tools, cancellation identity, and budgets.
- Results include terminal kind, evidence, artifacts, actions, unresolved items,
  confidence category, cost usage, and trace identity.
- The registry validates unique ID/version, exact permissions, available tools,
  and schema compatibility.
- Invocation state is durable and has exactly one terminal outcome.
- Agent effects still route through kernel/runtime/SmartDeny; registry membership
  never authorizes effects by itself.

### Workflow substrate

- Workflow definitions are versioned data with typed trigger, inputs, outputs,
  dependencies, retry, timeout, cancellation, approval, validation, rollback
  declaration, observability, and terminal outcomes.
- Existing jobs, campaigns, cron, and gateway are represented by explicit adapters
  with honest capability differences.
- Shared policy validates lifecycle completeness; it does not pretend every
  adapter supports rollback, replay, or parallel execution.

## Testing Decisions

- Behavioral changes use focused RED–GREEN–REFACTOR at public seams.
- Persistence tests use independent SQLite connections and deterministic failure
  injection for concurrency/crash windows.
- Security tests assert rejected behavior and absence of side effects.
- Routing tests cover unknown IDs, policy mismatch, no eligible route, bounded
  fallback, and cross-surface equivalence.
- Credential tests use temporary homes and synthetic tokens only. No real
  credential file is read or printed.
- Memory tests use injected clocks and fixed fixtures.
- Agent/workflow tests assert exact terminal uniqueness, cancellation, permission
  denial, and SmartDeny preservation.
- Compatibility tests cover provider transcript mapping, CLI/gateway HTTP, desktop
  native/HTTP bridge, and full Playwright.
- Canonical completion gates remain formatting, strict Clippy, all-target/all-
  feature tests, strict rustdoc, full desktop Playwright, Engineering Memory
  generation/validation/tests/currentness, and independent tree hashing.

## Ordered Micro-task Contract

### m27 — Specification and baseline

- Freeze this specification and the 30-task order.
- Confirm the prior exact tree and clean Engineering Memory currentness.
- **Proof:** specification frontmatter resolves; currentness reports no stale files.

### m28 — Canonical outcome types

- Add versioned outcome, error, artifact, replay, and durable provenance types.
- Reject empty IDs, invalid hashes, unbounded summaries, and inconsistent variants.
- **Proof:** focused serialization and validation RED/GREEN tests.

### m29 — Tool descriptor output contract

- Add output schema and replay declaration to canonical descriptors/catalog checks.
- **Proof:** catalog rejects absent/drifting/unsupported output contracts.

### m30 — Kernel canonical seam

- Change dispatch to return typed outcomes and serialize only at transcript edges.
- **Proof:** one scripted-turn test observes canonical success and error envelopes.

### m31 — Tool migration and validation

- Migrate all ten available tools and validate every emitted envelope.
- **Proof:** table-driven tests cover all available `ToolId`s and malformed output.

### m32 — Consumer compatibility

- Update provider transcript conversion, loopback/native responses, UI formatting,
  and E2E assumptions.
- **Proof:** provider tests, CLI/desktop tests, and focused Playwright.

### m33 — Failed-turn ledger

- Add typed turn/event schema and persist failure/cancellation atomically with the
  accepted transcript boundary.
- **Proof:** provider error, tool error, cancellation, and repeated terminalization.

### m34 — Restart continuation

- Continue a nonterminal turn by ID without duplicating user/tool segments.
- **Proof:** close/reopen kernel around a durable tool result and finish once.

### m35 — Cron cancellation

- Add cancellation request, lease fencing, and one terminal attempt.
- **Proof:** cancel pending/running, stale completion, repeated cancellation.

### m36 — Gateway dead letter

- Add cancellation, bounded attempts, acknowledgement, retry scheduling, and
  dead-letter terminal state.
- **Proof:** poison message reaches dead letter once; acknowledgement is idempotent.

### m37 — Provider identity

- Define canonical provider/model IDs and shared catalog metadata.
- **Proof:** aliases normalize; unknown/cross-provider IDs reject.

### m38 — Shared route resolution

- Route every user surface through one resolver.
- **Proof:** a table-driven cross-surface test produces identical decisions/errors.

### m39 — Routing policy

- Enforce capability, privacy, locality, context, and budget before adapter call.
- **Proof:** spy adapter is never invoked on policy denial.

### m40 — Route ledger and fallback

- Persist decisions and run a finite policy-approved fallback chain.
- **Proof:** exact attempt order, bounded loop, reason/provenance, no privacy escape.

### m41 — Execution manifest

- Persist versioned dependency hashes for each turn.
- **Proof:** stable fixture hash and changed-dependency sensitivity.

### m42 — Replay provenance

- Link model/tool calls and emit honest replay reports.
- **Proof:** mixed deterministic/model/external/ambiguous fixture classification.

### m43 — Auth-store boundary

- Isolate credential persistence, add atomic replacement and ACL verification.
- **Proof:** injected write failure preserves old synthetic credentials.

### m44 — DPAPI migration

- Encrypt Windows payloads and migrate synthetic legacy plaintext safely.
- **Proof:** disk lacks token plaintext; roundtrip works for current user; corrupt
  ciphertext and interrupted migration fail closed.

### m45 — Memory clock

- Replace fixed timestamps with injected UTC clock and monotonicity checks.
- **Proof:** fake-clock temporal sequence and backward-clock rejection/clamping rule.

### m46 — Memory sensitivity

- Add sensitivity labels and allowed-use filtering.
- **Proof:** sensitive claims cannot flow to forbidden purposes despite high trust.

### m47 — Memory retention

- Add deterministic retention and privacy-erasure audit behavior.
- **Proof:** boundary-time expiry, idempotent erase, no recoverable erased content.

### m48 — Agent contracts

- Add versioned typed request/result validation.
- **Proof:** valid roundtrip and invalid permission/budget/terminal combinations.

### m49 — Agent registry

- Register narrow agent descriptors with version/tool/permission checks.
- **Proof:** duplicate identity, unavailable tools, or broadened permissions reject.

### m50 — Agent lifecycle

- Persist invocation, cancellation, events, and one terminal result.
- **Proof:** cancel/retry/reopen preserves one terminal outcome and no effects bypass
  SmartDeny.

### m51 — Workflow schema

- Add versioned workflow lifecycle definitions and validation.
- **Proof:** every required field and terminal/cancel/retry invariant is enforced.

### m52 — Workflow adapters

- Project job/campaign/cron/gateway through honest adapters.
- **Proof:** registry exposes exact supported/unsupported capabilities per adapter.

### m53 — Workflow policy

- Normalize cancellation, retry, approval, and observability semantics.
- **Proof:** cross-adapter conformance suite with explicit unsupported outcomes.

### m54 — Integration and evaluations

- Add offline trajectories and cross-contract integration tests.
- **Proof:** tool → manifest → session link → agent/workflow terminal path and all
  denial/cancellation paths.

### m55 — ADRs and current documentation

- Record decisions and update architecture/contracts/maps plus this spec’s final
  disposition.
- **Proof:** validation reports no contradictory current claims.

### m56 — Engineering Memory and final freeze

- Regenerate generated memory, run every local gate, independently hash indexed
  bytes, and freeze the candidate.
- **Proof:** zero test failures, zero validation errors, zero stale documents, zero
  file-hash mismatches.

## Out of Scope

- Remote/hosted agents or subagents.
- Parallel agent execution or distributed orchestration.
- Production deployment, installation, publishing, commits, or pushes.
- Paid model calls or real credential migration; tests use synthetic temporary
  homes only.
- A vector database, GPU retrieval, semantic reranker, or knowledge graph.
- Cross-machine workflow coordination or an external message broker.
- Claiming exact replay for nondeterministic model or external stages.
- Replacing SQLite with a distributed database.

## Further Notes

- Source and executable tests outrank this specification.
- Generated Engineering Memory JSON is never manually edited.
- Every new durable state machine must define success, failure, cancellation,
  retry, and exactly one terminal outcome.
- Every high-risk effect remains subject to SmartDeny; no agent/workflow/routing
  abstraction may widen permissions.
- The tranche may stop only on a genuine unrecoverable blocker. A failed focused
  gate stays within its owning task until corrected.
