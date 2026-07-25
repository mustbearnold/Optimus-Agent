# Product-complete program P22 hold — 2026-07-25

Planes: **program P22** · delivery **PR #31** · architecture hold

## Board

Three-expert review (architecture / product / docs) + synthesis → **APPROVE-WITH-FIXES**.

### MUST-FIX applied
1. Demote ledger `projects.scope` → **partial** (honesty only; concurrent lease residual)
2. Doctor/settings fail-closed isolation defaults; desktop test expects honesty for isolated_profiles
3. Status bar uses **enforced_mode** (never labels isolated when enforced shared)
4. Remove unenforced concurrent-deny prose from project_bound note
5. Mkdir SmartDeny approvals_surface test
6. Scorecard/program/microtask residual honesty

## Commands (green on tip after fixes)
```
python3 scripts/check-parity-ledger.py
python3 scripts/check-architecture-marks.py
python3 scripts/check-domain-modularity.py
cargo test -p optimus-runtime --test path_confinement --test approvals_surface
cargo test -p optimus-desktop --lib  # system IPC honesty tests
```

## Ledger
- files.mutate → parity
- projects.scope → partial
- win 4 · parity 13 · partial 12 · missing 22
