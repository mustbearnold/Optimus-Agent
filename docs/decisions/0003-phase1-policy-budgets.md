# ADR-0003: Phase 1 policy, budgets, and bounded commands

## Status

Accepted — 2026-07-18

## Context

Phase 0 proved crash-resume for pure file effects. Phase 1 must exceed Hermes on:

- default security posture (smart-deny, not YOLO)
- runaways (step budgets + consecutive-failure circuit breaker)
- command processes that outlive the agent (timeout + kill)
- multi-job recovery after a supervisor restart

## Decision

1. **PolicyMode::SmartDeny (default)**  
   - Low-risk effects (`WriteFile`, `AssertFileEquals`) auto-run.  
   - High-risk effects (`RunCommand`) require an explicit `ApprovalGrant` before execution.  
   - Without a grant, the node becomes durable `awaiting_approval` (not failed).  
   - `PolicyMode::Unrestricted` exists only for explicit local break-glass / tests.

2. **Per-job budgets (durable on the job row)**  
   - `max_steps` (default 100)  
   - `max_consecutive_failures` (default 3)  
   - `command_timeout_ms` (default 30_000)  
   Exceeding steps or consecutive failures marks the job **failed** with a ledger event (circuit open).

3. **Bounded subprocess effector**  
   - Spawn with workspace cwd.  
   - Enforce `command_timeout_ms`.  
   - On timeout or cancel, kill the child process (best-effort).  
   - Never leave a successful node if the process was killed.

4. **Multi-job supervisor helpers**  
   - `recover_crashed_running` already interrupts `running` nodes.  
   - `list_resumable_jobs` → interrupted or awaiting_approval or pending-with-progress.  
   - `resume_all` recovers running, then resumes each resumable job.

5. **Schema**  
   - Bump store `schema_version` to `2` with additive job budget columns and `awaiting_approval` node status.

## Consequences

- Demo/CLI pure-file paths need no approvals.  
- Any `RunCommand` in tests must grant approval or use `Unrestricted`.  
- Windows command timeout tests use a long `powershell Start-Sleep` killed by the runtime.

## Alternatives rejected

- Hermes-like smart-allow defaults.  
- Soft token budgets only (no step/circuit limits).  
- Detached commands without timeout.
