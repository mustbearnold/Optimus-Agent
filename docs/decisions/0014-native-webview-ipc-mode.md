---
doc_id: decisions-0014-native-webview-ipc-mode
doc_type: decision
plane: decision
status: current
authority: record
summary: Decision record for ADR-0014: Native WebView IPC must not use fetch HTTP mode, including its context, consequences, and current documentary status.
reviewed_on: 2026-07-31
review_by: 2026-10-31
---

# ADR-0014: Native WebView IPC must not use fetch HTTP mode

## Status

Accepted — 2026-07-19

## Context

Optimus desktop serves the UI through a Wry custom protocol. Wry exposes that
protocol as `http://optimus.localhost/` on Windows WebView2 and Android, but as
`optimus://localhost/` on Linux WebKitGTK, macOS WebKit, and iOS WKWebView.
The JS bridge treated *any* `http:`/`https:` page as "HTTP mode" and called `fetch('/api/ipc')`.

That path hits the **asset** custom-protocol handler (`asset /api/ipc`), not the Kernel IPC
handler. Result: UI looked fine (static shell) but every action hung or failed — auth
appeared stuck historically, composer send did nothing, sessions never loaded.

Playwright `--http 127.0.0.1:PORT` still legitimately needs fetch IPC.

## Decision

`isHttpMode()` is true only when:

- `window.__OPTIMUS_HTTP_MODE__ === true` (set by HTTP server inject), or
- hostname is `127.0.0.1`/`localhost` **and** a non-empty port is present

Native custom-protocol pages use `window.ipc` / `chrome.webview.postMessage`.
The Rust shell selects the platform URL at compile time; never navigate
WebKitGTK to the WebView2-only `.localhost` HTTP form.

## Consequences

- Native UI works with CUA-verified live Codex chat
- Playwright HTTP suite unchanged
- Never use bare `location.protocol === 'http:'` for mode detection on custom protocols
- Keep a platform URL unit test because browser HTTP tests cannot detect a broken native custom-protocol navigation
