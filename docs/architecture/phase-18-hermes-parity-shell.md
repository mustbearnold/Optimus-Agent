# Phase 18 — Hermes-parity shell + Digital Rome (Sprint S1 in flight)

**Date:** 2026-07-19  
**Historical plan:** `docs/plans/historical/2026-07-19_130511-hermes-parity-rome-shell.md`
**Process:** subagent-driven-development

## Sprint S1 scope
- T1 Rome design tokens
- T2 Shell grid (left / main / right / term / status)
- T3 Nav routes (chat, capabilities, messaging, artifacts)
- T4 Sessions PINNED + counts (queued after shell)
- T5 doctor_json live counts (parallel, ipc.rs)
- T10 Titlebar Files/Term/Logs toggles
- T16 partial Rome component pass
- Gate: Playwright + rebuild-install-relaunch + native check

## Delegations
| ID | Goal | Files | Status |
|---|---|---|---|
| deleg_86a520d8 | S1 shell UI IA | `ui/index.html`, e2e | **completed** — parent verified all IDs + **21/21 PW** |
| deleg_38125a5a | doctor_json counts | `ipc.rs` | **completed** — build OK |
| deleg_e08170fe | T5 spec review | read-only | **PASS** |
| deleg_c885284e | T4 pins + status bar bind | `ui/index.html`, e2e | **timeout** but partial tree **GREEN 23/23** — parent accepted |

## Acceptance (S1)
- [x] `--rome-inlay` token present
- [x] `#leftRail` `#statusBar`; right/term hidden by default; toggles work
- [x] One header; no logo in titlebar
- [x] Capabilities route shows page
- [x] doctor returns `cron_jobs`, `campaigns_active`, `approvals_pending`
- [x] PINNED sessions + status bar bound to doctor
- [x] PW **23/23**; install relaunch

## S1 complete
Sprint S2: approvals UI, campaigns drawer, cron operator, capabilities page body — **done** (PW 26/26).

## S2
| Item | Status |
|---|---|
| Approvals panel + Grant | done |
| Campaigns create/run UI | done |
| SIGNAL/CRON operator | done |
| Capabilities doctor budget bar | done |
| bridge wrappers | done |
| Thinking rows + tool cards v2 | pending (T9) |
| PW 26/26 + install | done |

## Next
S2 remainder T9 thinking presentation; then S3 Files/Term/Messaging/Artifacts.
