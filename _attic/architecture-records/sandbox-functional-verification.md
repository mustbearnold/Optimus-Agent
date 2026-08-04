---
doc_id: architecture-sandbox-functional-verification
doc_type: history
plane: history
status: historical
authority: historical
summary: - CLI/desktop install: %LOCALAPPDATA%\Programs\OptimusAgent\ - Default home: %LOCALAPPDATA%\optimus - HTTP sandbox: optimus-desktop --http 8787 --home <tmp> - Note: computeruse tool not available in this Hermes session; exercised via...
reviewed_on: 2026-07-31
review_by: never
---

# Sandbox functional verification — 2026-07-19

## Environment

- CLI/desktop install: `%LOCALAPPDATA%\Programs\OptimusAgent\`
- Default home: `%LOCALAPPDATA%\optimus`
- HTTP sandbox: `optimus-desktop --http 8787 --home <tmp>`
- Note: `computer_use` tool not available in this Hermes session; exercised via CLI + Playwright + native relaunch.

## Matrix results

| Surface | Check | Result |
|---|---|---|
| doctor | phase 12 banner | **pass** |
| auth | import-hermes + status present | **pass** |
| chat-offline | echo session | **pass** |
| sessions | list titled | **pass** |
| cron add/list/tick | offline heartbeat | **pass** |
| chat --provider codex | `optimus-sandbox-ok` | **pass** |
| browser_live test | example.com | **pass** |
| browse navigate CLI | status 200 Example Domain | **pass** |
| Playwright e2e | 9 tests | **pass** |
| install/relaunch | pid on install path | **pass** |

## Fixes/improvements this session

1. Expanded Playwright: cron IPC + multi-turn UI  
2. CLI `browse navigate|snapshot|click`  
3. Desktop cron list + Add/Tick controls  
4. Doctor shows browser/cron flags  

## Open (next continuous loop)

- Native GUI computer-use when tool available  
- Approvals UI  
- Gateway lite  
- CDP browser pack  

## Repeat command

```bash
bash scripts/rebuild-install-relaunch.sh --dev
# CLI matrix + PW
cd apps/optimus-desktop && npx playwright test
optimus browse navigate https://example.com/
```
