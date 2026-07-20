# Context

Optimus Agent Phase 0 — durable Work Graph spine.

## Domain language

- **Job** — durable unit of work with ordered nodes
- **Node** — single effect step (write_file, run_command)
- **Event** — append-only ledger record
- **Interrupted** — node was `running` when process died; not auto-succeeded
- **Resume** — open store, mark interrupted nodes retryable, run remaining work
- **Workspace** — directory jail for file/command effects

## Public seams (Phase 0)

- `optimus_runtime::Runtime::open(db_path, workspace)`
- `Runtime::create_job(spec) -> JobId`
- `Runtime::run_next(job_id) -> StepOutcome`
- `Runtime::run_all(job_id) -> JobStatus`
- `Runtime::recover_crashed_running()` — mark running→interrupted
- `Runtime::resume(job_id) -> JobStatus`
