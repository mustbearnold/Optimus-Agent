# Product-complete program P26 hold — 2026-07-25

Planes: **program P26** · delivery **PR #36** · architecture hold (Domain /
Security / Doc hygiene)

## Board

Three-expert review (security / product-ledger / correctness) →
**APPROVE-WITH-FIXES** (product + correctness initially required fixes;
security **APPROVE**).

### MUST-FIX applied

1. Memory console scope matches `KernelConfig::default().memory_ctx` (not a
   isolated `console` project that hid agent claims)
2. Correction timestamps use `SystemMemoryClock` RFC3339 (not a broken 1970 stamp)
3. `list_recent` excludes tombstoned and knowledge-closed rows so Forget/Correct
   match recall semantics
4. Command palette opens the matching console tab (`skills`/`memory`/`packs`/`logs`)
5. Pack activate persistence asserted via `pack_prefs.json` reload
6. Scorecard “leading losses” no longer lists landed Memory/Skills/Packs/Logs
7. Program residual/anchor prose updated for P26 consoles complete
8. `core.pack-budget` evidence includes packs console paths

### SHOULD-FIX noted (not blocking)

- CLI consumer of surface registry still optional residual
- Palette `help` / `sessions` / `memory.recall` no-ops
- Multi-project memory scope UI later
- Pack prefs ≠ live mid-chat Kernel packs

## Commands (green after fixes)

```text
cargo test -p optimus-ops --lib surface_commands
cargo test -p optimus-desktop -- --test-threads=1
npm test ConsolesPage CommandPalette
python3 scripts/check-desktop-ipc-matrix.py
python3 scripts/check-parity-ledger.py
```

## Ledger

- `skills.ui` → parity
- `memory.ui` → parity
- `desktop.logs` → parity
- `surface.commands` → parity
- `core.pack-budget` product story completed by packs console (kernel already parity)

## Non-claims

- Memory ActionAuthorize
- UI inventing tools outside packs
- Live mid-chat pack mutation from consoles
- Hermes gate PASS

## Verdict

**program P26 closed after review board fixes.** Next: **program P27** or **P28**.
