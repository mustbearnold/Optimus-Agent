# ADR-0014: Native WebView IPC must not use fetch HTTP mode

## Status

Accepted — 2026-07-19

## Context

Optimus desktop serves the UI via wry custom protocol at `http://optimus.localhost/`.
The JS bridge treated *any* `http:`/`https:` page as "HTTP mode" and called `fetch('/api/ipc')`.

That path hits the **asset** custom-protocol handler (`asset /api/ipc`), not the Kernel IPC
handler. Result: UI looked fine (static shell) but every action hung or failed — auth
appeared stuck historically, composer send did nothing, sessions never loaded.

Playwright `--http 127.0.0.1:PORT` still legitimately needs fetch IPC.

## Decision

`isHttpMode()` is true only when:

- `window.__OPTIMUS_HTTP_MODE__ === true` (set by HTTP server inject), or
- hostname is `127.0.0.1`/`localhost` **and** a non-empty port is present

Native `optimus.localhost` uses `window.ipc` / `chrome.webview.postMessage`.

## Consequences

- Native UI works with CUA-verified live Codex chat
- Playwright HTTP suite unchanged
- Never use bare `location.protocol === 'http:'` for mode detection on custom protocols
