---
knowledge_type: specification
status: historical
owns:
  - docs/specifications/priority-1-integrity-100-nanotasks.md
  - docs/contracts/high-risk-contracts.md
  - docs/decisions/0021-owned-execution-and-causal-delivery.md
  - scripts/engineering_memory.py
  - scripts/test_engineering_memory.py
watches:
  - crates/optimus-memory/src/**
  - crates/optimus-kernel/src/**
  - crates/optimus-runtime/src/**
  - crates/optimus-store/src/**
  - docs/architecture/**
  - docs/maps/**
covers:
  - docs/specifications/priority-1-integrity-100-nanotasks.md
  - docs/contracts/high-risk-contracts.md
  - docs/decisions/0021-owned-execution-and-causal-delivery.md
  - scripts/engineering_memory.py
  - scripts/test_engineering_memory.py
depends_on:
  - docs/specifications/priority-1-integrity-next-30-microtasks.md
  - docs/contracts/high-risk-contracts.md
  - docs/decisions/0021-owned-execution-and-causal-delivery.md
validated_by:
  - crates/optimus-memory/tests/metamemory_mvp.rs
  - crates/optimus-eval/tests/integrity_integration.rs
  - scripts/test_engineering_memory.py
last_verified_commit: b59b90766fd3b001725dd1542a05326a1d4b4894
---

# Priority-1 integrity completion: 100 nano-tasks

- **Status:** Completed upon verified delivery of the exact commit containing this disposition
- **Task range:** n001–n100, exactly 100 tasks
- **Parent contract:** m45–m56 of the accepted 30-micro-task specification
- **Repository:** `mustbearnold/Optimus-Agent`
- **Delivery:** verified commits pushed directly to `origin/main`; GitHub Issues are the work ledger; no branches or pull requests

## Execution disposition

n001–n100 were executed in dependency order. Memory, agent, workflow,
integration, and documentation milestones were verified before delivery.
Milestone commits are `f99d789` (agent substrate) and `b59b907` (workflow
contracts/integrity evaluations), following the initial memory and Priority-1
baseline. n100 is completed by the verified `main` delivery and GitHub issue
reconciliation of the exact commit containing this disposition; no scope item
is silently deferred.
- **Execution:** one writer, no subagents, ordered RED–GREEN packets, generated Engineering Memory never edited manually
- **Starting condition:** local `main` has no commits; m45–m47 are interrupted and `cargo check -p optimus-memory` reports six compile errors

## Problem Statement

The first eighteen micro-tasks of the Priority-1 integrity tranche are implemented and focused-gated, but the memory-policy migration was interrupted between schema/API edits and call-site/test updates. The current tree is not compilable, cannot be committed honestly, and therefore cannot establish the first GitHub baseline. Beyond memory recovery, Optimus still lacks typed specialist-agent contracts, a permission-aware durable agent lifecycle, a general workflow schema with honest adapters, cross-contract evaluation, current architectural authority, regenerated Engineering Memory, and a verified exact-tree release identity.

The work must be small enough to stay GREEN after each bounded packet, while preserving the accepted architecture: deterministic logic belongs in tools, durable effects route through `optimus-runtime` and SmartDeny, state lives in runtime stores rather than prompts, cancellation is typed, and every execution has exactly one terminal outcome.

## Solution

Execute one dependency-ordered campaign of 100 nano-tasks:

1. Repair and finish memory clock, sensitivity, retention, tombstone, privacy erase, migration, and audit behavior.
2. Add versioned typed agent requests/results, a validated registry, and a durable invocation lifecycle that cannot authorize effects by registry membership.
3. Add a versioned workflow definition and registry, then represent jobs, campaigns, cron, and gateway through explicit capability adapters.
4. Add cross-contract integration/evaluation proving causal outcomes, denial, cancellation, replay classification, and no SmartDeny bypass.
5. Refresh ADRs, architecture, contracts, maps, and generated Engineering Memory.
6. Run every canonical gate, compute an independent exact-tree identity, deliver verified milestone commits to `main`, reconcile GitHub, and close the issue only after remote proof.

## User Stories

1. As a memory user, I want transaction and audit times from a real injected clock so temporal evidence is meaningful.
2. As a test author, I want a deterministic fake clock so chronology tests do not sleep or flake.
3. As a memory principal, I want sensitivity clearance enforced before writes and recalls.
4. As a policy consumer, I want recall purpose checked against each claim’s allowed uses.
5. As a privacy owner, I want deterministic retention evaluated at an explicit instant.
6. As a privacy owner, I want tombstones to hide claims without pretending the event never happened.
7. As a privacy owner, I want erase to remove recoverable protected content while retaining a minimal non-secret audit record.
8. As a migration operator, I want old memory databases upgraded conservatively without losing claims.
9. As an agent author, I want typed versioned request/result contracts so agents cannot invent incompatible envelopes.
10. As a security reviewer, I want explicit permission and tool envelopes validated against the canonical tool catalog.
11. As an operator, I want agent invocation events ordered and persisted.
12. As an operator, I want success, failure, and cancellation to be mutually exclusive terminal outcomes.
13. As a runtime owner, I want agent effects to retain Work Graph/effect-attempt provenance.
14. As a security owner, I want agent registration to grant no capability by itself.
15. As a workflow author, I want typed trigger, input, output, dependency, retry, timeout, cancellation, approval, and observability declarations.
16. As an operator, I want invalid or cyclic workflow definitions rejected before execution.
17. As an adapter author, I want unsupported capabilities reported explicitly rather than silently emulated.
18. As a scheduler operator, I want jobs, campaigns, cron, and gateway represented honestly through one lifecycle vocabulary.
19. As an evaluator, I want denial, cancellation, terminal uniqueness, causality, and replay behavior tested across contracts.
20. As an engineer, I want source, ADRs, generated memory, tests, and the exact remote commit to agree.

## Implementation Decisions

### Memory

- `MemoryClock` is injected into `Memory`; production uses process-monotonic UTC seconds and tests use deterministic clocks.
- Transaction time defaults to clock time, never valid time. Explicit historical `learned_at` remains supported.
- Sensitivity order is `public < personal < confidential < restricted`; migrated claims default to `personal`.
- A principal’s maximum sensitivity is part of authenticated write/recall context.
- Recall requires both sensitivity clearance and an `AllowedUse` matching the requested purpose.
- Tombstone excludes content from recall while retaining the original row for non-content audit until privacy erase.
- Privacy erase overwrites subject, predicate, and object with fixed non-secret markers and is idempotent.
- Retention uses an explicit evaluation instant and applies only within authenticated scope.
- Audit events contain stable IDs and operation metadata, never erased content.

### Agent contracts and lifecycle

- Agent IDs and versions are parsed canonical types.
- Requests include task, bounded context references, constraints, canonical tool IDs, permission envelope, budget, cancellation ID, and trace ID.
- Results discriminate `succeeded`, `failed`, `cancelled`, and `ambiguous`, and carry bounded evidence/artifact references plus unresolved items.
- Registry descriptors declare exact versions, tool requirements, permission ceilings, input/output schema versions, and responsibility.
- Registry validation rejects duplicate identities, unavailable tools, malformed versions, and permissions broader than the host ceiling.
- Invocation projection and append-only events live in SQLite with one nonterminal invocation per ID and one terminal event.
- Cancellation fences late settlement. Retry creates a new invocation linked to the prior one; it never rewrites a terminal invocation.
- Any effect reference must name an existing durable runtime effect attempt. Registry membership never bypasses SmartDeny.

### Workflow contracts and adapters

- Workflow definitions are versioned data with canonical IDs.
- Definitions include trigger, typed inputs/outputs, dependencies, retry, timeout, cancellation, approval, validation, rollback declaration, observability, and terminal outcomes.
- Validation rejects duplicate nodes, missing dependencies, cycles, zero timeout, unbounded retry, absent cancellation semantics, and incomplete terminal declarations.
- A workflow registry persists immutable `(id, version)` definitions.
- Jobs, campaigns, cron, and gateway implement an adapter descriptor exposing exact supported capabilities.
- Shared conformance policy checks lifecycle completeness while allowing explicit `unsupported` results.
- Adapters do not unify underlying stores or claim cross-database atomicity.

### Testing and delivery

- Behavioral slices use focused RED–GREEN–REFACTOR. Recovery tasks first reproduce and remove the existing compile failure.
- Persistence tests use temporary SQLite databases and independent reopen/connection checks.
- Security tests assert both rejection and absence of side effects/content leakage.
- Four milestone commits are permitted only after their declared gates pass; each is pushed once to `origin/main` and read back from GitHub.
- The final exact-tree identity is computed from the Engineering Memory repository index and independently recomputed from disk bytes.

## Testing Decisions

- **Focused:** package-level named tests for one behavior.
- **Affected:** complete owner crate tests and strict owner-crate Clippy.
- **Boundary:** kernel integration tests crossing memory/runtime/session/agent/workflow stores.
- **Canonical:** workspace format, strict Clippy, all-target/all-feature tests, strict rustdoc, desktop Playwright, Engineering Memory generation/validation/semantic/currentness, independent tree hash.
- Tests use synthetic credentials and temporary homes only; no real credential migration or paid provider call occurs.
- No test assertion may weaken SmartDeny, terminal uniqueness, sensitivity, or erase guarantees.

## Ordered 100 Nano-task Contract

### Phase A — Recover and finish evidence memory

1. **n001 Reproduce interrupted baseline.** Record the six current `optimus-memory` compile errors. **Proof:** focused check fails only at known missing helpers/decoder fields.
2. **n002 Add migration column helper.** Implement idempotent SQLite column detection/addition. **Proof:** legacy-schema migration test.
3. **n003 Decode policy columns.** Parse sensitivity, retention, tombstone, and erased fields into `ClaimView`. **Proof:** row roundtrip test.
4. **n004 Map recall purpose to allowed use.** Add total non-action mapping; action remains rejected earlier. **Proof:** mapping unit test.
5. **n005 Update authenticated contexts.** Add conservative `max_sensitivity` at every production/test `WriteContext` construction. **Proof:** workspace check advances past context errors.
6. **n006 Update claim drafts.** Add explicit sensitivity/retention at every production/test draft construction. **Proof:** workspace check advances past draft errors.
7. **n007 Restore memory compilation.** Resolve remaining API/schema compiler failures without broadening behavior. **Proof:** `cargo check -p optimus-memory` passes.
8. **n008 RED default transaction time.** Fake-clock test proves omitted `learned_at` uses clock rather than `valid_from`. **Proof:** focused RED then GREEN.
9. **n009 RED ledger time.** Fake-clock test proves audit events consume clock values. **Proof:** focused RED then GREEN.
10. **n010 Process-monotonic production clock.** Verify repeated observations never move backwards. **Proof:** clock unit test.
11. **n011 UTC conversion boundaries.** Cover epoch, leap day, year rollover, and known timestamp fixture. **Proof:** table test.
12. **n012 Explicit historical knowledge time.** Ensure supplied `learned_at` remains authoritative. **Proof:** bitemporal regression.
13. **n013 Close m45.** Run complete memory tests and strict Clippy. **Proof:** both pass.
14. **n014 RED sensitivity write ceiling.** Restricted draft under personal clearance fails with no row/event. **Proof:** focused RED/GREEN.
15. **n015 RED sensitivity recall ceiling.** High-sensitivity claim is invisible below clearance before limit. **Proof:** focused RED/GREEN.
16. **n016 RED allowed-use purpose filter.** Inform-only untrusted claim cannot appear in constraint/procedure recall. **Proof:** focused RED/GREEN.
17. **n017 Migration sensitivity default.** Legacy row reopens as personal. **Proof:** migration fixture.
18. **n018 Correction preserves sensitivity.** Correction cannot downgrade prior sensitivity or exceed caller clearance. **Proof:** correction tests.
19. **n019 Close m46.** Run complete memory tests and strict Clippy. **Proof:** both pass.
20. **n020 RED tombstone visibility.** Tombstoned claim disappears from recall but audit identity remains. **Proof:** focused RED.
21. **n021 Implement scoped tombstone.** Enforce scope, clock time, idempotency, and sanitized audit. **Proof:** focused GREEN.
22. **n022 RED privacy erase.** Erased claim payload must not remain recoverable through public APIs or raw DB text fields. **Proof:** focused RED.
23. **n023 Implement privacy erase.** Overwrite protected content with fixed markers and append sanitized audit. **Proof:** focused GREEN.
24. **n024 RED retention boundary.** Claim is retained before boundary and tombstoned at exact boundary. **Proof:** fake-clock table RED.
25. **n025 Implement scoped retention evaluation.** Deterministically tombstone eligible rows once. **Proof:** focused GREEN.
26. **n026 Add bounded audit inspection.** Expose sanitized event metadata for verification/operations. **Proof:** scope/order/limit tests.
27. **n027 Erase/tombstone idempotency.** Repetition creates no duplicate terminal privacy transition. **Proof:** event-count tests.
28. **n028 Erased-content negative scan.** Independent SQLite query finds no original subject/predicate/object. **Proof:** adversarial test.
29. **n029 Memory schema migration suite.** Reopen v1, v2 partial, and current databases without loss or policy downgrade. **Proof:** migration tests.
30. **n030 Memory milestone.** Run memory+kernel affected suites and Clippy; create and push first verified `main` commit referencing issue #1. **Proof:** local SHA equals `origin/main` SHA.

### Phase B — Typed agent substrate

31. **n031 Map canonical seams.** Freeze canonical tool, outcome, runtime effect, and cancellation dependencies in tests/spec notes. **Proof:** compile-only characterization.
32. **n032 Create agent module test seam.** Add module and failing public API test without implementation leakage. **Proof:** intended missing-API RED.
33. **n033 Canonical agent identity.** Parse bounded agent ID and semantic version. **Proof:** valid/invalid table.
34. **n034 Typed context references.** Add bounded source-backed context references with provenance requirement. **Proof:** validation tests.
35. **n035 Permission envelope.** Define exact tool/effect/network/filesystem ceilings. **Proof:** serialization and invalid-combination tests.
36. **n036 Agent budget.** Define nonzero step/time/schema/cost limits with checked validation. **Proof:** boundary tests.
37. **n037 Agent request contract.** Add versioned task, constraints, tools, permissions, budget, cancellation, and trace fields. **Proof:** valid roundtrip.
38. **n038 Agent request validation.** Reject empty task, duplicate tools, unknown schema version, missing cancellation, and unbounded budgets. **Proof:** table RED/GREEN.
39. **n039 Agent terminal result contract.** Add discriminated result kinds and bounded evidence/artifacts/unresolved items. **Proof:** roundtrip tests.
40. **n040 Agent result consistency.** Reject success with error, failure without error, cancellation without reason, and invalid hashes. **Proof:** table tests.
41. **n041 Agent descriptor contract.** Define responsibility, versions, schema versions, required tools, and requested permissions. **Proof:** valid descriptor test.
42. **n042 Agent registry schema.** Persist immutable descriptors and schema version. **Proof:** create/reopen test.
43. **n043 Register valid descriptor.** Store and retrieve exact descriptor bytes. **Proof:** roundtrip.
44. **n044 Reject duplicate identity/version.** No overwrite or last-write-wins. **Proof:** duplicate test.
45. **n045 Validate descriptor versions.** Reject malformed or unsupported request/result schema versions. **Proof:** focused tests.
46. **n046 Validate required tools.** Reject descriptors requiring unavailable canonical `ToolId`s. **Proof:** catalog-backed test.
47. **n047 Fence permissions.** Reject descriptor permissions broader than host ceiling. **Proof:** denial/no-row test.
48. **n048 Registry reopen integrity.** Persisted malformed JSON/semantic corruption fails closed. **Proof:** adversarial SQLite mutation.
49. **n049 Registry listing.** Deterministic ID/version order and exact lookup. **Proof:** ordering test.
50. **n050 Close m48/m49.** Run agent contract/registry tests and strict kernel Clippy. **Proof:** pass.
51. **n051 Invocation schema.** Add projection/events with one running state and terminal uniqueness. **Proof:** schema test.
52. **n052 Begin invocation.** Atomically persist validated request identity and accepted event. **Proof:** begin/reopen test.
53. **n053 Ordered invocation events.** Append monotonic sequence with bounded sanitized payloads. **Proof:** order test.
54. **n054 Terminal success/failure.** Settle projection and event exactly once. **Proof:** repeated-settlement fencing.
55. **n055 Typed cancellation.** Request cancellation and settle cancelled without converting to failure. **Proof:** cancellation test.
56. **n056 Fence late completion.** Completion after cancellation fails and cannot replace terminal state. **Proof:** stale-owner test.
57. **n057 Retry lineage.** Retry creates a new invocation linked to terminal predecessor. **Proof:** lineage test.
58. **n058 Reopen durability.** Terminal projection/events survive store reopen unchanged. **Proof:** reopen test.
59. **n059 Link runtime effect provenance.** Agent event may reference only an existing exact terminal effect attempt. **Proof:** valid/foreign/mismatched tests.
60. **n060 SmartDeny denial path.** Agent permission does not authorize an unapproved runtime effect. **Proof:** integration test observes no effect.
61. **n061 Cancellation token bridge.** Invocation cancellation maps to cooperative kernel token without polling ambiguity at loop boundaries. **Proof:** blocking-provider test.
62. **n062 Ambiguous terminal support.** Persist unknown external/effect settlement distinctly. **Proof:** terminal-kind test.
63. **n063 Public exports and rustdoc.** Expose only narrow validated APIs. **Proof:** strict package rustdoc.
64. **n064 Agent affected gate.** Run full kernel tests and strict Clippy. **Proof:** pass.
65. **n065 Agent milestone.** Commit and push verified agent substrate to `main`, referencing issue #1. **Proof:** GitHub commit read-back equals local SHA.

### Phase C — General workflow contract and adapters

66. **n066 Create workflow module RED.** Add public-seam missing API test. **Proof:** intended RED.
67. **n067 Canonical workflow identity/version.** Parse bounded IDs and versions. **Proof:** table tests.
68. **n068 Typed triggers.** Define manual, schedule, message, dependency, and runtime-event triggers. **Proof:** roundtrip/invalid tests.
69. **n069 Typed input/output declarations.** Require unique names and supported schema fragments. **Proof:** validation tests.
70. **n070 Dependency graph validation.** Reject missing nodes, self-dependency, duplicates, and cycles. **Proof:** graph fixtures.
71. **n071 Bounded retry policy.** Define finite attempts/backoff/retryable terminal kinds. **Proof:** boundary tests.
72. **n072 Timeout and cancellation policy.** Require nonzero timeout and explicit cancellation behavior. **Proof:** invalid-definition tests.
73. **n073 Approval policy.** Bind high-risk nodes to exact approval requirements. **Proof:** missing/broadened approval tests.
74. **n074 Terminal declaration.** Require success/failure/cancelled/ambiguous handling exactly once. **Proof:** completeness tests.
75. **n075 Rollback honesty.** Distinguish supported, compensating, and unsupported rollback. **Proof:** validation tests.
76. **n076 Observability contract.** Require ordered event classes and trace identity. **Proof:** schema test.
77. **n077 Workflow definition validation.** Compose all validators at one public seam. **Proof:** comprehensive table.
78. **n078 Workflow registry schema.** Persist immutable `(id,version)` definitions. **Proof:** create/reopen test.
79. **n079 Registry corruption fencing.** Malformed persisted definition fails closed. **Proof:** adversarial mutation.
80. **n080 Adapter capability contract.** Define lifecycle capability matrix and explicit unsupported result. **Proof:** roundtrip.
81. **n081 Job adapter.** Map Work Graph jobs honestly. **Proof:** capability/status fixture.
82. **n082 Campaign adapter.** Map campaign lifecycle without inventing rollback/replay. **Proof:** fixture.
83. **n083 Cron adapter.** Map leased schedule attempts, cancellation, and retry. **Proof:** fixture.
84. **n084 Gateway adapter.** Map claimed delivery, acknowledgement, retry, cancellation, and dead letter. **Proof:** fixture.
85. **n085 Adapter conformance suite.** Assert shared terminal/cancellation/observability semantics and explicit unsupported differences. **Proof:** table suite.

### Phase D — Integration, authority, gates, and delivery

86. **n086 Tool→manifest integration.** Canonical tool outcome links to execution manifest and exact effect attempt. **Proof:** integration trajectory.
87. **n087 Turn→agent integration.** Session terminal status and agent invocation terminal status agree without cross-DB atomicity claims. **Proof:** success/failure/cancel trajectories.
88. **n088 Agent→workflow integration.** Workflow node records exact agent invocation/result identity. **Proof:** offline trajectory.
89. **n089 Denial/cancellation evaluation.** Add offline cases for sensitivity denial, SmartDeny, route denial, cancellation, stale completion, and dead letter. **Proof:** evaluation suite.
90. **n090 Integration milestone.** Run memory/kernel/runtime/store suites and strict Clippy; commit and push to `main`. **Proof:** remote SHA read-back.
91. **n091 Write ADR-0022.** Record memory policy, agent substrate, workflow adapters, and acknowledged atomicity boundaries. **Proof:** ADR parser/section checks.
92. **n092 Update architecture/contracts.** Mark exact confirmed behavior and remaining limits. **Proof:** no contradictory current claims.
93. **n093 Update maps/spec disposition.** Map ownership and mark all 100 nano-tasks with final evidence. **Proof:** task cardinality/status validator.
94. **n094 Update Engineering Memory generator/tests.** Encode only source/test-proven semantic claims. **Proof:** semantic test RED/GREEN.
95. **n095 Generate and validate Engineering Memory.** Run generate, validate JSON, semantic tests. **Proof:** zero errors.
96. **n096 Currentness closure.** Run currentness check and resolve every stale document from source, never generated JSON edits. **Proof:** empty changed/stale sets.
97. **n097 Full Rust gates.** Format check, workspace strict Clippy, all-target/all-feature tests, strict rustdoc. **Proof:** all exit zero after final Rust edit.
98. **n098 Desktop/browser gate.** Run full desktop Playwright and affected CLI/desktop Rust tests. **Proof:** all pass.
99. **n099 Independent exact-tree verification.** Recompute every indexed file hash and aggregate identity independently. **Proof:** zero mismatches; exact file count/hash captured.
100. **n100 Final delivery and GitHub reconciliation.** Create final verified commit, push once to `origin/main`, verify remote tree/SHA, update and close issue #1, and report exact evidence. **Proof:** local HEAD = origin/main = GitHub API SHA; issue closed; no PR/extra branch.

## Out of Scope

- Subagents, remote agents, distributed orchestration, or parallel writers.
- Pull requests, feature branches, merge commits, rebases, or history rewriting.
- Production deployment, installer publication, release tags, or paid model calls.
- Real user credential migration during tests.
- Claiming atomicity across independent SQLite databases.
- Claiming deterministic replay for model/external stages without fixtures.
- Replacing SQLite, adding a message broker, or implementing distributed locks.
- Building arbitrary prompt-defined agents; only typed registered contracts are in scope.

## Further Notes

- Source and executable tests outrank this specification.
- A nano-task is complete only after its exact proof is observed on the post-edit tree.
- Milestone commits are delivery checkpoints, not substitutes for final canonical gates.
- If a milestone gate fails, remain within the owning nano-task and do not commit.
- Generated `.engineering-memory/*.json` files are changed only by the generator.
- Every remote mutation must be read back from GitHub before being reported.
