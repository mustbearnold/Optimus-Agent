# Product-complete program P23 hold — 2026-07-25

Planes: **program P23** · delivery **PR #33** · architecture hold

## Board

Three-expert review (architecture-security / product-ledger / correctness-UI) +
synthesis → **APPROVE-WITH-FIXES** (correctness initially **BLOCK** on send path).

### MUST-FIX applied

1. Composer **Send** merges gallery annotation with input (`composeSendMessage`) + unit tests
2. Agent coord bus records navigate **and** click; title reads `title` or `page_title`
3. HTTP post-redirect: unparsable `final_url` fails closed
4. SSRF suite asserts `BrowserError::Ssrf` strictly
5. Scorecard material partials / full-app “shared-session” language cleaned for ADR-0040
6. Verification delivery plane pinned to **PR #33**

## Commands (green on tip after fixes)

```text
cargo test -p optimus-kernel --lib -- browser_coord web_search http_effector
npm test -- BrowserSurface + composeSendMessage
preview-security.test.cjs → 13 pass
python3 scripts/check-parity-ledger.py → parity=17 partial=8
python3 scripts/check-architecture-marks.py → OK
```

## Ledger

- `browser.cdp` → parity (coordinated dual-domain, not shared session)
- `browser.http` → parity
- `browser.annotations` → parity
- `web.search` → parity

## Non-claims

- Shared cookie jar / storage partition / single CDP target
- Agent CDP attached to Electron preview WebContentsView
- Hermes gate PASS

## Verdict

**program P23 closed after review board fixes.** Next: **program P24**.
