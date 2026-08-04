---
doc_id: spec-004-runtime-effects
doc_type: reference
plane: work
status: current
authority: canonical
summary: Durable ordered jobs, exact-action SmartDeny approvals, cancellation, crash recovery, leased campaigns, and the high-risk contract catalog owned by optimus-runtime with graph/store.
reviewed_on: 2026-08-05
review_by: 2026-10-31
knowledge_type: specification
covers:
  - crates/optimus-runtime/src/**
  - crates/optimus-graph/src/**
  - crates/optimus-store/src/**
depends_on:
  - docs/decisions/0018-fail-closed-runtime-safety.md
  - docs/decisions/0031-safe-project-work-loop.md
  - docs/decisions/0046-approval-resumes-the-turn.md

validated_by:
  - crates/optimus-runtime/tests/**
  - crates/optimus-graph/tests/**
  - crates/optimus-kernel/tests/dev_run_containment.rs
---

# 004 — Runtime effects

Status: active
Owner: development agents (main-only)

## Purpose

Every durable side effect: ordered jobs, effect intents/receipts, bounded
command execution, exact-action SmartDeny approvals, cancellation, crash
recovery, output capture, and leased ordered campaigns. High-risk effects wait
for explicit approval; surfaces never execute model effects directly.

## Requirements

- R1. Effects MUST be durable as intents before execution and MUST produce
  exactly one terminal outcome (success/failure/cancelled) with a manifest.
- R2. Approvals MUST be exact-action: `chat_approval_resolve` repeats the
  pending event's run/call/job/node ids and `effect_sha256`; approve executes
  that exact persisted effect, deny executes nothing.
- R3. Cancellation MUST be supported for every long-running operation and
  MUST be durable (replay reconstructs the same terminal state).
- R4. Leases MUST fence concurrent runners (owner/token/generation/deadline:
  expire, renew, release, reject stale owners).
- R5. Campaigns MUST run as ordered, leased jobs with crash recovery.
- R6. `term_run` MUST be bounded (exact command, durable job/status/output).
- R7. High-risk contracts (C-08 onwards) MUST remain documented and each MUST
  name its evidence; retired contracts (C-17 Electron preview) MUST be marked
  retired, not deleted silently.

## Acceptance criteria
- [ ] A1. Given the runtime and graph integration suites, when they run, then containment, lease-fencing, and crash-recovery tests pass.
- [ ] A2. Given an approval event, when `chat_approval_resolve` repeats its exact binding, then approve executes once and deny executes zero times; changed identities and duplicate decisions fail closed.
- [ ] A3. Given the high-risk contract catalog, when it is audited against the code, then every contract names real evidence and retired contracts are marked retired.

## Out of scope

- Turn-loop semantics (spec 003).

## Open questions

- None.

## Links

Code: crates/optimus-runtime, optimus-graph, optimus-store · Tests: runtime
tests · ADRs: 0018, 0020, 0031, 0046 · Ontology: optimus-runtime
