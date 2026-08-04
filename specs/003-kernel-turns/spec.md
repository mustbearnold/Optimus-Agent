---
doc_id: spec-003-kernel-turns
doc_type: reference
plane: work
status: current
authority: canonical
summary: Provider-agnostic turn loop, session lifecycle, canonical model routing, transcript contracts, and the browser/search effectors owned by optimus-kernel and optimus-browser.
reviewed_on: 2026-08-05
review_by: 2026-10-31
knowledge_type: specification
covers:
  - crates/optimus-kernel/src/**
  - crates/optimus-browser/src/**
depends_on:
  - docs/decisions/0035-command-capability-envelope.md
  - docs/decisions/0040-shared-browser-contract.md
  - docs/decisions/0078-a-transcript-is-a-provider-contract-and-an-unreachable-toggle-asks.md

validated_by:
  - crates/optimus-kernel/tests/**
  - evals/**
  - crates/optimus-browser/tests/**
---

# 003 — Kernel turns

Status: active
Owner: development agents (main-only)

## Purpose

The provider-agnostic turn loop: sessions, canonical routing, tool dispatch,
transcript contracts, and the browser/search effectors. Everything the
surfaces (TUI, CLI, desktop) call stays behind this kernel.

## Requirements

- R1. The turn loop MUST be provider-agnostic; provider adapters select
  models via canonical catalog ids, with `auto` resolved by the Rust router at
  turn start.
- R2. Chat tool events MUST carry stable `event_id`/`run_id`/`call_id`,
  canonical `tool_id`, an explicit lifecycle phase
  (started/approval_required/succeeded/failed/cancelled/suppressed/ambiguous),
  and be persisted before delivery (replayable via get_session).
- R3. A transcript is a provider contract; unreachable toggles and
  non-terminal states are defects (ADR-0078).
- R4. Browser effectors MUST be SSRF-safe over HTTP and CDP-capable when
  available; the workbench Browser surface drives the effector directly
  (Electron-era WebContentsView preview is retired).
- R5. Key-based provider credentials MUST be stored/queried without ever
  returning the key (masked tail only, presence + origin).

## Acceptance criteria
- [ ] A1. Given the kernel integration suite, when `cargo test -p optimus-kernel` runs, then turn, tool-lifecycle, routing, and credential tests pass.
- [ ] A2. Given the offline evaluation suite, when fixtures replay, then zero side effects occur and baselines hold.
- [ ] A3. Given the matrix gate, when `browser_*` methods are classified, then they are renderer-callable and covered by spec 002.

## Out of scope

- Durable effect execution and approvals (spec 004).
- Memory/skills/packs (spec 006).

## Open questions

- None.

## Links

Code: crates/optimus-kernel, crates/optimus-browser · Tests: kernel tests +
evals · ADRs: 0035, 0040, 0078 · Ontology: optimus-kernel, optimus-browser
