---
knowledge_type: specification
status: active
covers:
  - crates/optimus-kernel/src/product_settings.rs
  - crates/optimus-host/src/system.rs
  - apps/optimus-desktop/ui/index.html
  - apps/optimus-desktop/ui/app.js
depends_on:
  - docs/decisions/0027-settings-driven-work-isolation.md
validated_by:
  - cargo test -p optimus-kernel --lib product_settings
  - cargo test -p optimus-desktop --bin optimus-desktop -- settings_ doctor_
---

# Phase 0 — Work isolation settings (durable, no enforcement yet)

**Date:** 2026-07-23  
**Goal:** Let users choose work isolation intent in Settings; persist under home.

## Acceptance

| # | What | Done when |
|---|---|---|
| 1 | Durable `settings.json` under Optimus home | load/save round-trip |
| 2 | Fields | `work_isolation`, `allow_concurrent_projects`, schema version |
| 3 | IPC | `settings_get`, `settings_set` |
| 4 | Doctor | includes isolation fields |
| 5 | Settings UI | radio modes + concurrent checkbox |
| 6 | Status bar | shows short mode label |
| 7 | No tool/policy change | shared behavior unchanged until Phase 1 |

## Out of scope

- Project-bound FS enforcement
- Profile homes
- Parallel run leases
