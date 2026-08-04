---
doc_id: evidence-pr32-run-controller-p0-board-2026-07-25
doc_type: evidence
plane: evidence
status: historical
authority: record
summary: Planes: design-note multi-agent P0 (not program phase) · delivery PR #32 · architecture S+++ hold
reviewed_on: 2026-07-31
review_by: never
---

# PR #32 three-expert review board — 2026-07-25

Planes: design-note multi-agent P0 (not program phase) · delivery **PR #32** · architecture S+++ hold

## Board

| Lens | Verdict |
|---|---|
| Architecture / Multi-agent / Durability | **APPROVE-WITH-FIXES** → fixes applied |
| Security / Claim honesty | **APPROVE-WITH-FIXES** → fixes applied |
| API correctness / Tests | **BLOCK** on tip → MUST-FIX applied → **APPROVE** |

Synthesis: **APPROVE** after MUST-FIX.

Raw lens notes (session): `local/tmp/pr32-review-{architecture,security,correctness}.md`.

## MUST-FIX applied

1. **Accept cannot overwrite cancel/budget terminal** — `check_budgets` / `transition` return `Err` after fail-closed terminalization; Accept commits only via `force_terminal(Succeeded)`; sole terminal writer. Regression: `accept_cannot_overwrite_cancel_terminal`.
2. **Plan attempt budget on revise + replan** — `charge_plan_attempt` used by `begin_planning`, `plan_revise`, and gate Replan; Rmax ∩ plan budget. Regressions: `plan_revise_respects_max_plan_attempts`, `replan_then_accept_respects_rmax`.
3. **GateDecision matches actual state** — decision built after side effects; preemption returns `gate_decision_from_terminal`.
4. **Claim honesty docs** — module docs: not WorkflowRunStore, no SmartDeny bypass, free-form ids are not capability grants.

## SHOULD residuals (not blocking merge)

- Public mutable control fields (host discipline until P1 encapsulation)
- Envelope provenance spine (`run_id`/`attempt_id`/`content_sha256` on every envelope) deferred
- Cancel tree fan-out to ADR-0033 children deferred (P0 non-claim)
- Registry resolution of `tool_ids`/`specialist_ids` when worker wiring lands (P1+)

## Commands (green on tip after fixes)

```text
cargo test -p optimus-workflow --lib -- --test-threads=1
  22 passed
```

## Non-claims

- Model spawn / specialist wiring (P1+)
- Durable WorkflowRunStore integration
- SmartDeny / host effect execution
- Hermes parity / product-complete program phase

## Verdict

**PR #32 ready to merge after board fixes.**
