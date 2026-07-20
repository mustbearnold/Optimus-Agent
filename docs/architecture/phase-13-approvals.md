# Phase 13 — SmartDeny approvals surface (function)

Date: 2026-07-19  
Priority note: **function > Hermes first; UI polish last** (user directive).

## Problem fixed

`run_all` treated `AwaitingApproval` as terminal even **after** a durable grant existed,
so `grant_and_resume` never executed the blocked node.

## Delivered

| Surface | API |
|---|---|
| Runtime | `list_pending_approvals`, `grant_and_resume`, `list_jobs_summary`, `job_id()` |
| CLI | `approvals list\|grant`, `jobs list\|resume\|submit-command` |
| Desktop IPC | `approvals_list`, `approvals_grant`, `jobs_list` |
| Doctor | `approvals: true`, phase 13 |

## Evidence

```text
cargo test -p optimus-runtime --test approvals_surface  # 2/2
optimus jobs submit-command cmd /C "echo granted-ok"
  → awaiting approval
optimus approvals grant <id>
  → Succeeded
npx playwright test  # 10/10
CUA get_text native: Codex ready + multi-turn chat intact
install relaunch pid on Programs\OptimusAgent
```

## Commands

```bash
optimus jobs submit-command cmd /C echo hi
optimus approvals list
optimus approvals grant <job-uuid>
optimus jobs list
```
