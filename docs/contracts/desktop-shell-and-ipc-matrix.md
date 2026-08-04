---
doc_id: contracts-desktop-shell-and-ipc-matrix
doc_type: reference
plane: current
status: current
authority: canonical
summary: Install authority: scripts/rebuild-install-relaunch.sh stages Tauri + React as the desktop entry and keeps Wry as the legacy rollback shell.
reviewed_on: 2026-08-05
review_by: 2026-10-31
knowledge_type: contract
owns:
  - scripts/check-desktop-ipc-matrix.py
  - scripts/test_desktop_ipc_matrix.py
  - crates/optimus-host/src/router.rs
  - apps/optimus-tauri/src/main.rs
  - apps/optimus-ui/src/ipc/contracts.ts
  - docs/contracts/desktop-ipc-methods.md
covers:
  - scripts/check-desktop-ipc-matrix.py
  - apps/optimus-tauri/src/main.rs
  - apps/optimus-ui/src/ipc/contracts.ts
depends_on:
  - docs/contracts/desktop-ipc-methods.md
  - docs/decisions/0028-electron-react-shell-rust-host.md
  - docs/architecture/architecture-marks.md
validated_by:
  - scripts/test_desktop_ipc_matrix.py
  - crates/optimus-host/src/router.rs
---

# Desktop shell authority and IPC matrix

## Default shell (truth freeze)

| Surface | Role |
|---|---|
| **Tauri + React** (`apps/optimus-tauri` + `apps/optimus-ui`) | **Default installed desktop** and repository daily path |
| **Rust host** (`optimus-desktop --host-only`) | Authority: IPC registry, sessions, SmartDeny, chat streams |
| **Wry/Tao** (`optimus-desktop` native window) | **Legacy rollback only** (desktop action `LegacyWry`) |

Install authority: `scripts/rebuild-install-relaunch.sh` stages Tauri as the
desktop entry and keeps Wry as the legacy rollback action. Electron is retired.

## IPC ownership matrix

| Channel | Owner | Critical methods / affordances |
|---|---|---|
| Host registry `METHOD_DOMAINS` | `optimus-desktop` router | Full frozen method set |
| React `DesktopMethod` | `optimus-ui` contracts | Renderer surface: every method must exist in the host registry |
| Tauri bridge `host_invoke` | `apps/optimus-tauri` command | Forwards any registry method; unknown methods fail in the host |
| Chat stream | Tauri `chat_start`/`chat_cancel` commands + host `/api/chat/stream` | start / cancel / subscribe (not `host_invoke`) |
| Window / pick folder | Tauri `window_action` / `pick_folder` commands | Not in `DesktopMethod` |
| `project_root_stage_native` | Rust host + shell internal | Never in the renderer surface |

### Critical invoke paths (must stay on the renderer surface and host registry)

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
| **Agent browser tools** | Kernel `browser_*` effectors (HTTP SSRF-safe / CDP when available) | Agent automation; renderer calls them as ordinary host methods |

The Electron `WebContentsView` preview is retired with Electron; the workbench
Browser surface is the kernel browser effector. UI copy should never claim
shared session state with agent tools it does not own.

## Renderer non-authority

Local React state (`rootPaths[]`, layout, theme) is presentation only. Project
filesystem authority remains Rust-owned (`project_scopes_*` + native grant
staging). Forged renderer roots must fail closed.
