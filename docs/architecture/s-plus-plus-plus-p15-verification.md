# S+++ P15 verification — UI IPC architecture

Date: 2026-07-25  
Planes: program **P15** · decision **ADR-0038** · delivery **PR #25**

## Exit evidence

| Microtask | Evidence |
|---|---|
| U1 Host classification | `check-desktop-ipc-matrix.py` `coverage=host_methods_all_classified` |
| U2 Critical approvals/scopes | Critical set includes `chat_approval_resolve`, `approvals_grant`, `project_scopes_*`; main_only not on renderer |
| U3 Preview sandbox | `apps/optimus-electron/test/preview-security.test.cjs` + browser-policy tests |
| U4 Install Electron primary | `rebuild-install-relaunch.sh` stages Electron; LegacyWry secondary action |
| U5 UI **S+++** | architecture-marks + ADR-0038 |

## Commands

```bash
python3 scripts/check-desktop-ipc-matrix.py
python3 scripts/test_desktop_ipc_matrix.py
node --test apps/optimus-electron/test/preview-security.test.cjs apps/optimus-electron/test/browser-policy.test.cjs
```

## Grade moves

| Mark | Before | After |
|---|---|---|
| UI architecture | A- | **S+++** |
