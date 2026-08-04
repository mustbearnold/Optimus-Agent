---
doc_id: decisions-0027-settings-driven-work-isolation
doc_type: decision
plane: decision
status: current
authority: record
summary: Decision record for ADR-0027: Settings-driven work isolation modes, including its context, consequences, and current documentary status.
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: decision
covers:
  - crates/optimus-kernel/src/product_settings.rs
  - crates/optimus-host/src/system.rs
  - apps/optimus-desktop/ui/index.html
  - apps/optimus-desktop/ui/app.js
depends_on:
  - docs/decisions/0018-fail-closed-runtime-safety.md
  - docs/architecture.md
validated_by:
  - crates/optimus-kernel/src/product_settings.rs
  - crates/optimus-host/src/system.rs
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

## Reasons

A durable mode gives every product surface one truth source while preserving a
low-ceremony shared default. Explicitly separating configured intent from
enforcement prevents the Settings UI and local project catalog from implying a
security boundary that the runtime has not implemented.

## Risks and unresolved boundaries

- **Planned behaviour:** `project_bound` does not yet bind tool roots, memory,
  browser profiles, or job leases.
- **Planned behaviour:** `isolated_profiles` does not yet provision sealed
  homes or migration/recovery flows.
- **Unknown or unresolved behaviour:** concurrent-project denial has no runtime
  effect until project ownership and leases are typed.

## Evaluation evidence

- Product-setting unit tests cover missing, valid, malformed, and unknown mode
  persistence.
- Desktop system IPC tests cover get/set and Doctor projection.
- React Settings and multi-folder contracts label enforcement independently
  from the local catalog.

## Relevant code

- `crates/optimus-kernel/src/product_settings.rs`
- `apps/optimus-desktop/src/ipc/system.rs`
- `apps/optimus-ui/src/components/settings/SettingsDialog.tsx`
- `apps/optimus-ui/src/state/projectStore.ts`

## Relevant tests

- Unit tests colocated with `crates/optimus-kernel/src/product_settings.rs`
- Unit tests colocated with `apps/optimus-desktop/src/ipc/system.rs`
- `apps/optimus-ui/src/state/projectStore.test.ts`
- `apps/optimus-electron/e2e/react-browser-contract.spec.cjs`

## Conditions for reconsideration

Reconsider the mode vocabulary only with a migration for durable
`settings.json` values. Reconsider the shared default after project-bound
enforcement, migration, rollback, and installed multi-project evidence are
green.
