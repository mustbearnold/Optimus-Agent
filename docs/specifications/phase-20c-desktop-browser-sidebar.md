---
knowledge_type: specification
status: draft
covers:
  - apps/optimus-desktop/ui/index.html
  - apps/optimus-desktop/src/bridge.rs
  - apps/optimus-desktop/src/ipc/router.rs
  - apps/optimus-desktop/src/ipc/runtime_ops.rs
  - crates/optimus-kernel/src/browser.rs
  - crates/optimus-kernel/src/lib.rs
depends_on:
  - docs/specifications/phase-20b-kernel-cdp-integration.md
validated_by:
  - cargo check --workspace
  - cargo test --workspace
---

# P20C — Desktop Browser Sidebar

**Date:** 2026-07-22
**Prerequisite:** P20B — Kernel has `best_effector()` dispatching to CDP or HTTP
**Goal:** Replace the desktop Browser tab stub with a live preview panel that shows
the current browser page screenshot (CDP) or text (HTTP fallback), with a URL bar
and click-through support.

## Current state

The Browser tab at `ui/index.html:291-298` is a stub:

```html
<div id="browserStub">
  <p><strong>Codex-class preview</strong> ships in phase P11 (CDP engine).</p>
  <p>Status: <code>doctor.preview_browser = false</code></p>
</div>
```

The kernel's `best_effector()` now supports CDP (with screenshots + SOM elements)
and HTTP (text-only) backends, but the desktop has no way to drive them.

## Target state

The Browser tab becomes a live preview panel:

```
┌─────────────────────────────────┐
│  Preview browser                │
│  [https://example.com] [Go]     │
├─────────────────────────────────┤
│                                 │
│  [Screenshot or text preview]   │
│                                 │
│  [Element 1] Button: Submit     │
│  [Element 2] Link: About        │
│                                 │
│  Effector: cdp-browser           │
└─────────────────────────────────┘
```

## Implementation

### Sub-tasks

### P20C-1: Add `browser_navigate` IPC method

In `apps/optimus-desktop/src/ipc/router.rs`:

```rust
("browser_navigate", Domain::Runtime),
```

In `apps/optimus-desktop/src/ipc/runtime_ops.rs`:

```rust
"browser_navigate" => {
    let url = params.get("url").and_then(|v| v.as_str()).ok_or("browser_navigate requires url")?;
    let workspace = home.join("workspace");
    let effector = best_effector(&workspace).map_err(|e| e.to_string())?;
    let result = effector.navigate(url).map_err(|e| e.to_string())?;
    let _ = effector.close();
    Ok(serde_json::from_str(&result).unwrap_or(json!({"ok":false,"error":result})))
}
```

This mirrors how `term_run` works — one-shot: open effector, do one call, close.

### P20C-2: Update `ui/index.html` Browser panel

Replace the stub (lines 291-298) with:

```html
<div class="rp-panel" id="rpBrowser" data-tab="browser">
  <div class="pane-head">
    <div class="section-label">Preview browser</div>
    <input type="text" id="browserUrl" placeholder="https://..." />
    <button id="browserGo" type="button">Go</button>
  </div>
  <div id="browserViewport">
    <div id="browserStatus" class="browser-status">Enter a URL and press Go.</div>
    <div id="browserScreenshot" hidden></div>
    <div id="browserElements" class="browser-elements" hidden></div>
  </div>
</div>
```

### P20C-3: Add JS handlers in `bridge.rs`

Add these JS functions to the bridge:

```javascript
async function browserNavigate(url) {
  const result = await post('browser_navigate', { url: url });
  // Render the result
  const viewport = document.getElementById('browserViewport');
  const status = document.getElementById('browserStatus');
  const screenshot = document.getElementById('browserScreenshot');
  const elements = document.getElementById('browserElements');

  if (result.screenshot_b64) {
    status.textContent = result.effector + ' · ' + (result.title || '');
    screenshot.innerHTML = '<img src="data:image/png;base64,' + result.screenshot_b64 + '" style="max-width:100%"/>';
    screenshot.hidden = false;
    if (result.elements && result.elements.length > 0) {
      elements.innerHTML = result.elements.map(e =>
        '<div class="browser-elem" data-index="' + e.index + '">' +
        '<span class="elem-index">' + e.index + '</span> ' +
        '<span class="elem-tag">&lt;' + e.tag + '&gt;</span> ' +
        '<span class="elem-text">' + escapeHtml(e.text) + '</span>' +
        '</div>'
      ).join('');
      elements.hidden = false;
    } else {
      elements.hidden = true;
    }
  } else if (result.title !== undefined) {
    // HTTP text effector
    status.textContent = result.effector + ' · ' + (result.title || result.final_url || '');
    screenshot.innerHTML = '<pre class="browser-text">' + escapeHtml(result.text || '') + '</pre>';
    screenshot.hidden = false;
    if (result.links && result.links.length > 0) {
      elements.innerHTML = result.links.map(l =>
        '<div class="browser-elem" data-index="' + l.index + '">' +
        '<span class="elem-index">' + l.index + '</span> ' +
        '<span class="elem-text">' + escapeHtml(l.text) + '</span>' +
        ' <span class="elem-href">' + escapeHtml(l.href) + '</span>' +
        '</div>'
      ).join('');
      elements.hidden = false;
    } else {
      elements.hidden = true;
    }
  } else {
    status.textContent = 'Error: ' + (result.error || 'unknown');
  }
}

window.browserNavigate = browserNavigate;
```

Plus a `escapeHtml` helper and an event listener on the `Go` button.

### P20C-4: Wire Go button + URL bar

In the JS:

```javascript
document.getElementById('browserGo').addEventListener('click', function () {
  const url = document.getElementById('browserUrl').value.trim();
  if (!url) return;
  // Auto-prepend https:// if no scheme
  const fullUrl = url.startsWith('http://') || url.startsWith('https://') ? url : 'https://' + url;
  document.getElementById('browserStatus').textContent = 'Loading...';
  browserNavigate(fullUrl);
});

// Enter key triggers Go
document.getElementById('browserUrl').addEventListener('keydown', function (e) {
  if (e.key === 'Enter') document.getElementById('browserGo').click();
});
```

### P20C-5: Add CSS for browser panel

A few styles for the browser tab:

```css
#browserUrl { flex: 1; margin: 0 4px; padding: 2px 6px; border: 1px solid var(--border); background: var(--bg); color: var(--text); font-size: 12px; border-radius: 3px; }
.browser-status { padding: 8px; font-size: 11px; color: var(--text-3); }
.browser-elements { padding: 4px; max-height: 200px; overflow: auto; font-size: 11px; }
.browser-elem { padding: 2px 4px; cursor: pointer; border-bottom: 1px solid var(--bg-2); }
.browser-elem:hover { background: var(--bg-2); }
.elem-index { display: inline-block; width: 20px; text-align: right; margin-right: 6px; color: var(--accent); font-weight: bold; }
.elem-tag { color: #6a9fb5; margin-right: 4px; }
.elem-text { color: var(--text-2); }
.elem-href { color: var(--text-3); font-size: 10px; }
.browser-text { white-space: pre-wrap; word-break: break-word; font-size: 10px; line-height: 1.4; padding: 4px; margin: 0; color: var(--text-2); max-height: 300px; overflow: auto; }
#browserScreenshot img { border: 1px solid var(--border); border-radius: 2px; }
```

### P20C-6: Verify compilation + tests

```bash
cargo check --workspace
cargo test --workspace
```

## Out of scope

- Element click-through from the panel (clicking an element in the panel sends IPC)
  — we render elements but clicking them doesn't navigate yet. That's a follow-up.
- SOM overlay rendered on the screenshot itself (numbered boxes drawn over the image)
  — that's P20C.5 if we get to it.
- Multi-tab management
- CDP connection persistence across kernel turns

## Manual verification

1. `cargo run -p optimus-desktop -- --http 8787`
2. Open `http://127.0.0.1:8787`
3. Click Files button → Browser tab
4. Type `example.com` → press Go
5. See: screenshot (CDP) or text (HTTP fallback) + element listing
