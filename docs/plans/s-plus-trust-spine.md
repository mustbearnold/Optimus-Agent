---
knowledge_type: plan
status: current
owns:
  - docs/plans/s-plus-trust-spine.md
watches:
  - crates/optimus-graph/src/lib.rs
  - crates/optimus-runtime/src/lib.rs
  - crates/optimus-runtime/src/campaign.rs
  - crates/optimus-kernel/src/session.rs
  - docs/maps/security-and-approvals.md
covers:
  - docs/plans/s-plus-trust-spine.md
depends_on:
  - docs/architecture/architecture-marks.md
validated_by:
  - crates/optimus-runtime/tests/approvals_surface.rs
  - crates/optimus-runtime/tests/path_confinement.rs
  - crates/optimus-kernel/tests/session_resume.rs
last_verified_commit: null
---

# S+++ program — Phase 0–1 executable plan

## Phase 0 — Truth freeze (docs)

- [x] Architecture marks scorecard: `docs/architecture/architecture-marks.md`
- [x] Default shell = Electron + React; Wry = legacy rollback
- [x] Align `docs/architecture/sota-scorecard.md` desktop truth
- [x] Align security map WriteFile / ProjectWriteFile risk language
- [x] Keep scorecard + marks updated in the same PR as behaviour changes

## Phase 1 — Trust spine

### 1A — Effect risk policy

| Microtask | Status | Owner |
|---|---|---|
| T1 Plain `Effect::WriteFile` is high-risk under SmartDeny | done | `optimus-graph` |
| T2 Path-shape preflight rejects escapes before approval wait | done | `optimus-runtime` |
| T3 Skill grant: `FsWorkspace` for writes, `Terminal` for commands | done | `optimus-runtime` |
| T4 Campaign docs/tests: WriteFile steps may `AwaitingApproval` | done | `campaign` |
| T5 Command env sanitisation (strip loader injection vars) + cwd = workspace | done | `optimus-runtime` |
| T6 Document residual: approved process can still open absolute paths outside workspace | done | security map |

### 1B — Effect ↔ session coupling

| Microtask | Status | Owner |
|---|---|---|
| T7 On session load, effect links without tool messages are repaired into transcript | done | `session` |
| T8 Regression: simulate effect link without tool message → open repairs | done | `session_resume` |

### 1C — Cancellation honesty

| Microtask | Status | Owner |
|---|---|---|
| T9 Document Stop = cooperative token; terminal outcome authoritative | done (system-overview existing + marks) | architecture |
| T10 Existing cancellation tests remain green | verify in this change | maps |

## Exit gate for Phase 1

- [x] SmartDeny high-risk set includes all host-mutating file writes (`WriteFile`, `ProjectWriteFile`) and commands.
- [x] Path preflight does not require user approval of syntactically illegal paths.
- [x] Session open repairs missing tool messages for durable effect links.
- [x] Focused tests green: `optimus-runtime` full suite + `optimus-kernel` (incl. `session_resume`).
- [x] Security map and architecture-marks grades updated honestly (not S+++ until command FS capability is closed).

## Out of scope for Phase 1

- Kernel crate split (Phase 2) — **done as `optimus-eval` + `optimus-ops`**
- Specialist agents (Phase 3)
- MCP / Telegram (Phase 6)
- Full Landlock/bwrap sandbox for child processes (tracked residual)

## Phase 2 — Kernel waist extraction (done)

| Extracted crate | Contents | Dependency rule |
|---|---|---|
| `optimus-ops` | Gateway delivery authority, cron store | No kernel dependency; kernel re-exports |
| `optimus-eval` | eval, evaluation, replay | Depends on kernel; kernel must not depend on eval |

**Remaining in kernel waist (later peels):** agent/workflow contracts, artifacts, browser, routing, turn loop, trace (production path), credentials.

**Import rule for consumers:** offline eval APIs → `optimus_eval`; turn loop → `optimus_kernel`; gateway/cron types available via kernel re-export or `optimus_ops`.

## Phase 3 — Multi-agent vertical (done)

| Piece | Identity | Notes |
|---|---|---|
| Specialist | `workspace_writer@1.0.0` | write_file only; SmartDeny-bound |
| Workflow | `write_file_handoff@1.0.0` | Job adapter + agent binding |
| Executor | `run_write_file_handoff` | invocation → job → provenance → artifact → settle |
| CLI | `optimus vertical list\|write-file` | operator surface |
| Tests | `crates/optimus-kernel/tests/specialist_vertical.rs` | seed, deny, grant, success, cancel fence |

**Still not claimed:** general specialist routing, parallel children, arbitrary DAG workflows.

## Phase 4 — One shell + IPC matrix (done)

| Piece | Location |
|---|---|
| Default shell truth | README, architecture-marks, system-overview, install script (Electron primary) |
| IPC matrix checker | `scripts/check-desktop-ipc-matrix.py` + unit tests |
| Electron allowlist tests | `apps/optimus-electron/test/ipc-allowlist.test.cjs` |
| Contract doc | `docs/contracts/desktop-shell-and-ipc-matrix.md` |
| Preview vs agent browser | UI note + contract table |

**Critical invoke methods gated:** sessions, chat_approval_resolve, project_scopes_*, approvals_*, fs_*, settings_*, doctor, ping.

## Phase 5 — Causal observability (done)

| Piece | Location |
|---|---|
| Causal load API | `optimus_kernel::load_causal_turn` / `list_recent_causal_turns` |
| CLI | `optimus trace show` / `optimus trace recent` |
| Security denial codes | `SecurityDenialCode` + `classify_security_denial` |
| Merge gate | `python3 scripts/check-observability-gate.py` |
| Tests | `crates/optimus-kernel/tests/causal_trace.rs` |

**Still not claimed:** OpenTelemetry export; single distributed transaction across all stores.
