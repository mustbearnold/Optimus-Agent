# Phase 20 CDP Preview Browser — evidence

**Date:** 2026-07-23  
**Milestone closed:** P20D packaging + gates (after P20A–C)

## What shipped

| Slice | Status |
|---|---|
| P20A `optimus-browser` CDP crate | Done |
| P20B Kernel `BrowserEffector` + CDP/HTTP fallback | Done |
| P20C Desktop browser chrome + annotations → composer | Done |
| P20D Doctor flag + CI-safe gates + evidence | Done (this note) |

## Doctor

- `doctor.preview_browser` is `true` when `optimus_kernel::chrome_binary_path()` finds Chromium/Chrome (PATH, `OPTIMUS_CHROME_PATH`, or Playwright cache).
- `doctor.browser` is `"cdp"` or `"http-ssrf-safe"`.
- Unit test: `ipc::system::tests::doctor_preview_browser_matches_chrome_detection`.

## Gating

- Live CDP unit test in `optimus-browser` remains behind `cdp_live_tests` feature (ignored by default).
- Kernel `browser_live` integration runs against the real network when Chrome is present; CDP tool JSON keeps summary fields (`final_url`, `page_title`, `text`) visible in truncated tool traces.
- Chromium keeps its OS sandbox enabled. Top-level navigation and every intercepted
  HTTP(S) request fail closed on local, link-local, private, metadata, reserved,
  or unresolved destinations; `file:` navigation is rejected.
- Browser profile state is canonicalized under the workspace `.optimus` directory,
  and symlinked state roots/profile components are rejected.

## Validation run (this machine)

```text
cargo test -p optimus-kernel --lib chrome_binary_path
  → 2 passed

cargo test -p optimus-desktop doctor_preview_browser
  → 1 passed

cargo test -p optimus-kernel --test browser_live
  → 1 passed (CDP effector on example.com)

cargo test -p optimus-browser --features cdp_live_tests live_navigate_and_screenshot
  → 1 passed (sandboxed Chromium with request interception, 3.15s)

cargo test --workspace
  → all suites green (0 FAILED)

npx playwright test e2e/06-preview-browser.spec.js
  → 7 passed (15.2s)
```

## Files touched for P20D closeout

- `docs/specifications/phase-20d-packaging-gates.md`
- `crates/optimus-kernel/src/browser.rs` — CDP tool JSON summary fields; chrome path env tests
- `apps/optimus-desktop/src/ipc/system.rs` — doctor.preview_browser unit test
- `apps/optimus-desktop/Cargo.toml` — tempfile dev-dep for doctor test
- `docs/evidence/phase-20-cdp-browser-2026-07-23.md` (this file)

## Known limits

- Full-page screenshot base64 is still large in model tool payloads (trace summary is safe; model context may still need a later truncation policy).
- Element count on example.com may be 0 under headless viewport filters; acceptance uses title/URL presence.
- Native live WebView paint is not proven by HTTP Playwright; see e2e suite comments.
