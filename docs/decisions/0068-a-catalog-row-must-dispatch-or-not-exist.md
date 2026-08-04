---
doc_id: decisions-0068-a-catalog-row-must-dispatch-or-not-exist
doc_type: decision
plane: decision
status: current
authority: record
summary: Removes the nine scaffold tool rows that cannot honestly ship within one quarter, keeps the five with committed implementation lanes, and re-marks packs.breadth from parity to missing; a refusing catalog row teaches the model false affordances and costs prompt budget for nothing.
reviewed_on: 2026-08-01
review_by: 2026-11-01
knowledge_type: decision
covers:
  - crates/optimus-packs/src/lib.rs
  - crates/optimus-packs/src/catalog.rs
  - scripts/check-tool-coverage.py
  - docs/architecture/parity-capability-ledger.json
depends_on:
  - docs/decisions/0036-domain-modularity-single-catalog.md
  - docs/plans/competitive-bottleneck-audit.md
validated_by:
  - crates/optimus-kernel/tests/tool_coverage.rs
  - scripts/check-tool-coverage.py
  - scripts/verify.sh
---

# ADR-0068: A catalog row must dispatch or not exist

- **Status:** Accepted
- **Date:** 2026-08-01

## Context

The single catalog (ADR-0036) classified 31 tools: 17 dispatchable and 14
declared-but-refusing scaffolds, six packs shipping zero tools. The 2026-08-01
competitive audit (B-CAP-01) established what the scaffolds cost: every
refusing row is presented to the model as an affordance, spends prompt budget
on a schema that cannot execute, and converts user intent into a refusal
instead of an honest absence. The parity ledger's `packs.breadth` row claimed
`parity` with the scaffold declarations themselves as evidence — breadth on
paper, refusal at dispatch.

## Decision

1. A builtin catalog row exists only when its tool dispatches, or when its
   implementation lane is committed inside the current quarter. Nine rows
   leave the catalog now: `job_run` (runtime-internal; the Work Graph owns
   job advancement), `image_generate` and `tts` (the media lane's local-first
   design is undecided), `gh_pr` and `git_deep` (terminal covers real use
   until a designed lane exists), `x_search`, `message_send` (meaningless
   until a live gateway transport ships), `home_device_status` and
   `office_doc_summary` (breadth scaffolds with no integration behind them).
2. Five scaffolds remain, each with a committed lane: `vision_analyze`
   (next capability land; consumes the artifact-store screenshots the
   browser tools now produce), `clarify` (rides the ADR-0046 resume
   mechanics), and the three `desktop_*` effectors (ship under heavy
   approval once the installed-app baseline is committed).
3. `packs.breadth` re-marks from `parity` to `missing` with this ADR as
   evidence. A missing capability stated plainly outranks a parity claim
   backed by refusals.
4. Removing a row is not removing the ambition: each deleted tool returns in
   the land that implements it, with a trajectory in the same commit.
   Emptied packs keep their identity so activation, policy vocabulary, and
   future restorations stay stable.

## Consequences

- The model-visible tool surface tells the truth; refused-scaffold prompt
  cost drops to zero for the deleted rows.
- The ledger shows its first honest `missing` row; the scorecard's "0
  missing" line was cosmetics purchased with refusing scaffolds.
- Coverage pins move 17/14 to 17/5 and may only move again alongside the
  catalog in the same commit, as the gate already requires.

## Evaluation evidence

- `crates/optimus-kernel/tests/tool_coverage.rs` proves the remaining five
  refuse with the typed unavailable contract and the seventeen dispatch.
- `scripts/check-tool-coverage.py` pins the new counts.

## Reconsider when

A deleted lane's implementation land arrives (restore its row with its
trajectory), or product evidence shows a scaffold's schema-visibility itself
guided users better than absence — no such evidence existed at decision time.
