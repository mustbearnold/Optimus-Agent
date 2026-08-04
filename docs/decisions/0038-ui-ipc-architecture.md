---
doc_id: decisions-0038-ui-ipc-architecture
doc_type: decision
plane: decision
status: current
authority: record
summary: Decision record for ADR-0038: UI IPC architecture completeness (P15), including its context, consequences, and current documentary status.
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: decision
covers:
  - scripts/check-desktop-ipc-matrix.py
  - scripts/test_desktop_ipc_matrix.py
  - apps/optimus-ui/src/ipc/contracts.ts
  - crates/optimus-host/src/router.rs
  - scripts/rebuild-install-relaunch.sh
depends_on:
  - docs/decisions/0028-electron-react-shell-rust-host.md
  - docs/decisions/0029-react-workbench-and-electron-preview-view.md
validated_by:
  - scripts/check-desktop-ipc-matrix.py
  - scripts/test_desktop_ipc_matrix.py
  - apps/optimus-ui/src/state/conversationStore.test.ts
---

# ADR-0038: UI IPC architecture completeness (P15)

- **Status:** Accepted
- **Date:** 2026-07-25

## Context

UI architecture was **A-**: Electron+React is the default shell and an IPC matrix
gate existed, but critical invoke coverage was incomplete relative to product
paths (`term_run`, jobs, session rename/delete), host methods needed explicit
non-invoke classification, and preview sandbox guarantees needed a lightweight
static test.

## Decision

1. **Default shell:** Electron + React over `optimus-desktop --host-only`.
   Legacy Wry remains optional (`LegacyWry` desktop action) and is not required
   for daily-path gates.
2. **IPC matrix law:** host `METHOD_DOMAINS` ⊇ Electron `DESKTOP_METHODS` =
   React `DesktopMethod`. Every host method is either invoke-allowlisted or
   documented in `HOST_NON_INVOKE_CHANNELS` (including main-only staging).
3. **Critical invoke set (P15 expanded):** sessions CRUD surface, chat approval
   resolve, project scopes, approvals, fs, settings, doctor, `term_run`,
   `jobs_list`.
4. **Main-only:** `project_root_stage_native` never on renderer allowlists;
   preload must not expose it.
5. **Preview:** `WebContentsView` with `nodeIntegration: false`,
   `contextIsolation: true`, `sandbox: true`, separate partition, denied
   permissions/downloads, navigation policy — product language remains
   **preview**, not agent browser tools.
6. **Cancel honesty:** UI run status comes from host stream lifecycle
   (`conversationStore`); Stop requests cooperative cancel — no optimistic
   success terminal without host event.
7. **Install truth:** `rebuild-install-relaunch.sh` stages Electron as primary;
   Legacy Wry secondary.

## Consequences

- Positive: UI mark can move to **S+++** with fail-closed matrix + preview tests.
- Residual: full Playwright e2e for every approval path remains supplementary;
  matrix + unit/security tests are the merge gate. Live native install still
  uses `skills/optimus-native-ui-testing` when claiming shell changes.

## Alternatives considered

- **Expose all host methods to Electron invoke.** Rejected: chat/window/OS need
  dedicated channels; main-only staging must stay host-gated.
- **Require Playwright for every PR.** Rejected as sole gate; too slow; matrix is
  deterministic merge bar.

## Risks

- New host methods forgotten in matrix classification. Mitigated by
  `uncovered` error and unit test `test_every_host_method_is_classified`.

## Conditions for reconsideration

- Add vertical IPC methods to critical set when product surfaces them.

## Documentation completion addendum (2026-07-31)

## Reasons

The decision makes the invariant in the Decision section explicit and testable. It is preferred because the failure described in Context cannot be managed reliably through prompt convention or caller discipline alone.

## Evaluation evidence

- `scripts/check-desktop-ipc-matrix.py`
- `scripts/test_desktop_ipc_matrix.py`
- `apps/optimus-electron/test/preview-security.test.cjs`
- `apps/optimus-electron/test/browser-policy.test.cjs`
- `apps/optimus-ui/src/state/conversationStore.test.ts`

## Relevant code

- `scripts/check-desktop-ipc-matrix.py`
- `scripts/test_desktop_ipc_matrix.py`
- `apps/optimus-electron/main.cjs`
- `apps/optimus-electron/preload.cjs`
- `apps/optimus-ui/src/ipc/contracts.ts`
- `crates/optimus-host/src/router.rs`
- `scripts/rebuild-install-relaunch.sh`

## Relevant tests

- `scripts/check-desktop-ipc-matrix.py`
- `scripts/test_desktop_ipc_matrix.py`
- `apps/optimus-electron/test/preview-security.test.cjs`
- `apps/optimus-electron/test/browser-policy.test.cjs`
- `apps/optimus-ui/src/state/conversationStore.test.ts`
