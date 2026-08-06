---
doc_id: architecture-desktop-ops
doc_type: explanation
plane: current
status: current
authority: canonical
summary: Desktop daily use, run/build/streaming, durability and backup, doctor, crash/resume operations.
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: architecture
owns:
  - apps/optimus-desktop/src/main.rs
  - apps/optimus-cli/src/main.rs
---

# Desktop daily use and operations

## Daily use (desktop)

## Honest status

**Still not a full Hermes replacement** (no messenger gateway, cron UI, browser automation, skill editor).

**Now usable for local daily chat** if Codex OAuth is imported:

| Capability | Status |
|---|---|
| Real multi-turn sessions (SQLite) | yes |
| Sidebar = real sessions list | yes |
| Resume prior session | yes |
| Live **Codex** chat (SSE OAuth) | yes (default provider) |
| OpenAI-compatible API key chat | yes |
| Offline echo / memory demo | yes |
| Import Codex from Hermes (read-only) | yes (button + CLI) |
| Non-blocking chat (UI thread) | yes (worker thread) |
| Light/dark theme | yes |
| Terminal/tools in loop | yes when model calls tools |
| Browser effector | stub only |
| Gateway / Telegram / cron UI | no |
| Streaming tokens to UI | no (full turn then paint) |

## Run

```bash
cargo run -p optimus-desktop
```

1. Click **Import Codex** (or `optimus auth codex import-hermes` with same home)
2. **New session**
3. Provider = **gpt-5.4 · Codex**
4. Chat

Home: `%LOCALAPPDATA%/optimus`

## Build

`cargo build -p optimus-desktop` — green after this slice.


## Streaming (desktop)

## What shipped

End-to-end **token streaming** for daily chat:

| Layer | Behavior |
|---|---|
| `StreamEvent` | `TextDelta` · `ToolStatus` · `Status` |
| `ModelProvider::complete_streaming` | default one-shot; overrides stream |
| `ScriptedModel` | ~12-char chunks (UI/Playwright) |
| `CodexOAuthModel` | live SSE line reader → delta sink |
| `Kernel::turn_with_sink` | forwards model + tool events |
| HTTP | `POST /api/chat/stream` (SSE) |
| WebView | `chat_stream` IPC + `__optimusStream` pushes |
| UI | progressive bubble + caret while streaming |

## Verification

```text
cargo test --workspace -- --test-threads=1   # all green
cd apps/optimus-desktop && npx playwright test
  7 passed (3.6s)
```

Includes:
- Enter streams offline reply progressively
- SSE endpoint emits `delta` then `done`

## Run

```bash
# native window (streams via WebView IPC)
cargo run -p optimus-desktop

# Playwright / browser
cargo run -p optimus-desktop -- --http 8787
cd apps/optimus-desktop && npx playwright test
```

## Daily-use status (updated)

| Need | Status |
|---|---|
| Multi-turn sessions | yes |
| Sidebar sessions | yes |
| Enter-to-send | yes (Playwright) |
| Live Codex OAuth | yes |
| **Streaming tokens** | **yes** |
| HTTP e2e harness | yes |
| Gateway / cron / browser agent | no |

Still not a full Hermes OS — but local chat is now usable with progressive replies.


## Durability and backup

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

- s-plus-plus-plus-p18-verification.md (atticked)
- [system-overview.md](../architecture.md) state table
- Program phase P18 in s-plus-plus-plus-program.md (atticked)
