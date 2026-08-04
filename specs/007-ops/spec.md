---
doc_id: spec-007-ops
doc_type: reference
plane: work
status: current
authority: canonical
summary: Operator services — durable local gateway delivery authority and cron schedule store owned by optimus-ops, with observability contracts.
reviewed_on: 2026-08-05
review_by: 2026-10-31
knowledge_type: specification
covers:
  - crates/optimus-ops/src/**
validated_by:
  - crates/optimus-ops/tests/**
  - crates/optimus-kernel/tests/**
---

# 007 — Operator services

Status: active
Owner: development agents (main-only)

## Purpose

The operator plane: durable local gateway delivery authority, cron schedule
store, and runtime observability. The kernel re-exports these for surface
convenience; the operator services own the durable state.

## Requirements

- R1. Cron schedules MUST be durable (`cron_list`, `cron_add`, `cron_tick`,
  `cron_set_enabled`, `cron_remove`, `cron_history`); a tick MUST produce
  exactly one terminal outcome.
- R2. Gateway delivery MUST be a durable local authority with inbox/outbox,
  enqueue, ambiguity handling, and ack'd delivery (`gateway_status`,
  `gateway_inbox`, `gateway_outbox`, `gateway_enqueue`, `gateway_ambiguous`,
  `gateway_ack_delivery`, `gateway_telegram_status`).
- R3. Runtime events MUST be observable and ordered (architectural law 11);
  observability must not depend on any single surface.
- R4. `logs_tail` and `commands_list` MUST remain bounded runtime consoles.

## Acceptance criteria
- [ ] A1. Given the ops crate suite, when it runs, then all tests pass.
- [ ] A2. Given the observability map, when it is compared with the implemented surfaces, then it matches.

## Out of scope

- Evaluation harnesses (spec 008).

## Open questions

- None.

## Links

Code: crates/optimus-ops · ADRs: 0060 · Ontology: optimus-ops
