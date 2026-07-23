---
knowledge_type: specification
status: draft
covers:
  - crates/optimus-kernel/src/browser.rs
  - crates/optimus-kernel/src/lib.rs
  - crates/optimus-packs/src/lib.rs
  - crates/optimus-browser/src/lib.rs
depends_on:
  - docs/specifications/phase-20-cdp-preview-browser.md
  - crates/optimus-browser/Cargo.toml
validated_by:
  - crates/optimus-kernel/tests/kernel_turn.rs
  - crates/optimus-packs/tests/packs_budget.rs
  - cargo test --workspace
---

# P20B — Kernel CDP Browser Integration

**Date:** 2026-07-22
**Prerequisite:** P20A — `crates/optimus-browser` crate exists with `CdpBrowserSession`
**Goal:** Wire the CDP browser crate into the kernel so `browser_navigate`, `browser_click`, `browser_snapshot` tools use CDP when Chrome is available, falling back to the existing HTTP text effector.

## Current state

The kernel at `crates/optimus-kernel/src/browser.rs` has an **HTTP-only browser effector**:

- `BrowserSession` struct with `navigate`, `snapshot`, `click` methods
- Uses `ureq` for HTTP fetch + regex-based HTML parsing
- SSRF-gated (no localhost/private IPs)
- Returns text-only page snapshots
- Tool dispatch in `lib.rs ~L1420-1463` directly uses `BrowserSession`

P20A created `crates/optimus-browser` with `CdpBrowserSession` that supports:
- Real Chromium navigation, JavaScript rendering
- Viewport screenshots (base64 PNG)
- DOM snapshots with interactable-element bounding boxes
- SOM-indexed click interaction

## Target state

```rust
// Kernel dispatch at ~L1420 becomes:
pub trait BrowserEffector {
    fn navigate(&mut self, url: &str) -> Result<BrowserPageJson>;
    fn snapshot(&self) -> Result<BrowserPageJson>;
    fn click(&mut self, index: usize) -> Result<BrowserPageJson>;
    fn close(&mut self) -> Result<()>;
}

struct HttpBrowserEffector(BrowserSession);   // existing
struct CdpBrowserEffector(CdpBrowserSession); // new

// On turn start: try CDP, fall back to HTTP
let effector: Box<dyn BrowserEffector> = try_cdp(workspace)?.unwrap_or_else(|| http(workspace));
```

## Sub-tasks (ordered)

### P20B-1: Add `BrowserEffector` trait

In `crates/optimus-kernel/src/browser.rs`, add:

```rust
pub trait BrowserEffector: Send {
    fn navigate(&mut self, url: &str) -> Result<String>;
    fn snapshot(&self) -> Result<String>;
    fn click(&mut self, index: usize) -> Result<String>;
    fn close(&mut self) -> Result<()>;
}
```

The return type is `String` (JSON serialized) matching the existing `page_to_tool_json()` pattern so the kernel's tool dispatch doesn't need to change.

### P20B-2: Implement `HttpBrowserEffector` wrapping existing `BrowserSession`

```rust
pub struct HttpBrowserEffector(BrowserSession);
impl BrowserEffector for HttpBrowserEffector { ... }
```

Transfers the existing `BrowserSession::open` + all methods unchanged.

### P20B-3: Implement `CdpBrowserEffector` wrapping `CdpBrowserSession`

```rust
pub struct CdpBrowserEffector(CdpBrowserSession);
impl BrowserEffector for CdpBrowserEffector {
    fn navigate(&mut self, url: &str) -> Result<String> {
        let state = self.0.navigate(url)?;
        Ok(serde_json::to_string(&state)?)
    }
    fn snapshot(&self) -> Result<String> {
        let cap = self.0.som_capture()?;
        Ok(serde_json::to_string(&cap)?)
    }
    fn click(&mut self, index: usize) -> Result<String> {
        let state = self.0.click(index)?;
        Ok(serde_json::to_string(&state)?)
    }
}
```

**Note:** The CDP snapshot returns `SomCapture` (screenshot + elements), not the old HTTP page structure. The model spec, UI, and downstream consumers need different handling — but for P20B the key is the kernel tool dispatch doesn't break. We treat the snapshot JSON as opaque for now and adjust consumers in P20C.

### P20B-4: Update kernel dispatch to try CDP, fall back to HTTP

In `crates/optimus-kernel/src/lib.rs`, the `ToolInvocation::Browser*` match arms currently:

```rust
let mut browser = BrowserSession::open(&self.workspace)?;
```

Change to:

```rust
let effector_result = try_cdp_effector(&self.workspace);
let mut browser: Box<dyn BrowserEffector> = match effector_result {
    Ok(e) => e,
    Err(_) => Box::new(HttpBrowserEffector(BrowserSession::open(&self.workspace)?)),
};
```

Where `try_cdp_effector` attempts:
1. Check if `chromium-browser` or `chromium` or `chrome` is on PATH
2. If yes, launch `CdpBrowserSession`
3. If no, return error → fall back to HTTP

The effector is reused across the turn's browser calls so the same session stays alive.

### P20B-5: Add CdpDetection helper

A simple module-level function:

```rust
fn has_chrome() -> bool {
    which::which("chromium-browser").is_ok()
        || which::which("chromium").is_ok()
        || which::which("google-chrome").is_ok()
        || which::which("chrome").is_ok()
}
```

Use `which` crate from the workspace. If not available, just check PATH manually.

### P20B-6: Add `optimus-browser` and `which` deps to `optimus-kernel`

In `crates/optimus-kernel/Cargo.toml`:

```toml
optimus-browser = { path = "../../crates/optimus-browser" }
which = "7"
```

### P20B-7: Make `CdpBrowserSession` `Send`

The trait requires `Send`. `CdpBrowserSession` uses `Arc<Browser>` and `Arc<Tab>` — these are already `Send + Sync` in headless_chrome. Verify with a compile check.

### P20B-8: Ensure `browser_state.json` persistence

The CDP effector saves its state file (`.optimus/browser_state.json` in workspace). The `close()` method on the trait must be called:
- When drop occurs (already in `Drop` impl)
- The effector `Box<dyn BrowserEffector>` is stored in a local and dropped at the end of tool dispatch

No explicit cleanup needed — the `Drop` impl handles it.

### P20B-9: Verify workspace tests pass

```bash
cargo check --workspace
cargo test --workspace
```

Existing pack budget tests must still pass. Existing kernel turn tests must still pass.

## Out of scope

- Desktop browser tab UI (P20C)
- SOM overlay rendering in the snapshot tool output (P20C)
- Updating model tool descriptors for CDP output shape (P20C)
- Multi-tab management
- CDP connection resilience / reconnect
- Chrome binary download management

## Rollback

If `optimus-browser` dependency causes build issues on this machine:
1. Remove `optimus-browser` dep from `optimus-kernel`
2. The `try_cdp_effector` always returns None → falls back to HTTP
3. Existing HTTP browser effector works unchanged
