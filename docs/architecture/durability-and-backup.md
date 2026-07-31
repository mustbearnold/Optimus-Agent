---
doc_id: architecture-durability-and-backup
doc_type: explanation
plane: current
status: current
authority: supporting
summary: Date: 2026-07-25 Planes: program P18 · mark Durability / crash safety · delivery PR #28
reviewed_on: 2026-07-31
review_by: 2026-10-31
---

# Durability and backup (operator contract)

Date: 2026-07-25  
Planes: program **P18** · mark **Durability / crash safety** · delivery **PR #28**

## Scope of architecture Durability S+++

**In scope (Confirmed process-local / local SQLite):**

- Work Graph jobs/nodes/effects in `optimus.db` (exactly one terminal outcome;
  crash-resume; quarantine on corrupt projection).
- Campaign plans/leases in the same `optimus.db` (schema versioned).
- Session transcripts + effect links in `sessions.db` with repair-on-open when a
  durable effect link outlives a tool message.
- Local gateway delivery authority (`gateway/gateway.db` + adapter dirs) and
  cron leases (`cron.db`) as **local** fencing — not off-box exactly-once.
- Memory / skills / execution DBs as independent SQLite files under the same home.

**Out of scope for this mark (explicit residual):**

- External messaging **exactly-once** across third-party networks (Telegram,
  etc.). Local leases/claims remain Confirmed; cross-host delivery is not
  claimed S+++.
- A single distributed transaction spanning all home DBs.

## Backup set

Prefer copying the **entire Optimus home** while writers are stopped.

Minimum path set (also emitted by `optimus doctor backup-list`):

| Relative path | Role |
|---|---|
| `optimus.db` (+ `-wal`/`-shm` if present) | Work Graph + campaigns |
| `sessions.db` (+ wal/shm) | Transcripts + effect links |
| `memory.db` (+ wal/shm) | MetaMemory claims |
| `skills.db` (+ wal/shm) | Skills registry |
| `execution.db` (+ wal/shm) | Execution manifests / tool lifecycle |
| `cron.db` (+ wal/shm) | Cron schedules and leases |
| `gateway/gateway.db` (+ wal/shm) | Gateway claims/attempts |
| `gateway/inbox`, `gateway/outbox`, `gateway/processed`, `gateway/failed` | Adapter file queues |
| `routing.db` (+ wal/shm) | Routing telemetry |
| `settings.json` | Product settings (not secrets) |
| `workflow-runs.db` (+ wal/shm) | Durable workflow run ledger |
| `agent-invocations.db` (+ wal/shm) | Agent invocation ledger |
| `workflow-registry.db` (+ wal/shm) | Workflow definition registry |
| `agent-registry.db` (+ wal/shm) | Agent descriptor registry |
| `project-authority.json` | Project root authority |
| `artifacts/` | Content-addressed blobs |

### Cold backup procedure

1. Stop Optimus CLI, desktop host, gateway serve, and cron runners using the home.
2. `optimus --home <HOME> doctor backup-list` — confirm present paths.
3. Copy the home directory (or every present path from backup-list) to immutable storage.
4. Record product version: `optimus version --json`.

### Restore procedure

1. Stop writers.
2. Replace the home directory (or restore listed files in place).
3. `optimus --home <HOME> doctor` — expect schema versions OK and quarantine empty
   (or investigate quarantined jobs before resume).
4. `optimus --home <HOME> resume-all` only after doctor is clean for intended work.

## Doctor commands

```bash
# Multi-DB schema inventory + quarantine
optimus --home .optimus doctor
optimus --home .optimus doctor --json

# Backup path set
optimus --home .optimus doctor backup-list
optimus --home .optimus doctor backup-list --json
```

Doctor is **read-only** (never creates or migrates DBs). It exits non-zero when
schema skew, open/inspect failures, or quarantined jobs are reported.

## Crash / resume operator notes

- Running nodes require recovery: `optimus resume` / `resume-all` calls
  `recover_crashed_running` before resume.
- Prepared `RunCommand` attempts that crash mid-flight become **ambiguous** and
  are never blindly replayed.
- Session open repairs missing tool messages from durable effect links
  (deterministic JSON with `"repaired": true`).

## Related

- [s-plus-plus-plus-p18-verification.md](./s-plus-plus-plus-p18-verification.md)
- [system-overview.md](./system-overview.md) state table
- Program phase P18 in [s-plus-plus-plus-program.md](../plans/s-plus-plus-plus-program.md)
