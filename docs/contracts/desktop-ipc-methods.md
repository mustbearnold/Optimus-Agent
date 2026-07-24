---
knowledge_type: contract
status: current
covers:
  - apps/optimus-desktop/src/ipc/router.rs
  - apps/optimus-desktop/src/main.rs
  - apps/optimus-desktop/src/bridge.rs
  - apps/optimus-desktop/src/native_workers.rs
  - apps/optimus-desktop/src/preview_embed.rs
  - apps/optimus-electron/main.cjs
  - apps/optimus-electron/preload.cjs
  - apps/optimus-ui/src/ipc/**
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
