---
doc_id: spec-005-agents-workflows
doc_type: reference
plane: work
status: current
authority: canonical
summary: Versioned specialist descriptors, the immutable agent registry, durable workflow DAGs, and the content-addressed artifact store owned by optimus-agent, optimus-workflow, and optimus-artifacts.
reviewed_on: 2026-08-05
review_by: 2026-10-31
knowledge_type: specification
covers:
  - crates/optimus-agent/src/**
  - crates/optimus-workflow/src/**
  - crates/optimus-artifacts/src/**
validated_by:
  - crates/optimus-agent/tests/**
  - crates/optimus-workflow/tests/**
  - crates/optimus-artifacts/tests/**
---

# 005 — Agents, workflows, artifacts

Status: active
Owner: development agents (main-only)

## Purpose

The specialist-agent layer: narrow, documented responsibilities communicated
through typed inputs and outputs; versioned immutable descriptors; durable
workflow DAGs with terminal outcomes; and the content-addressed artifact store
that carries handoffs between them.

## Requirements

- R1. Each specialist agent MUST have a narrow, documented responsibility and
  typed I/O (architectural law 2–3).
- R2. Descriptors MUST be versioned and the registry immutable; invocation,
  cancellation, retry, and terminal outcomes MUST be durably ledged with
  effect provenance links.
- R3. Workflows MUST be durable DAGs (`WorkflowRunStore`) with exactly one
  terminal outcome per run and defined success/failure/cancellation/retry
  behaviour.
- R4. Artifacts MUST be content-addressed under `{home}/artifacts` with
  source-backed provenance.
- R5. An unreachable vertical MUST be archived, not carried (ADR-0073).

## Acceptance criteria
- [ ] A1. Given the agent/workflow/artifact crate suites, when they run, then all tests pass.
- [ ] A2. Given a descriptor mutation attempt, when the registry is consulted, then immutability holds (no silent mutation).

## Out of scope

- Kernel turn loop (spec 003) and runtime effects (spec 004).

## Open questions

- None.

## Links

Code: crates/optimus-agent, optimus-workflow, optimus-artifacts · ADRs: 0073 ·
Ontology: optimus-agent, optimus-workflow, optimus-artifacts
