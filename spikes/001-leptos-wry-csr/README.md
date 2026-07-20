# Spike 001: Leptos CSR inside the Optimus desktop architecture

## Question

**Given** the existing Wry desktop backend and `window.optimus` IPC bridge, **when** a Leptos 0.8 CSR/WASM client is mounted, **then** can it render a compact Rome-style shell and complete a real `doctor` plus offline `chat` IPC round trip without changing production code?

## Approach

- Standalone Cargo workspace: does not modify the production workspace.
- Leptos `0.8.20` with the `csr` feature (latest stable exercised; `0.9.0-beta` was intentionally avoided).
- Trunk `0.21.14` for WASM build/dev serving.
- The component calls `window.optimus.invoke`, matching the existing native Wry bridge.
- `bridge.js` is only a Trunk adapter that forwards that same API to `/api/ipc`.
- Optimus HTTP mode runs on `127.0.0.1:4317`; Trunk runs on `127.0.0.1:4318` and proxies `/api/`.

## Run

```text
cargo build -p optimus-desktop
optimus-desktop.exe --http 4317 --home <temporary-home>
trunk serve
```

Open `http://127.0.0.1:4318`, verify `IPC online`, then submit a message and verify `offline echo: <message>`.

## Verdict: PARTIAL

### What worked

- `trunk build --release` produced a real optimized Leptos/WASM distribution on Windows.
- The Leptos component mounted with no browser console or WASM errors.
- `window.optimus.invoke("doctor", ...)` completed through Trunk's `/api/` proxy to the existing Optimus HTTP backend.
- Two sequential offline chat turns completed and rendered (`offline echo: leptos-scroll-one` and `offline echo: leptos-scroll-two`), with the Optimus session ID reused by Leptos state.
- Reactive busy/input/message/status state worked without JavaScript application logic.
- Message auto-scroll was measured after two turns: `scrollTop=68`; the final bubble ended at y=396 and the composer began at y=437, so it was not obscured.
- The compact Rome-style titlebar, rail, conversation, composer, and status bar rendered coherently at 1258×622.
- Optimized distribution size: **261,465 bytes total**, including **217,025 bytes WASM**, **38,425 bytes loader JS**, **4,157 bytes CSS**, and small HTML/bridge files.

### What did not yet run

- Production native Wry currently serves one self-contained HTML document and no external JS/WASM assets. This spike exercised the real backend through HTTP mode, not a packaged native executable with the Leptos distribution embedded.
- Header/rail controls are visual placeholders; only `doctor` and offline chat are wired.
- Existing Playwright parity coverage has not been ported to Leptos components.

### Surprises

- `serde_wasm_bindgen::to_value` encoded JSON maps as JavaScript `Map`; `JSON.stringify` reduced IPC params to `{}`. `Serializer::json_compatible()` fixed the boundary and is required for this bridge contract.
- The stable `0.8` line resolved to Leptos `0.8.20`; the registry also exposes `0.9.0-beta`, which was intentionally avoided.
- The optimized payload is small enough for a desktop WebView, but it is materially larger than the current inline JavaScript shell and needs explicit packaging/cache behavior.

### Recommendation for the real build

Proceed incrementally rather than rewriting the entire shell at once:

1. Add a production asset bundle abstraction that embeds and serves Trunk's generated HTML, JS, WASM, and CSS through Wry's custom protocol and HTTP mode from one manifest.
2. Keep the existing Rust IPC/router/backend unchanged; expose it to Leptos exclusively through the existing `window.optimus` contract.
3. Port one bounded vertical slice first (the conversation transcript + composer), preserving DOM contracts needed by Playwright.
4. Run exact behavior/visual parity before migrating rail, projects/sessions, capabilities, files, terminal, and native window controls.
5. Retain the current frontend as a rollback path until native packaging and all parity tests pass.
