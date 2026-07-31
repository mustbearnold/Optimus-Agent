---
doc_id: decisions-0033-multi-agent-dag-execution
doc_type: decision
plane: decision
status: current
authority: record
summary: Decision record for ADR-0033: Multi-agent DAG execution (P10), including its context, consequences, and current documentary status.
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: decision
covers:
  - crates/optimus-workflow/src/workflow_run.rs
  - crates/optimus-workflow/src/specialist_vertical.rs
  - crates/optimus-agent/src/lib.rs
  - crates/optimus-workflow/src/workflow.rs
  - crates/optimus-kernel/tests/specialist_vertical.rs
  - crates/optimus-kernel/tests/workflow_dag.rs
depends_on:
  - docs/decisions/0001-kernel-and-work-graph.md
  - docs/decisions/0022-versioned-agent-and-workflow-contracts.md
  - docs/plans/s-plus-plus-plus-program.md
validated_by:
  - crates/optimus-kernel/tests/specialist_vertical.rs
  - crates/optimus-kernel/tests/workflow_dag.rs
---

# ADR-0033: Multi-agent DAG execution (P10)

- **Status:** Accepted
- **Date:** 2026-07-25

## Context

Phase 3 delivered one specialist (`workspace_writer`) and one hard-coded
executor (`run_write_file_handoff`). Architecture mark **Multi-agent readiness**
remained **B** because there was no second specialist, no durable workflow-run
ledger, no DAG scheduler over registered definitions, and no parent/child cancel
tree. P10 of the S+++ program requires a bounded multi-agent platform spine
without claiming open-ended model-chosen agent spawning or MCP specialists.

## Decision

1. **Registered definitions only.** Executable workflows are immutable rows in
   `WorkflowRegistry`. The scheduler never invents nodes or agents from model
   free text.
2. **Durable `WorkflowRunStore`.** Each run has a stable id, workflow
   identity/version, inputs JSON, ordered node projections, fenced owner lease
   (owner/token/generation/deadline), cancellation reason, and exactly one
   terminal outcome enforced in storage.
3. **Topological DAG schedule.** Node readiness requires every dependency to be
   `succeeded`. Cycles are rejected at definition validate time (existing). The
   executor may run ready nodes sequentially; parallelism is optional later and
   not required for correctness.
4. **Specialists remain Work Graph / SmartDeny bound for host mutation.**
   Write specialists create durable jobs. Read specialists may settle without a
   mutating effect but still use durable agent invocations and content-addressed
   handoff artifacts. No specialist bypasses SmartDeny for high-risk effects.
5. **Built-in specialists (P10):**
   - `workspace_writer@1.0.0` — `write_file` only
   - `workspace_reader@1.0.0` — `read_file` only (no shell, no network, no write)
6. **Built-in workflows (P10):**
   - `write_file_handoff@1.0.0` — single write node (existing vertical)
   - `read_file_handoff@1.0.0` — single read + handoff artifact
   - `write_then_read_handoff@1.0.0` — write → read DAG proving dependency order
7. **Parent/child cancel tree.** The workflow run is the parent. Child agent
   invocations (and their Work Graph jobs when present) are linked on the run.
   Cancelling the run requests cancellation on every non-terminal child and
   cancels linked jobs. Late child success settlement remains fenced by
   `AgentInvocationStore` cancel requests. A terminal parent rejects new child
   begins.
8. **Handoff artifacts.** Successful specialists may publish content-addressed
   artifacts linked from the agent result; parents consume hashes as evidence,
   not as permission grants.
9. **Honest interim residual.** Command/shell specialists and full OS
   filesystem confinement for approved commands remain **P12**. P10 does not
   introduce `command_runner`.
10. **Not claimed:** general model-driven specialist routing, parallel leased
    child agents outside the registered DAG, MCP tools as agents, or a
    universal retry scheduler beyond per-node declared `max_attempts` (P10
    executes attempt 1 only unless a later phase implements retry).

## Alternatives considered

- **Only hard-code a second vertical without a run ledger.** Rejected: does not
  move multi-agent readiness past another one-off path.
- **Free-form model DAG.** Rejected: bypasses immutable registries and
  permission ceilings.
- **Campaign steps as agents.** Rejected: campaigns are deterministic effects,
  not typed agent invocations (ADR-0022).
- **Ship command_runner in P10.** Rejected: security residual is owned by P12.

## Consequences

- Positive: multi-agent mark can climb toward S/S+++ with tests for DAG order,
  dual specialists, cancel tree, and handoffs.
- Positive: P11 can peel agent/workflow/run crates without redesigning the
  execution model.
- Negative: executor dispatch for built-in agents is still a closed match table
  until a safer capability protocol exists.
- Neutral: `run_write_file_handoff` remains as a convenience wrapper over the
  registered write workflow for CLI compatibility.

## Reasons

Registered DAG execution reuses Work Graph and SmartDeny instead of inventing a
second multi-agent runtime. Two specialists with complementary tools prove
permission ceilings and handoff without requiring a command specialist before
P12.

## Risks

- Closed dispatch tables can become a second god-module if every new specialist
  is hardcoded in the kernel rather than a capability protocol.
- Sequential ready-node scheduling may hide concurrency bugs when parallelism
  is added later.
- Multi-DB identity (run store vs invocation store vs optimus.db) is reconciled,
  not transactional.

## Evaluation evidence

- `crates/optimus-kernel/tests/workflow_dag.rs` — DAG order, dual artifacts,
  cancel-after-begin, terminal parent child deny, terminal uniqueness.
- `crates/optimus-kernel/tests/specialist_vertical.rs` — seed both agents/three
  workflows; write SmartDeny/grant/success paths.
- Engineering Memory agent count ≥ 2 after generate.

## Conditions for reconsideration

- Introducing model-chosen specialist routing or MCP agents as specialists.
- Adding parallel multi-ready-node execution or command_runner specialists.
- Peeling agent/workflow/run into separate crates (expected in P11; update
  ownership docs, not necessarily this decision).

## Relevant code

- `crates/optimus-kernel/src/workflow_run.rs`
- `crates/optimus-kernel/src/specialist_vertical.rs`
- `crates/optimus-kernel/src/agent.rs`
- `crates/optimus-kernel/src/workflow.rs`
- `apps/optimus-cli/src/main.rs` (`vertical` subcommands)

## Relevant tests

- `crates/optimus-kernel/tests/workflow_dag.rs`
- `crates/optimus-kernel/tests/specialist_vertical.rs`
- `crates/optimus-kernel/tests/agent_contracts.rs`
- `crates/optimus-kernel/tests/workflow_contracts.rs`
