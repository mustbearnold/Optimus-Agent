# Optimus Agent developer domain context

This is concise domain language for coding agents working on the Optimus source
tree. It is not loaded into installed Optimus product sessions and it does not
define product-agent behaviour. The current architecture authority is
[`docs/architecture/system-overview.md`](docs/architecture/system-overview.md);
the Work Graph vocabulary below is foundational, not a complete inventory of
the July 31, 2026 product.

## Foundational domain language

- **Job** — durable unit of work with ordered nodes
- **Node** — single effect step (write_file, run_command)
- **Event** — append-only ledger record
- **Interrupted** — node was `running` when process died; not auto-succeeded
- **Resume** — open store, mark interrupted nodes retryable, run remaining work
- **Workspace** — directory jail for file/command effects
- **Turn** — one user request carried to exactly one terminal outcome
- **Tool** — typed capability whose deterministic effects execute outside prompts
- **Project grant** — bounded, persisted authority for routine effects in one
  identified project
- **Approval** — explicit authority for a particular high-risk effect; not a
  general autonomy setting
- **Agent** — narrow specialist with typed inputs and outputs
- **Workflow** — runtime-owned orchestration of agents and deterministic nodes

## Durable Work Graph seams

- `optimus_runtime::Runtime::open(db_path, workspace)`
- `Runtime::create_job(spec) -> JobId`
- `Runtime::run_next(job_id) -> StepOutcome`
- `Runtime::run_all(job_id) -> JobStatus`
- `Runtime::recover_crashed_running()` — mark running→interrupted
- `Runtime::resume(job_id) -> JobStatus`
