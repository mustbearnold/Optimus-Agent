---
doc_id: architecture-product-complete-p23-verification
doc_type: history
plane: history
status: historical
authority: historical
summary: Planes: program P23 · delivery PR #33 · architecture hold (Security / UI) · ledger browser.cdp, browser.http, browser.annotations, web.search → parity
reviewed_on: 2026-07-31
review_by: never
knowledge_type: verification
owns:
  - docs/architecture/product-complete-p23-verification.md
covers:
  - docs/plans/product-complete-program.md
depends_on:
  - docs/decisions/0040-shared-browser-contract.md
  - docs/plans/product-complete-program.md
validated_by:
  - crates/optimus-kernel/src/browser_coord.rs
  - crates/optimus-kernel/src/browser.rs
  - crates/optimus-kernel/src/web_search.rs
  - apps/optimus-ui/src/components/workspace/BrowserSurface.test.tsx
---

# Product-complete program P23 verification

Planes: **program P23** · delivery **PR #33** · architecture hold (Security / UI) ·
ledger `browser.cdp`, `browser.http`, `browser.annotations`, `web.search` → **parity**

Date: 2026-07-25

## Goal

Coordinated preview + agent browser under SharedBrowserContract (not shared CDP
session); web search extract schema + provenance; annotation gallery + explicit
Add to prompt; HTTP browser SSRF without CDP.

## What landed

| Item | Result | Evidence |
|---|:---:|---|
| ADR-0040 SharedBrowserContract | **PASS** | `docs/decisions/0040-shared-browser-contract.md` |
| Supersede ADR-0015 shared-session | **PASS** | ADR-0015 addendum + ADR-0029 §9 amend |
| BrowserCoordBus dual domains | **PASS** | `browser_coord.rs` unit tests |
| Agent navigate records agent domain | **PASS** | kernel tool dispatch |
| web.search schema + provenance | **PASS** | `web_search.rs` offline fixtures |
| HTTP SSRF without CDP | **PASS** | `http_effector_navigate_rejects_ssrf_targets_without_cdp` |
| Annotation gallery + Add to prompt | **PASS** | `BrowserSurface.test.tsx` |
| Security map + C-17 | **PASS** | docs updated |
| Preview security tests | **PASS** | electron `preview-security.test.cjs` (hold) |
| Ledger four rows | **PASS** | parity |

## Residuals (owned, not grade failures)

| Residual | Owner |
|---|---|
| Live paint-parity e2e (agent navigate URL mirrored into preview chrome) | Optional UX polish; protocol + dual URLs proven in unit bus |
| Preview-side write into BrowserCoordBus from Electron main | P24/P25 if dual-URL status chrome needs host IPC |
| Optional CDP agent backend availability on every host | environment residual; HTTP fallback is parity path |

## Hold suite

```bash
cargo test -p optimus-kernel --lib browser_coord -- --test-threads=1
cargo test -p optimus-kernel --lib web_search -- --test-threads=1
cargo test -p optimus-kernel --lib browser::tests -- --test-threads=1
cd apps/optimus-ui && npm test -- --run src/components/workspace/BrowserSurface.test.tsx
cd apps/optimus-electron && npm test -- --run test/preview-security.test.cjs
python3 scripts/check-parity-ledger.py
python3 scripts/check-architecture-marks.py
```

## Non-claims

- Shared Chromium cookie jar / storage partition / single CDP target
- Agent CDP attached to Electron preview WebContentsView
- Hermes gate PASS
- Full concurrent-project lease (S2.14)

## Verdict

**program P23 coordinated browser exit: PASS** (pending three-expert board + merge).
Next: program P24 daily chat/session or parallel product phases.
