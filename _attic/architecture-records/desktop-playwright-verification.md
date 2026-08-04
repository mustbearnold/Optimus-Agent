---
doc_id: architecture-desktop-playwright-verification
doc_type: history
plane: history
status: historical
authority: historical
summary: Optimus desktop now supports headless browser testing:
reviewed_on: 2026-07-31
review_by: never
---

# Desktop Playwright verification — 2026-07-19

## Harness

Optimus desktop now supports headless browser testing:

```bash
cargo build -p optimus-desktop
set OPTIMUS_HTTP_TOKEN=<32-or-more-random-characters>
cargo run -p optimus-desktop -- --http 8787 --development-http --home %TEMP%/optimus-e2e

cd apps/optimus-desktop
npm install
npx playwright test
```

`--http PORT --development-http` serves the same UI + `/api/ipc` JSON bridge
(no native window). A strong `OPTIMUS_HTTP_TOKEN` is mandatory. The injected test
bridge supplies bearer/CSRF headers; unsafe requests also require the exact
loopback origin. Wildcard CORS is disabled.

## Automated results

```text
npx playwright test
  6 passed (2.8s)
```

| Test | Result |
|---|---|
| health API | pass |
| UI leaves Starting… | pass (~200ms) |
| Enter sends offline chat | pass |
| Theme toggle | pass |
| New session thread | pass |
| IPC doctor fetch | pass |

## Manual Playwright review script findings

Against live HTTP server with Hermes Codex import:

| Check | Result |
|---|---|
| Boot banner | `Codex ready · import:hermes · refresh yes` |
| Light theme | `data-theme=light` |
| Enter send | user bubble + `offline echo: …` |
| Input cleared | empty after send |
| Shift+Enter | newline kept (`line1\nline2`) |
| Sidebar thread | count ≥ 1 |

Screenshots: `local/tmp/pw-review/01-boot.png` … `03-chat.png`

## Improvements landed from this pass

1. **HTTP + Playwright harness** (`--http`)
2. **Bridge** supports fetch IPC in browser
3. **Bootstrap** single-flight, ~200ms (was multi-second / stuck)
4. **Enter-to-send** capture-phase; Shift+Enter newline
5. **Default offline** when Codex missing (first-run chat works)
6. **Auth import** into desktop home documented

## Still not daily-OS complete

- No token streaming in UI
- Codex live path not covered by default e2e (offline deterministic)
- No gateway/cron/browser effector
- Native WebView path not driven by Playwright (HTTP parity instead)

## Commands of record

```bash
export CARGO_TARGET_DIR="E:/Projects/Optimus Agent/local/tmp/cargo-target"
cargo build -p optimus-desktop
cd apps/optimus-desktop && npx playwright test
```
