---
doc_id: lessons-ai-agent-mistakes
doc_type: explanation
plane: current
status: current
authority: supporting
summary: Only repeatable lessons belong here. Task-by-task progress belongs in execution evidence, not this ledger.
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: lesson-ledger
owns:
  - AGENTS.md
  - docs/engineering-memory/README.md
  - docs/contracts/high-risk-contracts.md
  - scripts/engineering_memory.py
  - scripts/test_engineering_memory.py
watches:
  - docs/architecture/**
  - docs/contracts/**
covers:
  - AGENTS.md
  - docs/engineering-memory/README.md
  - docs/contracts/high-risk-contracts.md
  - scripts/engineering_memory.py
  - scripts/test_engineering_memory.py
depends_on:
  - docs/decisions/0017-engineering-memory-separation.md
  - docs/decisions/0032-engineering-memory-compact-lenses.md
validated_by:
  - scripts/test_engineering_memory.py
---

# Recurring AI coding-agent mistakes

Only repeatable lessons belong here. Task-by-task progress belongs in execution
evidence, not this ledger.

## Translating development instructions into product behaviour

**Failure:** The user tells a coding agent to work autonomously, select stronger
models, orchestrate subagents, avoid approval chatter, or follow a particular
VCS process. The coding agent then changes Optimus product prompts, routing,
permissions, or approval UX as though the development instruction were a
runtime feature request.

**Correction:** First identify the instruction plane. Requests about how the
current coding agent should build Optimus remain development policy in
`AGENTS.md`. Change the product only when the user explicitly names Optimus
runtime/product behaviour. When ambiguous, preserve product behaviour and
proceed autonomously with repository development.

**Gate:** `scripts/check-instruction-planes.py` verifies the root audience
markers and the product prompt source; kernel tests verify that the development
`AGENTS.md` body is not injected into product sessions.

## Status inflation

- **Problem:** planned or partially implemented architecture is described as
  current.
- **Symptoms:** “multi-agent,” “replay,” “router,” or “GPU” claims cite plans or
  type names but no complete implementation/tests.
- **Root cause:** prose is treated as evidence and absence of a failure is treated
  as success.
- **Successful approach:** require explicit current/inferred/planned/unresolved
  labels and source/test ownership.
- **Enforcement:** Engineering Memory label lint plus generated package/registry
  maps.
- **Future warning:** a `Cancelled` enum variant is not a cancellation system;
  persisted effects are not deterministic replay.
- **Date:** 2026-07-20.

## Duplicate ownership

- **Problem:** agents/tools/registries are added beside an existing canonical
  owner.
- **Symptoms:** provider tool schemas diverge from runtime dispatch or UI method
  lists diverge from handlers.
- **Root cause:** coding agents create a convenient local map instead of tracing
  the canonical type.
- **Successful approach:** make `ToolDesc` canonical, freeze IPC method ownership,
  and generate secondary views.
- **Enforcement:** duplicate-ID and source-reconciliation validation.
- **Date:** 2026-07-20.

## Treating compilation as completion

- **Problem:** a build is reported as proof of behavior.
- **Symptoms:** approval, provider parsing, session restoration, installed-native
  behavior, or exact artifact identity remain untested.
- **Successful approach:** focused regression, relevant integration/eval,
  installed/native evidence when applicable, then exact source/evidence identity.
- **Enforcement:** definition-of-done laws and coverage maps.
- **Date:** 2026-07-20.

## Late validation after effects

- **Problem:** one invalid model call is discovered after a sibling effect runs.
- **Root cause:** validation and dispatch are interleaved.
- **Successful approach:** validate the complete provider call batch—identity,
  advertised set, availability, and schema—before any sibling effect.
- **Enforcement:** canonical contract tests for no-effect failure.
- **Date:** 2026-07-20.

## Weakening evidence to finish

- **Problem:** stale reviews, imprecise test labels, or incomplete build logs are
  reused as acceptance.
- **Successful approach:** bind the exact candidate tree and evidence, rehash at
  review start/end, and require fresh review after any bound change.
- **Enforcement:** deterministic hashes and evidence-description validation.
- **Date:** 2026-07-20.

## Documentation without impact coverage

- **Problem:** architecture prose silently becomes stale after source changes.
- **Successful approach:** frontmatter coverage patterns plus pre-generation
  staleness check and reverse source-to-knowledge impact map.
- **Enforcement:** `engineering_memory.py check` before `generate`.
- **Date:** 2026-07-20.

## Calling persistence durable without checking atomicity

- **Problem:** separate durable writes are described as one durable transition.
- **Symptoms:** projection/event disagreement, orphan campaign jobs, duplicate
  cron runs, or an outbox reply whose input remains queued after a crash.
- **Root cause:** “stored in SQLite/files” is treated as equivalent to atomic,
  leased, idempotent, and recoverable.
- **Successful approach:** enumerate every crash window and require transaction,
  handoff, claim/lease, idempotency, and reconciliation contracts explicitly.
- **Enforcement:** C-15 job creation now commits job/nodes/events atomically;
  C-16 uses one database, deterministic step/job IDs, job-derived campaign
  status, targeted recovery, and crash-window regressions. Later Work Graph
  transitions and other C-17/C-18 workflows remain explicit debt.
- **Date:** 2026-07-20.

## Testing total loss but not partial loss

- **Problem:** a one-item corruption test proves that an empty result is rejected
  but misses subset loss in a multi-step plan.
- **Symptoms:** one reassigned/missing step is filtered out, remaining steps run,
  and the workflow reports success without a prerequisite.
- **Successful approach:** test multi-item plans with first/middle/last loss and
  persist independent completeness metadata (expected count plus contiguous
  indices) before effects.
- **Enforcement:** campaign partial-reassignment, count-mismatch, index-gap, and
  legacy-migration regressions.
- **Date:** 2026-07-20.

## Mistake-to-enforcement ladder

```text
Repeated observation
  -> concise Engineering Memory rule
  -> focused contract test or deterministic lint
  -> evaluation/regression gate
  -> less dependence on prompt obedience
```
