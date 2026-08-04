---
doc_id: contracts-desktop-shell-and-ipc-matrix
doc_type: reference
plane: current
status: current
authority: canonical
summary: Install authority: scripts/rebuild-install-relaunch.sh stages Tauri as the primary entry and keeps Electron and Wry as explicit rollback shells.
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: contract
owns:
  - scripts/check-desktop-ipc-matrix.py
  - scripts/test_desktop_ipc_matrix.py
  - crates/optimus-host/src/router.rs
  - apps/optimus-electron/main.cjs
  - apps/optimus-ui/src/ipc/contracts.ts
  - docs/contracts/desktop-ipc-methods.md
covers:
  - scripts/check-desktop-ipc-matrix.py
  - apps/optimus-electron/main.cjs
  - apps/optimus-ui/src/ipc/contracts.ts
depends_on:
  - docs/contracts/desktop-ipc-methods.md
  - docs/decisions/0028-electron-react-shell-rust-host.md
  - docs/architecture/architecture-marks.md
validated_by:
  - scripts/test_desktop_ipc_matrix.py
  - crates/optimus-host/src/router.rs
  - apps/optimus-electron/test/ipc-allowlist.test.cjs
---

# Desktop shell authority and IPC matrix (Phase 4)

## Default shell (truth freeze)

| Surface | Role |
|---|---|
| **Tauri + React** (`apps/optimus-tauri` + `apps/optimus-ui`) | **Default installed desktop** and repository daily path |
| **Rust host** (`optimus-desktop --host-only`) | Authority: IPC registry, sessions, SmartDeny, chat streams |
| **Electron + React** (`apps/optimus-electron`) | **Browser-preview rollback** while Tauri parity is completed |
| **Wry/Tao** (`optimus-desktop` native window) | **Legacy rollback only** (desktop action `LegacyWry`) |

Install authority: `scripts/rebuild-install-relaunch.sh` stages Tauri as the
primary entry and keeps Electron and Wry as explicit rollback actions.

## IPC ownership matrix

| Channel | Owner | Critical methods / affordances |
|---|---|---|
| Host registry `METHOD_DOMAINS` | `optimus-desktop` router | Full frozen method set |
| Electron `DESKTOP_METHODS` | `main.cjs` invoke allowlist | Subset of registry for renderer `invoke` |
| React `DesktopMethod` | `optimus-ui` contracts | **Must equal** Electron allowlist |
| Chat SSE | Electron main + host `/api/chat/stream` | start / cancel / subscribe (not `invoke`) |
| Window / pick folder | Preload dedicated channels | Not in `DESKTOP_METHODS` |
| `project_root_stage_native` | Electron main only | Never in renderer allowlist |

### Critical invoke paths (must stay on all three parse surfaces)

`ping`, `doctor`, `sessions`, `new_session`, `get_session`, `delete_session`,
`rename_session`, `chat_approval_resolve`, `project_scopes_list`,
`project_scopes_authorize`, `approvals_list`, `approvals_grant`, `fs_roots`,
`fs_list`, `fs_read`, `settings_get`, `settings_set`, `term_run`, `jobs_list`

(Authority list: `scripts/check-desktop-ipc-matrix.py` `CRITICAL_INVOKE_METHODS`.)

Validator:

```bash
python3 scripts/check-desktop-ipc-matrix.py
python3 scripts/test_desktop_ipc_matrix.py
```

## Browser product language

| Name | What it is | Authority |
|---|---|---|
| **Preview browser** | Electron sandboxed `WebContentsView` in the workbench | User navigation/annotations; no agent cookies/history claim |
| **Agent browser tools** | Kernel `browser_*` effectors (HTTP SSRF-safe / CDP when available) | Agent automation; separate state from preview |

UI surfaces should say **Preview browser**, never claim shared session with agent tools.

## Renderer non-authority

Local React state (`rootPaths[]`, layout, theme) is presentation only. Project
filesystem authority remains Rust-owned (`project_scopes_*` + native grant
staging). Forged renderer roots must fail closed.
