---
knowledge_type: decision
status: current
covers:
  - crates/optimus-kernel/src/product_settings.rs
  - apps/optimus-desktop/src/ipc/system.rs
  - apps/optimus-desktop/ui/index.html
  - apps/optimus-desktop/ui/app.js
depends_on:
  - docs/decisions/0018-fail-closed-runtime-safety.md
  - docs/architecture/system-overview.md
validated_by:
  - crates/optimus-kernel/src/product_settings.rs
  - apps/optimus-desktop/src/ipc/system.rs
last_verified_commit: null
---

# ADR-0027: Settings-driven work isolation modes

- **Status:** Accepted
- **Date:** 2026-07-23

## Context

Users want multi-project parallel work without interference (a known Hermes
pain point). Optimus today uses one home/workspace; Projects are primarily UI
session folders. Forcing isolated profiles on everyone would break the simple
daily workbench. Forcing shared-only forever cannot deliver true non-interference.

## Decision

1. Product settings own a durable **work isolation mode** under the Optimus home
   (`settings.json`), not only browser `localStorage`.
2. Three modes are defined:
   - `shared` — one home/workspace; projects organize sessions (default).
   - `project_bound` — active project roots tools, memory scope, and browser
     profile dirs (Phase 1+ enforcement).
   - `isolated_profiles` — each project maps to a sealed profile home
     (Phase 2+ enforcement).
3. Settings also store `allow_concurrent_projects` (bool). When false, concurrent
   mutating runs across projects are policy-denied once enforcement lands.
4. Mode changes are explicit user actions. No silent data merge across modes.
5. **Phase 0** (this delivery): persist + surface mode in Settings UI, status
   bar, and Doctor. Runtime enforcement of B/C is deferred; values other than
   `shared` are stored and displayed as **configured intent**.
6. Kernel and desktop must not invent a fourth mode string. Unknown values fail
   closed to `shared` with a load note.

## Alternatives considered

- Profiles as the only mode. Rejected: high ceremony for single-project users.
- UI-only labels without durable settings. Rejected: not enforceable or portable.
- Env-only isolation. Rejected: not discoverable in product Settings.

## Consequences

- Default remains shared; no behavior regression for existing installs.
- Later phases bind tool FS, memory, browser, and job leases to the selected mode.
- Doctor reports both configured mode and whether enforcement is active.
