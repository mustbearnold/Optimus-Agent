---
knowledge_type: contract
status: current
covers:
  - apps/optimus-desktop/src/ipc/router.rs
  - apps/optimus-desktop/src/main.rs
  - apps/optimus-desktop/src/bridge.rs
  - apps/optimus-desktop/src/native_workers.rs
  - apps/optimus-desktop/src/preview_embed.rs
---

# Desktop IPC method inventory (frozen for shell migration)

Wire request: `{ "id": number, "method": string, "params": object }`  
Wire reply: `{ "id": number, "ok": true, "result": any }` or `{ "id": number, "ok": false, "error": string }`

Stream push: `window.__optimusStream(id, event)` (native)  
HTTP stream: `POST /api/chat/stream` SSE with the same event shapes.

## Registry methods (`ipc/router.rs`)

### System
`ping`, `doctor`, `auth_status`, `auth_import_hermes`, `auth_import_cli`, `settings_get`, `settings_set`

### Sessions
`sessions`, `new_session`, `get_session`, `delete_session`, `rename_session`

### Scheduling
`cron_list`, `cron_add`, `cron_tick`

### Runtime
`approvals_list`, `approvals_grant`, `jobs_list`, `campaign_list`, `campaign_create`, `campaign_run`, `campaign_status`, `term_run`, `browser_navigate`, `browser_click`, `browser_reload`

**Note:** On Wry, navigate/reload may hit PreviewEmbed first. Agent tools use the same names via host workers/HTTP.

### Files
`fs_roots`, `fs_list`, `fs_read`, `artifacts_list`, `artifacts_put_text`, `artifacts_get`, `artifacts_delete`, `artifacts_delete_many`

### Chat
`chat`, `chat_offline`

### OS / window
`window_minimize`, `window_maximize`, `window_close`, `window_drag`, `window_outer_position`, `window_set_outer_position`, `pick_folder`, `open_path`, `open_url`

## Non-registry stream / embed methods

| Method | Role |
|---|---|
| `chat_stream` | Streaming chat |
| `chat_cancel` | Cancel stream by id |
| `browser_embed` | Preview bounds/z-order |
| `browser_back` / `browser_forward` | Preview history |
| `browser_set_annotate` | Annotation mode |
| `browser_annotation` | Pin push from preview |

## Host HTTP surface (Electron / Playwright)

| Route | Role |
|---|---|
| `GET /` | UI HTML (legacy assembled) or SPA static (later) |
| `GET /api/health` | Liveness |
| `POST /api/ipc` | JSON IPC (Bearer + CSRF) |
| `POST /api/chat/stream` | SSE chat stream |

Security: `OPTIMUS_HTTP_TOKEN` ≥ 32 chars, development/host flag, loopback only.
