# Product-complete program P21 hold evidence — 2026-07-25

Planes: **program P21** · delivery **PR #30** · architecture hold

## Three-expert review board

| Lens | Result |
|---|---|
| Domain / Security S+++ hold | **APPROVE-WITH-FIXES** (late full text): no S+++ demotion; no ads↔handler MUST-FIX; SHOULD registry/script hygiene only |
| Tool-loop completeness (S1.1/S1.2/S1.4) | **APPROVE-WITH-FIXES** → fixes applied |
| Docs / ledger / microtask hygiene | **APPROVE-WITH-FIXES** → fixes applied |

Architecture hold agent returned after hygiene fixes were already applied; its
MUST-FIX list was empty for Domain/Security (doc/SHOULD only). Tool-loop + docs
MUST-FIX items from the other two lenses (and synthesis) were applied in this
hold.

## MUST-FIX applied

1. Microtasks S1.1 / S1.2 / S1.4 → `done` in `full-app-microtasks.md`
2. Residual ownership + P29 partial list: `core.tool-loop` / `core.pack-budget` HOLD parity
3. Immediate next action → open program P22
4. Kernel PackLimit typed ToolOutcome test added
5. Ledger pack-budget evidence: removed weak desktop e2e budget claim; added this hold file
6. Scorecard marker includes program P21

## Commands (2026-07-25)

```text
cargo test -p optimus-kernel --test kernel_turn activate_pack -- --test-threads=1
  activate_pack_increases_tools_and_tokens ... ok
  activate_pack_on_demand_limit_returns_typed_tool_outcome_not_turn_abort ... ok
  activate_pack_schema_budget_returns_typed_tool_outcome_not_turn_abort ... ok

python3 scripts/check-domain-modularity.py → OK
python3 scripts/check-architecture-marks.py → OK
python3 scripts/optimus_version.py release-check → PASS (Hermes gate BLOCKED by design)
python3 scripts/check-parity-ledger.py → ok after this evidence path exists
```

Also previously green: `cargo test -p optimus-packs --test packs_budget` (30),
full `kernel_turn` + `domain_modularity` suites.

## Ledger

- `core.tool-loop` → parity
- `core.pack-budget` → parity
- Rollup: win 4 · parity 12 · partial 12 · missing 23

## Non-claims

- Table-driven envelope for every `ALL_DISPATCHABLE` tool (SHOULD residual)
- Packs console UI (program P26)
- files.mutate (program P22)
- Hermes `gate` PASS

## Verdict

**program P21 closed after review board fixes.** Next: **program P22**.
