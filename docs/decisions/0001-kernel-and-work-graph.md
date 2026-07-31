---
doc_id: decisions-0001-kernel-and-work-graph
doc_type: decision
plane: decision
status: current
authority: record
summary: Decision record for ADR-0001: Kernel language and Work Graph durability spine, including its context, consequences, and current documentary status.
reviewed_on: 2026-07-31
review_by: 2026-10-31
---

# ADR-0001: Kernel language and Work Graph durability spine

## Status

Accepted — 2026-07-18

## Context

Hermes Agent’s learning-loop product is strong, but long-horizon work is split across process-local `delegate_task`, a separate cron scheduler, and kanban — and large Python modules host the loop. Optimus must exceed Hermes on crash safety and multi-day campaigns.

## Decision

1. **Kernel and durable runtime are Rust** in a Cargo workspace.
2. **One Work Graph** is the durability spine for:
   - interactive turn tool steps (later phases)
   - background jobs
   - subagents
   - cron/schedule ticks
   - multi-day campaigns
3. **Process is replaceable; the graph is not.** Every node commit is durable before side effects are considered done.
4. **Commit boundary:** a node moves `pending → running → succeeded|failed|cancelled` with an append-only event log. Resume never re-runs a `succeeded` node; `running` at crash is treated as **uncertain** and re-enters a deterministic recovery policy (default: fail-closed to `interrupted`, allow explicit retry).
5. **Phase 0 scope:** SQLite-backed job + ordered nodes + file/terminal effectors + crash-resume golden test. No LLM required.

## Consequences

- Positive: single mental model; testable resume; Windows service-quality path later.
- Negative: steeper initial scaffold than a Python script agent.
- Neutral: Hermes skill/session import deferred; OpenAI-tool-loop compatibility later via adapter, not core identity.

## Alternatives rejected

- **Hermes fork in Python:** inherits god-module accretion and weak durability.
- **Separate cron/delegate/kanban stores:** repeats Hermes fragmentation.
- **At-least-once blind replay of `running` nodes:** unsafe for non-idempotent tools; Optimus marks interrupted and retries only under policy.
