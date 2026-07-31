---
doc_id: architecture-s-plus-plus-plus-p15-verification
doc_type: history
plane: history
status: historical
authority: historical
summary: Date: 2026-07-25 Planes: program P15 · decision ADR-0038 · delivery PR #25
reviewed_on: 2026-07-31
review_by: never
---

# S+++ P15 verification — UI IPC architecture

Date: 2026-07-25  
Planes: program **P15** · decision **ADR-0038** · delivery **PR #25**

## Exit evidence

| Microtask | Evidence |
|---|---|
| U1 Host classification | `check-desktop-ipc-matrix.py` `coverage=host_methods_all_classified` |
| U2 Critical approvals/scopes | Critical set + matrix/unit tests (allowlist authority). Program “e2e” bar superseded by deterministic matrix + main_only denial (ADR-0038 residual: Playwright supplementary) |
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
