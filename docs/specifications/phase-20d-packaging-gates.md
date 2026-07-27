---
knowledge_type: specification
status: active
covers:
  - crates/optimus-host/src/system.rs
  - crates/optimus-kernel/src/browser.rs
  - crates/optimus-browser/src/lib.rs
  - apps/optimus-desktop/e2e/06-preview-browser.spec.js
depends_on:
  - docs/specifications/phase-20-cdp-preview-browser.md
  - docs/specifications/phase-20c-desktop-browser-sidebar.md
validated_by:
  - cargo test -p optimus-kernel --lib browser
  - cargo test --workspace
  - apps/optimus-desktop/e2e/06-preview-browser.spec.js
---

# P20D — Packaging + gates

**Date:** 2026-07-23  
**Goal:** Close Phase 20 with honest doctor reporting and CI-safe browser tests.

## Acceptance

| # | What | Done when |
|---|---|---|
| 19 | `doctor.preview_browser` reflects Chromium presence | `doctor_json` returns `preview_browser: bool` and `browser: "cdp"\|"http-ssrf-safe"` |
| 20 | Live CDP tests stay gated | `optimus-browser` live test uses `cdp_live_tests` feature / ignore by default |
| 21 | Workspace tests green without Chrome | `cargo test --workspace` passes |
| 22 | Preview browser Playwright suite green | `npx playwright test e2e/06-preview-browser.spec.js` |

## Scope (this slice)

1. Document P20D.
2. Add focused unit coverage for `chrome_binary_path` + doctor browser fields.
3. Run cargo workspace tests.
4. Run Playwright preview browser suite if deps available.
5. Write evidence under `docs/evidence/`.

## Out of scope

- Multi-tab CDP management
- Native live WebView paint proof beyond existing e2e contracts
- Enabling `cdp` feature by default in production builds
