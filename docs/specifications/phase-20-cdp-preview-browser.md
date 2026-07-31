---
doc_id: specifications-phase-20-cdp-preview-browser
doc_type: history
plane: history
status: historical
authority: historical
summary: Date: 2026-07-22 Priority: P1 — fills SOTA scorecard's #1 product loss Builds on: Phase 18/19 sidebars, FS, terminal, and HTTP browser effector
reviewed_on: 2026-07-31
review_by: never
knowledge_type: specification
covers:
  - Cargo.toml
  - crates/optimus-browser/src/**
  - crates/optimus-packs/src/**
  - crates/optimus-kernel/src/browser.rs
  - crates/optimus-kernel/src/lib.rs
  - apps/optimus-desktop/src/**
  - apps/optimus-desktop/ui/**
depends_on:
  - docs/architecture/system-overview.md
  - docs/contracts/high-risk-contracts.md
validated_by:
  - crates/optimus-browser/src/lib.rs
  - crates/optimus-packs/tests/packs_budget.rs
  - crates/optimus-kernel/tests/kernel_turn.rs
  - apps/optimus-desktop/e2e/**
---

# Phase 20 — CDP Preview Browser

**Date:** 2026-07-22
**Priority:** P1 — fills SOTA scorecard's #1 product loss
**Builds on:** Phase 18/19 sidebars, FS, terminal, and HTTP browser effector

## Status before Phase 20

The existing `crates/optimus-kernel/src/browser.rs` is an **HTTP text effector**:

- Fetches HTML, extracts text + links (regex/AST-less)
- SSRF-gated (no localhost/private IPs)
- No JavaScript, no screenshots, no interactive DOM
- Desktop tab at `ui/index.html:291` is a stub: *"Codex-class preview ships in P11"*

Hermes has CDP-based multi-tab browser automation with SOM overlays and interactive elements. This is SOTA scorecard loss #1 ("Shared-session CDP Preview Browser").

## Phase 20 goal

Ship a `crates/optimus-browser` crate that wraps the **headless_chrome** crate for CDP WebSocket control, and wire it through the existing Optimus packs/kernel/desktop layers so that:

1. `browser_navigate`, `browser_click`, `browser_snapshot` tools drive a real CDP browser
2. The desktop Browser tab shows a live preview frame (screenshots + interaction)
3. The existing HTTP text effector stays as fallback when CDP is unavailable

## Architecture

```text
crates/optimus-browser/         ← NEW crate
  └── src/
      ├── lib.rs                 ← CdpBrowserSession (replaces BrowserSession when CDP available)
      ├── session.rs             ← Tab management, headless_chrome lifecycle
      ├── screenshot.rs          ← DOM snapshot + SOM overlay rendering
      └── som.rs                 ← Set-of-Mark: numbered overlays over elements

crates/optimus-kernel/src/browser.rs  ← extended: CdpBrowserSession | HttpBrowserSession
crates/optimus-packs/src/lib.rs       ← Browser pack gains CDP status flag
apps/optimus-desktop/
  ├── src/ipc/browser.rs              ← IPC bridge for browser tab
  └── ui/index.html                   ← rpBrowser panel becomes live frame
```

## Sub-tasks (ordered)

### P20A — Crates and integration (spike day)

| # | What | Acceptance |
|---|---|---|
| 1 | Add `headless_chrome` dep to workspace `Cargo.toml` | `cargo check` passes |
| 2 | Create `crates/optimus-browser/` with `Cargo.toml` + `src/lib.rs` | crate builds |
| 3 | `CdpBrowserSession::new(headless: bool)` — launch or connect to Chrome via CDP | connects to `/usr/bin/chromium-browser --remote-debugging-port=9222` |
| 4 | `session.navigate(url)` — open page, wait for load, return title + URL | returns expected page info |
| 5 | `session.screenshot()` — capture PNG bytes of viewport | returns base64 PNG |
| 6 | `session.dom_snapshot()` — get element tree with bounding boxes | returns structured element list |
| 7 | `session.som_capture()` — screenshot + DOM + numbered overlays | combined output |
| 8 | `session.click(index)` — click element by SOM index, wait for navigation | page updates |
| 9 | `session.close()` — close tab | no resource leak |

### P20B — Kernel integration

| # | What | Acceptance |
|---|---|---|
| 10 | Refactor `BrowserSession` in `optimus-kernel` to abstract over HTTP/CDP | both backends compile |
| 11 | `browser_navigate` dispatches to CDP when available, else HTTP | turn test passes both paths |
| 12 | `browser_click` dispatches to CDP click | tool outcome contains new page state |
| 13 | `browser_snapshot` returns SOM-annotated page (screenshot + elements + numbered overlays) vs the current text-only | kernel test verifies SOM output shape |
| 14 | Browser-session state persists across turns | last session survives kernel restart |

### P20C — Desktop Browser tab

| # | What | Acceptance |
|---|---|---|
| 15 | Desktop IPC method `browser_navigate(url)` → returns screenshot + element tree | playwrite test |
| 16 | `rpBrowser` panel replaces stub with: URL bar + rendered screenshot + numbered element overlay + click-through | playwrite test |
| 17 | Clicking SOM element in the panel sends `browser_click(index)` to kernel | turn observes the click |
| 18 | Files tab still works alongside browser tab | desktop-1 tests pass |

### P20D — Packaging + gates

| # | What | Acceptance |
|---|---|---|
| 19 | `doctor.preview_browser` reflects Chromium presence | `doctor` output changes |
| 20 | Browser tests are gated behind `#[cfg(not(ci_no_chrome))]` or env-var-gated | CI without Chrome still passes |
| 21 | Full cargo test gate passes | `cargo test --workspace` green |
| 22 | Desktop Playwright gate passes | `npx playwright test` green |

## Integration points

### Kernel dispatch (`crates/optimus-kernel/src/lib.rs`)

Currently at `~L1420` — the `ToolInvocation::Browser*` match arms create `BrowserSession::open`. Change to:

```rust
ToolInvocation::BrowserNavigate => {
    let browser: Box<dyn BrowserEffector> = if has_cdp() {
        Box::new(CdpBrowserSession::open(&self.workspace)?)
    } else {
        Box::new(HttpBrowserSession::open(&self.workspace)?)
    };
    // ... dispatch
}
```

The `BrowserEffector` trait lives in `crates/optimus-kernel/src/browser.rs` for now (can extract to `optimus-browser` crate later):

```rust
pub trait BrowserEffector {
    fn navigate(&mut self, url: &str) -> Result<BrowserPage>;
    fn snapshot(&self) -> Result<BrowserPage>;
    fn click(&mut self, index: usize) -> Result<BrowserPage>;
    fn close(&mut self) -> Result<()>;
}
```

### Desktop IPC

The desktop needs a new IPC method `browser_navigate` that returns:
```json
{
  "ok": true,
  "screenshot_b64": "iVBOR...",
  "elements": [
    {"index": 1, "tag": "button", "text": "Submit", "bounds": [10, 20, 100, 40]}
  ],
  "som_url": "data:image/png;base64,..."
}
```

### Packs

The Browser pack currently has `ToolPolicy::NetworkRead` and `ToolInvocation::{BrowserNavigate, BrowserSnapshot, BrowserClick}`. No change needed to the pack schema itself — just the backend changes.

### Desktop UI

Replace `ui/index.html` lines 291-298 (the stub) with:

```html
<div class="rp-panel" id="rpBrowser" data-tab="browser">
  <div class="pane-head">
    <div class="section-label">Preview browser</div>
    <input type="text" id="browserUrl" placeholder="https://..." />
    <button id="browserGo">Go</button>
  </div>
  <div id="browserViewport">
    <!-- SOM canvas rendered here -->
  </div>
</div>
```

The JS bridge attaches `__optimusBrowser` handlers that call the native IPC `browser_navigate`, `browser_click`, etc.

### Live embed resize/redraw contract

The current desktop uses a persistent native child WebView over
`#browserLiveHole`. While the user drags either horizontal divider, or while
window/viewport resize events are active, changed geometry is dispatched in the
same JavaScript input turn and sampled again at `requestAnimationFrame` cadence.
This uses the compositor's maximum presentation cadence instead of an off-frame
fixed timer. Geometry is rounded to the nearest CSS pixel; one-pixel divider
movement therefore remains one-pixel native movement rather than 2 px stepping.

At most one native embed update may be in flight. If GTK/WebKit is still
applying that update, intermediate geometry is discarded and replaced by one
pending “latest bounds” marker; completion recomputes from the live DOM and
sends only that newest rectangle. Unchanged geometry is a no-op, including
while a divider remains held. This keeps the browser at the divider without
duplicate native work or a stale replay queue.

The native `PreviewEmbed` owns visibility and geometry as separate state
transitions. Hidden-to-visible performs one GTK restack. Visible-to-visible
bounds changes use only persistent fixed-child geometry plus Wry `set_bounds`;
they do not remove/re-add, hide/show, queue draw, or remap the WebView. Window
z-order reassertion raises the existing native child without reparenting it.

Both sidebars reserve a 10 px resize gutter and expose the same 7 px divider.
The right Browser hole begins after that gutter, leaving a protected gap between
the hit target and native WebView bounds. The handle and browser therefore use
disjoint horizontal rectangles: the raised WebKit child cannot cover or
intercept the divider, and Files, Artifacts, and Browser retain stable DOM IDs.

The pulse stops on pointer up/cancel, or 160 ms after the final window resize
event, so idle CPU and IPC traffic return to the normal change-detected path.
Per-frame native logging is prohibited on this path. The direct contracts live
in `apps/optimus-desktop/e2e/06-preview-browser.spec.js`: fast-native tests lock
same-turn dispatch, display-frame changed-state delivery, pixel fidelity, and
no duplicate geometry; a slow-native test locks one-in-flight backpressure plus
latest-bounds convergence. Native package tests lock the move-only hot-path
transition and the embed boundary.

## Out of scope

- Multi-tab / multi-session browser management (single active tab for P20)
- JavaScript console injection or `page.evaluate()` (add in P21)
- SOM path or coordinate-based clicks (element-index only, matching Hermes CUA)
- Device emulation / viewport resizing
- Video recording of browser sessions
- CDP over WebSocket to a remote debug target (localhost only)
- Headless mode switch in the UI (CLI flag `--browser-headless` only)

## Rollback plan

If `headless_chrome` causes build issues (Chromium binary downloads, rustc compilation failures, native lib conflicts), fall back to:

1. Shell out to Chromium via `std::process::Command` with `--headless --screenshot` flags
2. Use a `serde_json` CDP-socket client (raw WebSocket + `json!()` calls, no crate dep)

The Rust CDP ecosystem (chromiumoxide, headless_chrome) is mature enough that this is unlikely, but the architecture must isolate the CDP backend behind `BrowserEffector` so we can swap.

## Verification

```bash
# Crate compiles
cargo check -p optimus-browser

# Full workspace
cargo test --workspace

# Desktop tests
cd apps/optimus-desktop
npx playwright test

# Doctor reports CDP availability
cargo run -p optimus-cli -- doctor
```

## Evidence location

- Baseline: `local/tmp/baselines/PF-00-report.md`
- Phase 20 evidence: `docs/evidence/phase-20-cdp-browser-{date}.md`
- SOM output test fixtures: `local/tmp/cua-evidence/`
