---
doc_id: contracts-desktop-ipc-methods
doc_type: reference
plane: current
status: current
authority: supporting
summary: Cross-surface allowlist matrix (Rust registry vs Electron vs React): desktop-shell-and-ipc-matrix.md and python3 scripts/check-desktop-ipc-matrix.py.
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: contract
covers:
  - crates/optimus-host/src/router.rs
  - apps/optimus-desktop/src/main.rs
  - apps/optimus-desktop/src/bridge.rs
  - apps/optimus-desktop/src/native_workers.rs
  - apps/optimus-desktop/src/preview_embed.rs
  - apps/optimus-electron/main.cjs
  - apps/optimus-electron/preload.cjs
  - apps/optimus-ui/src/ipc/**
---

# Desktop IPC method inventory (frozen for shell migration)

Cross-surface allowlist matrix (Rust registry vs Electron vs React):
[desktop-shell-and-ipc-matrix.md](./desktop-shell-and-ipc-matrix.md) and
`python3 scripts/check-desktop-ipc-matrix.py`.

Wire request: `{ "id": number, "method": string, "params": object }`  
Wire reply: `{ "id": number, "ok": true, "result": any }` or `{ "id": number, "ok": false, "error": string }`

Stream push: `window.__optimusStream(id, event)` (native)  
HTTP stream: `POST /api/chat/stream` SSE with the same event shapes.

## Registry methods (`ipc/router.rs`)

### System
`ping`, `doctor`, `auth_status`, `auth_import_hermes`, `auth_import_cli`, `settings_get`, `settings_set`

### Sessions
`sessions`, `new_session`, `get_session`, `delete_session`, `rename_session`,
`session_search`, `archive_session`, `pin_session`

Cron: `cron_list`, `cron_add`, `cron_tick`, `cron_set_enabled`, `cron_remove`,
`cron_history`

Artifacts: `artifacts_list`, `artifacts_put_text`, `artifacts_get`,
`artifacts_delete`, `artifacts_delete_many`, `artifacts_export`,
`artifacts_export_zip`

Consoles (program P26): `skills_list`, `skills_pin`, `skills_deprecate`,
`memory_list`, `memory_recall`, `memory_correct`, `memory_forget`,
`packs_state`, `packs_activate`, `packs_deactivate`, `logs_tail`,
`commands_list`

Messaging (program P28): `gateway_status`, `gateway_inbox`, `gateway_outbox`,
`gateway_enqueue`, `gateway_ambiguous`, `gateway_ack_delivery`,
`gateway_telegram_status`

Extensibility (program P27): `providers_catalog`, `providers_route_preview`,
`mcp_status`, `mcp_tools`, `packs_verify_signed`

`get_session` returns presentation-safe user/assistant messages. Provider
tool-call arrays and tool-result protocol messages are omitted; ordered durable
`tool_events` are attached to their owning assistant turn. `run_status` reports
the latest durable turn state.

### Scheduling
`cron_list`, `cron_add`, `cron_tick`

`cron_add.provider` accepts a concrete provider catalog wire id and persists its
canonical runtime identity. The legacy React spelling `openai_compat` is
accepted only as migration input, normalized on new writes, and normalized
before routing older persisted schedules.

### Runtime
`approvals_list`, `approvals_grant`, `jobs_list`, `campaign_list`, `campaign_create`, `campaign_run`, `campaign_status`, `term_run`, `browser_navigate`, `browser_click`, `browser_reload`

**Note:** On Wry, navigate/reload may hit PreviewEmbed first. Agent tools use the same names via host workers/HTTP.

### Files
`fs_roots`, `fs_list`, `fs_read`, `project_scopes_list`,
`project_scopes_authorize`, `artifacts_list`, `artifacts_put_text`,
`artifacts_get`, `artifacts_delete`, `artifacts_delete_many`

### Chat
`chat`, `chat_offline`, `chat_approval_resolve`

`ChatRequest.provider` accepts the catalog wire ids `offline`, `codex`, and
`open-ai-compat` plus `auto`. `auto` is a selector evaluated by the Rust router
at turn start, while the returned result and durable route decision name the
concrete provider/model used. An omitted model means the selected provider's
default; the literal model id `auto` is not sent. Explicit model ids reach the
canonical router unchanged and must be owned by the requested provider.

`chat_approval_resolve` accepts a session-owned approval decision only when the
request repeats the pending event's `run_id`, `call_id`, `job_id`, `node_id`,
`node_index`, and `effect_sha256`. Approve executes that exact persisted effect;
deny executes nothing. Both paths durably terminalize the tool event, assistant
receipt, turn, and execution manifest before returning a presentation-safe
summary. The caller must reload `get_session` for the canonical projection.

### OS / window
`window_minimize`, `window_maximize`, `window_close`, `window_drag`, `window_outer_position`, `window_set_outer_position`, `pick_folder`, `open_path`, `open_url`

`pick_folder` stages the native selection as a short-lived, single-use project
root grant and returns its opaque token. Electron main exchanges the native
path through internal `project_root_stage_native`; that method is not in the
renderer `invoke` allowlist and requires a separate random main-process secret
that is not sent to renderer code.

## Chat tool lifecycle

Chat streams carry versioned `tool` events with stable `event_id`, `run_id`,
and `call_id`, canonical `tool_id`, an explicit lifecycle `phase`, bounded
summary, optional duration, and optional validated terminal `outcome`. The
phase set is `started`, `approval_required`, `succeeded`, `failed`, `cancelled`,
`suppressed`, and `ambiguous`. These events are persisted before delivery and
may be replayed by `get_session`; consumers deduplicate by `event_id`.
`approval_required` also carries the exact approval binding needed by
`chat_approval_resolve`. Terminal events may retain that binding as audit
evidence, but the UI exposes controls only for the pending phase.

## Non-registry stream / embed methods

| Method | Role |
|---|---|
| `chat_stream` | Streaming chat |
| `chat_cancel` | Cancel stream by id |
| `browser_embed` | Preview bounds/z-order |
| `browser_back` / `browser_forward` | Preview history |
| `browser_set_annotate` | Annotation mode |
| `browser_annotation` | Pin push from preview |

## Host HTTP surface (Electron main / Playwright)

| Route | Role |
|---|---|
| `GET /` | Legacy assembled UI HTML and development/test harness |
| `GET /api/health` | Liveness |
| `POST /api/ipc` | JSON IPC (Bearer + CSRF) |
| `POST /api/chat/stream` | SSE chat stream |

Security: `OPTIMUS_HTTP_TOKEN` ≥ 32 chars, development/host flag, loopback only.

## Electron React bridge

**Confirmed current behaviour:** the production React renderer does not call
the host routes directly and does not receive `OPTIMUS_HTTP_TOKEN`. Electron
main authenticates the same frozen host calls and the preload exposes:

```ts
type OptimusElectronBridge = {
  invoke<T>(method: DesktopMethod, params?: Record<string, unknown>): Promise<T>;
  chat: {
    start(request: ChatRequest): Promise<{ streamId: number }>;
    cancel(streamId: number): Promise<{ requested: boolean }>;
    subscribe(listener: (event: ChatEnvelope) => void): () => void;
  };
  browser: {
    setBounds(bounds: BrowserBounds): void;
    setVisible(visible: boolean): void;
    navigate(url: string): Promise<BrowserState>;
    back(): Promise<BrowserState>;
    forward(): Promise<BrowserState>;
    reload(): Promise<BrowserState>;
    state(): Promise<BrowserState>;
    annotate(): Promise<BrowserAnnotation>;
    cancelAnnotation(): Promise<{ cancelled: boolean }>;
    subscribe(listener: (state: BrowserState) => void): () => void;
  };
  windowAction(action: "minimize" | "maximize" | "close"): Promise<unknown>;
  pickFolder(): Promise<PickFolderResult>;
  openPath(path: string): Promise<unknown>;
};
```

The bridge allowlists existing desktop method names, rejects serialized
requests larger than 1 MiB, and permits one foreground chat stream per window.
Chat envelopes carry both `streamId` and session ID. `hostInfo` is retained for
legacy compatibility but omits the token in React mode.

The Electron Browser methods control a user-facing `WebContentsView`; they are
not aliases for Rust `browser_navigate`, `browser_click`, or
`browser_reload`.

`annotate()` is an explicit one-shot user-preview capability. Main injects a
temporary capture into the sandboxed page and returns at most bounded URL,
title, tag, role, accessible label/short text, and rounded geometry. It
consumes the selected click, times out after two minutes, and is cancelled by
Escape, `cancelAnnotation()`, or preview suspension. No page HTML or selector
crosses the preload boundary. Settings, project-source management, and task
overlays set the preview invisible before their renderer UI is shown.
