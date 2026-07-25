---
knowledge_type: verification
status: current
owns:
  - docs/architecture/product-complete-p26-verification.md
covers:
  - docs/plans/product-complete-program.md
depends_on:
  - docs/plans/product-complete-program.md
validated_by:
  - apps/optimus-desktop/src/ipc/consoles.rs
  - crates/optimus-ops/src/surface_commands.rs
  - crates/optimus-memory/src/lib.rs
  - apps/optimus-ui/src/components/consoles/ConsolesPage.tsx
  - apps/optimus-ui/src/components/chrome/CommandPalette.tsx
  - scripts/check-desktop-ipc-matrix.py
last_verified_commit: null
---

# Product-complete program P26 verification

Planes: **program P26** · delivery **PR #36** · architecture hold (Domain /
Security / Doc hygiene) · ledger `skills.ui`, `memory.ui`, `desktop.logs`,
`surface.commands` → **parity** (packs console completes product story for
`core.pack-budget`)

Date: 2026-07-25

## Goal

Surface existing skills/memory/packs backends with secure consoles; redacted
logs drawer; unified surface command registry + palette (not a second tool list).

## What landed

| Item | Result | Evidence |
|---|:---:|---|
| Skills list/pin/deprecate | **PASS** | `skills_*` IPC + console |
| Memory list/recall/correct/forget | **PASS** | ActionAuthorize refused |
| Memory fence on packets | **PASS** | unit + UI |
| Memory console scope = kernel default | **PASS** | `console_ctx` ≡ `KernelConfig::default().memory_ctx` |
| Correction timestamps RFC3339 | **PASS** | `SystemMemoryClock` (not fake 1970 stamp) |
| Packs activate/deactivate | **PASS** | CapabilitySession + `pack_prefs.json` |
| No second tool catalog | **PASS** | catalog from packs crate only |
| logs_tail redaction | **PASS** | home path redacted |
| Surface commands registry | **PASS** | `surface_commands` + palette |
| IPC matrix | **PASS** | 12 console methods registered |

## Residuals

| Residual | Owner |
|---|---|
| Pack prefs are console/session preferences, not live Kernel turn packs mid-chat | chat activate_pack still turn-owned |
| Logs are diagnostic aggregates, not full stderr capture | observability product depth |
| CLI `commands list` subcommand optional | registry is shared; CLI listing can call same fn |
| Multi-project memory scope UI (non-default project) | later project console / P27+ |

## Hold suite

```bash
cargo test -p optimus-ops --lib surface_commands
cargo test -p optimus-desktop -- --test-threads=1
cd apps/optimus-ui && npm test -- ConsolesPage CommandPalette
python3 scripts/check-desktop-ipc-matrix.py
python3 scripts/check-parity-ledger.py
```

## Non-claims

- Memory ActionAuthorize
- UI inventing tools outside packs
- Hermes gate PASS
- Live mid-chat pack mutation from consoles

## Board

See `docs/evidence/product-complete-p26-hold-2026-07-25.md`.

## Verdict

**program P26 exit: PASS** after review-board MUST-FIX (PR #36).
Next: program P27 extensibility or P28 messaging.
