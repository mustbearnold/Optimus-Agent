---
doc_id: decisions-0034-control-plane-crate-peels
doc_type: decision
plane: decision
status: current
authority: record
summary: Decision record for ADR-0034: Control-plane crate peels (P11), including its context, consequences, and current documentary status.
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: decision
covers:
  - crates/optimus-agent/**
  - crates/optimus-workflow/**
  - crates/optimus-artifacts/**
  - crates/optimus-kernel/src/lib.rs
  - crates/optimus-runtime/src/lib.rs
  - scripts/gates/check-crate-layers.py
  - Cargo.toml
depends_on:
  - docs/decisions/0001-kernel-and-work-graph.md
  - docs/decisions/0022-versioned-agent-and-workflow-contracts.md
  - docs/decisions/0033-multi-agent-dag-execution.md
validated_by:
  - scripts/gates/check-crate-layers.py
  - crates/optimus-kernel/tests/agent_contracts.rs
  - crates/optimus-kernel/tests/workflow_dag.rs
  - crates/optimus-kernel/tests/specialist_vertical.rs
---

# ADR-0034: Control-plane crate peels (P11)

- **Status:** Accepted
- **Date:** 2026-07-25

## Context

After P10, multi-agent contracts and the DAG runner lived inside
`optimus-kernel`, keeping the control-plane waist too wide (mark **B+**). P11
requires dedicated crates with public APIs and a fail-closed dependency lint so
eval/ops/agent/workflow/artifacts cannot re-form a god module.

## Decision

1. **`optimus-artifacts`** owns the content-addressed artifact store
   (`ArtifactStore`, handoff blobs). No dependency on kernel, agent, workflow,
   runtime, or graph.
2. **`optimus-agent`** owns versioned agent identities, descriptors, request/
   result contracts, immutable registry, and invocation ledger. Depends on
   packs, runtime (including shared `CancellationToken`), and graph ids only.
   Does **not** depend on workflow or kernel.
3. **`optimus-workflow`** owns workflow definitions/registry, durable
   `WorkflowRunStore`, built-in specialist verticals, and the registered DAG
   executor. Depends on agent + artifacts + runtime + packs + graph.
4. **`optimus-kernel`** remains the turn/provider/tool-dispatch/session waist
   and **re-exports** agent, workflow, artifacts, and ops for surface
   convenience. Surfaces may keep importing from `optimus_kernel` without
   behaviour change.
5. **`CancellationToken`** moves to **`optimus-runtime`** so agent and kernel
   share one cooperative token type without a kernel↔agent cycle.
6. **`optimus-browser`** remains the CDP implementation crate; kernel
   `browser.rs` stays the product effector facade (HTTP + CDP factory). Full
   HTTP move into `optimus-browser` is deferred without blocking control-plane
   S+++ for agent/workflow/artifacts peels.
7. **Layer lint:** `scripts/gates/check-crate-layers.py` enforces the forbidden and
   required edges above.

## Dependency graph

```text
optimus-artifacts
optimus-packs / optimus-graph / optimus-runtime
        \\
     optimus-agent
        \\
     optimus-workflow  (+ artifacts)
        \\
     optimus-kernel  (turn loop; re-exports)
        \\
     optimus-eval
```

## Alternatives considered

- **Single `optimus-control` mega-crate.** Rejected: recreates the waist.
- **Keep modules in kernel with only docs.** Rejected: does not change
  dependency reality.
- **Agent crate owns verticals.** Rejected: verticals need workflow run store →
  cycle with workflow→agent for `AgentId`. Verticals live in workflow crate.

## Consequences

- Positive: control-plane modularity reaches S+++ for peeled domains; P12+ can
  grow specialists without enlarging turn-loop sources.
- Positive: surfaces keep stable `optimus_kernel::` imports.
- Negative: more crates to version; re-export surface must stay curated.
- Neutral: HTTP browser effector still lives under kernel until a later peel.

## Reasons

P10 multi-agent code must not calcify inside the turn loop. Peels match the
existing ops/eval extraction pattern.

## Risks

- Re-export sprawl if every new type is re-exported from kernel by habit.
- Tests still live under `optimus-kernel/tests` while implementation is peeled
  (acceptable; may move later).

## Evaluation evidence

- `python3 scripts/gates/check-crate-layers.py` exits 0
- Kernel multi-agent/contract tests green against re-exports
- `cargo check -p optimus-agent -p optimus-workflow -p optimus-artifacts`

## Conditions for reconsideration

- Moving HTTP browser fully into `optimus-browser`
- Moving kernel tests into the owner crates
- Introducing a capability protocol that removes closed specialist dispatch from
  `optimus-workflow`

## Relevant code

- `crates/optimus-agent/`
- `crates/optimus-workflow/`
- `crates/optimus-artifacts/`
- `crates/optimus-kernel/src/lib.rs`
- `crates/optimus-runtime/src/lib.rs` (`CancellationToken`)
- `scripts/gates/check-crate-layers.py`

## Relevant tests

- `crates/optimus-kernel/tests/agent_contracts.rs`
- `crates/optimus-kernel/tests/workflow_contracts.rs`
- `crates/optimus-kernel/tests/workflow_dag.rs`
- `crates/optimus-kernel/tests/specialist_vertical.rs`
