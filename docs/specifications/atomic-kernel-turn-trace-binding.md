---
knowledge_type: specification
status: historical
covers:
  - crates/optimus-kernel/src/execution.rs
  - crates/optimus-kernel/src/lib.rs
  - crates/optimus-kernel/tests/kernel_turn.rs
  - crates/optimus-kernel/tests/session_resume.rs
  - docs/architecture/system-overview.md
  - docs/maps/observability-and-evaluations.md
  - scripts/engineering_memory.py
  - scripts/test_engineering_memory.py
---

# Atomic kernel-turn trace binding

**Date:** 2026-07-20

## Problem and outcome

**Observed fact:** `ExecutionStore` has an `execution_trace_links` table and exact
`bind_trace`/`trace_context` APIs. Production `Kernel` turn creation writes an
execution manifest but never binds a trace. Only a storage-level trace contract
test calls `bind_trace`.

**Observed fact:** the Priority-2 dataset declares trace requirements for its
offline trajectories, while the current trajectory result exposes no trace
evidence.

**Intended outcome:** every newly recorded production kernel turn creates its
execution manifest and one root `TraceContext` in a single execution-database
transaction. Successful results expose that exact context. Interrupted-turn
resume reads and reuses the existing context rather than generating another one.

## Scope

- Add an atomic traced-manifest creation API to `ExecutionStore`.
- Use it for new kernel turns and for the missing-manifest recovery path.
- Return the bound trace context in successful `TurnResult` values.
- Require an existing trace link when resuming an existing manifest.
- Cover success, cancellation/failure persistence, and interruption/resume
  identity.
- Update current trace/evaluation authority and generated Engineering Memory.

## Non-scope

- `TraceStore` span/event creation or settlement.
- Child spans for route, model, tool, agent, workflow, or gateway work.
- Distributed tracing or OpenTelemetry export.
- Complete ten-case observation/report production.
- Evaluation baselines, thresholds, CLI/CI gates, routing, releases, or live
  provider evaluation.
- Schema migration: `execution_trace_links` already exists.

## Authoritative existing behaviour

- `ExecutionStore::begin` creates an untraced manifest and remains supported for
  storage-level and compatibility callers.
- `ExecutionStore::bind_trace` rejects a second link by primary-key/unique
  constraints.
- `ExecutionStore::trace_context` parses and returns the exact persisted context.
- A session turn and execution manifest retain one stable identity across resume.
- Session and execution databases are independent authorities; this milestone
  does not claim a cross-database transaction.
- ADR-0023 and the Priority-2 specification govern trace identity and evaluation
  evidence limits.

## Behavioural contracts and invariants

1. `begin_traced` validates the same manifest inputs as `begin`.
2. It inserts one manifest and one root trace link in one SQLite transaction.
3. The root context has no parent span.
4. Failure of either insertion rolls back both rows.
5. Production kernel manifest creation uses `begin_traced`; it may not call
   untraced `begin` followed by a separately committed link.
6. A successful `TurnResult.trace_context` equals the context persisted for that
   turn's manifest.
7. Cancellation or failure terminalizes the existing manifest without deleting
   or replacing its trace link.
8. Resume with an existing manifest loads its exact context. Missing or malformed
   trace evidence fails closed before model or tool execution.
9. Resume must not create a second context or alter trace/span identity.
10. A recovery path that creates a previously missing manifest creates exactly
    one traced manifest through the same atomic API.
11. Trace identity grants no permission and changes no routing, policy, approval,
    tool, or replay decision.
12. This link is execution-manifest causal evidence only. It is not evidence that
    a corresponding `TraceStore` span or event stream exists.

## State, interface, compatibility, and mutation boundaries

- State mutations are limited to the existing execution manifest and trace-link
  tables plus ordinary pre-existing session/runtime mutations.
- `ExecutionStore::begin` remains source- and behavior-compatible.
- `ExecutionStore::begin_traced` returns `(manifest_id, TraceContext)`.
- `TurnResult` adds a public `trace_context: TraceContext` field.
- No database version or migration changes.
- Existing rows without links remain readable through storage APIs, but kernel
  resume rejects an existing untraced manifest before execution.

## Failure, interruption, recovery, and races

- Atomic traced creation prevents a manifest-without-link state from a partial
  execution-database write.
- SQLite uniqueness prevents duplicate links under competing writers.
- A crash after traced creation but before terminal settlement leaves the same
  running manifest and trace link; resume reuses both.
- A crash before traced creation leaves no execution row from that transaction;
  the existing recovery path may create one traced manifest.
- Existing cross-database session/execution partial-state limits remain unchanged.
- No rollback deletes historical terminal rows or links.
- Corrupt, missing, duplicate, or unparseable trace evidence fails closed.

## Acceptance criteria

- A successful kernel turn persists one root trace context and returns that exact
  context.
- A cancelled or failed turn retains one readable trace link on its terminal
  execution manifest.
- An interrupted turn resumed through a reopened kernel retains the same trace ID
  and span ID and produces no second execution manifest/link.
- Atomic traced insertion rolls back the manifest when link insertion is forced to
  fail.
- Existing untraced-manifest storage contracts remain valid.
- Focused tests, canonical gates, Engineering Memory, exact diff hygiene, and
  detached staged-tree verification pass.

## Execution plan and ledger

### Slice 1 — atomic new-turn binding

- **Outcome:** new kernel turns have one atomic root execution trace and successful
  results expose it.
- **Dependency:** existing execution manifest/link schema.
- **RED:** a focused successful-turn contract requires a persisted trace equal to
  `TurnResult.trace_context`; current `TurnResult` has no field and production
  manifests have no link.
- **GREEN:** add transactional `begin_traced`, generate a root context, and route
  production manifest creation through it.
- **Refactor:** share manifest insertion logic without changing `begin` semantics.
- **Verification:** selected kernel-turn and trace contracts plus strict focused
  Clippy.
- **Complete when:** success and terminal failure/cancellation retain exact links,
  and forced atomic failure leaves no manifest.
- **Observed evidence:** the selected contract failed to compile because
  `TurnResult.trace_context` did not exist. After transactional `begin_traced`
  and kernel threading, the exact success contract passed. Forced trace-link
  failure rolled back the manifest, and cancellation retained its link.

### Slice 2 — resume identity

- **Outcome:** interrupted turns reuse exact persisted trace identity.
- **Dependency:** Slice 1.
- **RED:** an active session paired with an already-terminal traced manifest
  invoked and recorded the model, then terminalized the session before late
  execution settlement failed.
- **GREEN:** load the trace for an existing manifest, pass it through recorded-turn
  execution, and fail closed when absent.
- **Refactor:** use one recorded-execution value for manifest and trace identity.
- **Verification:** focused interruption/resume, missing-link denial, and relevant
  session tests.
- **Complete when:** resume neither replaces nor duplicates trace identity and
  missing evidence blocks execution.
- **Observed evidence:** manifest session/turn identity and running status now
  preflight before trace loading or model execution. Terminal and untraced
  manifests leave the active session and manifest evidence unchanged with zero
  model calls. A nominal reopened resume returned and persisted the exact
  pre-interruption trace and reused the same manifest.

## Final verification

- Focused kernel turn, session resume, trace, and evaluation-adjacent contracts.
- `python -m unittest scripts/test_engineering_memory.py -v`.
- Engineering Memory generate, strict validate, and currentness check.
- `cargo fmt --all --check`.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- `cargo test --workspace --all-features`.
- `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps`.
- Exact changed-path/diff inspection and detached staged-tree focused verification.

## Prohibited actions

- Do not fabricate trace evidence or claim `TraceStore` lifecycle coverage.
- Do not silently repair or delete historical untraced rows.
- Do not add network/provider/process execution.
- Do not weaken cancellation, replay, approval, SmartDeny, or terminal-outcome
  contracts.
- Do not manually edit generated Engineering Memory JSON.
- Do not create a branch or pull request, install dependencies, deploy, release,
  publish, access credentials, or modify unrelated paths.

## Assumptions

- **Reasonable inference:** one root execution context per turn is the narrowest
  useful causal identity and can later back trajectory evaluation evidence.
- **Unresolved:** child-span propagation and `TraceStore` lifecycle remain future
  milestones; this specification makes no claim about them.
