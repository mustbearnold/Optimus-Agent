# Phase 3 verification — 2026-07-18

## Scope delivered

Per ADR-0005 — crate `optimus-skills`:

| Capability | Behavior |
|---|---|
| Create | Always `candidate` (or human `pinned`) |
| Outcomes | `record_outcome` updates uses/successes/failures/tokens |
| Promote | `try_promote` only if uses≥3 and success_rate≥0.8 |
| Permissions closed | `authorize` requires ⊆ declared; `update_body` cannot expand |
| Resolve | pinned > proven > candidate; skips deprecated |
| CLI | `optimus skills list\|create\|resolve` |

## Gates

| Gate | Result |
|---|---|
| fmt | pass |
| clippy `-D warnings` | pass |
| `cargo test --workspace` | **21 passed** |
| doctor | phase 3 skills-2.0 |
| skills create/list/resolve smoke | ok |

### Skills tests (6)

- create → candidate
- promote gates (uses + rate)
- authorize rejects undeclared Net
- update cannot expand permissions
- resolve prefers pinned over proven
- deprecated excluded from list/resolve

Prior phases (0–2) remain green.

## Exceeds Hermes

Hermes: create skill after task, weak measurement, curator TTL hygiene.  
Optimus: **outcome-gated promotion**, **closed permission sets**, versioned resolve order.

## Not yet

- agentskills.io / Hermes skill import
- Executable skill checks in promote
- LLM loop auto skill_load
- Runtime wiring of skill authorize → SmartDeny grants
