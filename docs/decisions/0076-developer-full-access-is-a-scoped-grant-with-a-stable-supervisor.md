---
doc_id: decisions-0076-developer-full-access-is-a-scoped-grant-with-a-stable-supervisor
doc_type: decision
plane: decision
status: current
authority: record
summary: Developer Full Access is an explicit, revocable local capability grant with a selected path scope and independent toggles; self-rebuilds run through a stable supervisor that health-checks, logs, stops, and restores a separate development instance.
reviewed_on: 2026-08-04
review_by: 2026-11-03
knowledge_type: decision
covers:
  - crates/optimus-policy/src/developer_access.rs
  - crates/optimus-policy/src/lib.rs
  - crates/optimus-kernel/src/lib.rs
  - crates/optimus-runtime/src/command_envelope.rs
  - crates/optimus-runtime/src/process_ownership.rs
  - crates/optimus-host/src/developer.rs
  - crates/optimus-host/src/chat.rs
  - crates/optimus-kernel/src/config.rs
  - crates/optimus-kernel/src/tool_dispatch.rs
  - crates/optimus-packs/src/invocation.rs
  - crates/optimus-packs/src/lib.rs
  - apps/optimus-tauri/src/main.rs
  - apps/optimus-ui/src/components/settings/DeveloperAccessPanel.tsx
  - apps/optimus-ui/src/components/workbench/WorkbenchStatusBar.tsx
depends_on:
  - docs/decisions/0035-command-capability-envelope.md
  - docs/decisions/0044-bounded-project-trust-and-capability-broker.md
  - docs/decisions/0060-owned-localhost-is-a-process-bound-lease.md
validated_by:
  - crates/optimus-policy/src/developer_access.rs
  - crates/optimus-policy/src/lib.rs
  - crates/optimus-host/src/developer.rs
  - apps/optimus-tauri/src/main.rs
  - crates/optimus-runtime/src/command_envelope.rs
  - apps/optimus-ui/src/components/workbench/Composer.test.tsx
  - apps/optimus-electron/test/ipc-allowlist.test.cjs
  - crates/optimus-kernel/tests/tool_coverage.rs
---

# ADR-0076: Developer Full Access is a scoped grant with a stable supervisor

- **Status:** Accepted
- **Date:** 2026-08-03

## Context

Optimus needs to develop and rebuild itself without turning the process that
serves the current conversation into an unavailable control channel. Existing
project access profiles are intentionally approval-oriented and do not provide
an explicit, broker-enforced local development authority with a separate
instance lifecycle.

## Decision

Optimus exposes `developer_full_access` as a distinct autonomy profile. The
profile is not authority by itself: each request must carry an active
`DeveloperAccessGrant` issued by the host after the one-time risk confirmation.
The grant records one of three scopes—selected repository, selected
directories, or entire local machine—and independent toggles for workspace
files, terminal execution, process management, package installation, network,
external services, production systems, and secrets.

The capability broker remains authoritative. A disabled, stale, malformed,
out-of-scope, or capability-disabled request is denied. Destructive effects may
still pause when the grant's destructive-action toggle is enabled. Commerce and
production access remain separate fences; the UI cannot turn production access
on in this mode.

The host owns a durable development supervisor state machine. It launches the
development binary as a separate process with a separate home, loopback port,
bearer token, workspace binding, and bounded log. A headless host child is
authenticated through its health endpoint; a windowed Tauri child is launched
with its own home and an embedded React bundle, then writes a PID-bound
readiness marker before it is considered healthy. Both surfaces can be
stopped, restarted, or rolled back, and a failed replacement leaves the
previous healthy instance in place. Revocation and emergency stop terminate
the child and clear the active instance without terminating the host serving
the current control channel.

When the user has a selected session, the supervisor can perform an explicit
session handoff at launch. It refuses the handoff while any parent session has
an active turn, checkpoints and atomically snapshots the parent session and
execution stores into the child home, writes a bounded handoff marker, and
passes the selected UUID to the windowed child. The child starts with the
copied transcript and selects that session through a startup context IPC call;
subsequent child work stays isolated from the parent. Restart and rollback do
not silently re-snapshot the parent, so the child remains internally
consistent after launch.

In `developer_full_access` chat turns, the kernel advertises one additional
agent tool, `self_development`. The host-owned callback defaults omitted
workspace/surface arguments from the active grant (selected roots use the first
root; whole-machine scope requires an explicit workspace), records a timed
action row, and delegates to the same build-before-stop supervisor route used
by the UI. Ordinary profiles and invalid or capability-incomplete grants do not
advertise this tool. The callback is an invocation seam, not a second command
executor: scope, capability, build, readiness, rollback, and revocation remain
enforced by the host supervisor and broker.

## Consequences

- The normal repository-scoped choice can support self-development without
  granting unrelated host authority.
- Whole-machine access is explicit and advanced rather than an alias for
  ordinary Full Project mode.
- The running control channel is not overwritten by a rebuild; the separate
  child is disposable and health-gated.
- The current implementation exposes the stable supervisor controls,
  authenticated development instance lifecycle, build-before-stop repository
  workflow, a UI action that builds and launches a separate Tauri desktop copy,
  explicit selected-session handoff into that copy, and an agent-facing
  `self_development` tool that uses the same supervisor. The desktop shell
  itself remains a separate window; automatic shell replacement or focus
  transfer must preserve the same fallback guarantee and cannot be an
  unguarded renderer redirect.
- The grant is local product state, not durable autonomy stored in the graph;
  graph records and ordinary runtimes do not acquire it by deserialization.

## Alternatives considered

- Extend `full_project` with implicit command, process, package, and network
  authority. Rejected because it hides a material privilege change behind an
  existing profile and gives no stable self-rebuild boundary.
- Replace the current desktop process in place after every rebuild. Rejected
  because a failed build or unhealthy startup would remove the control channel
  needed to recover.
- Put all authority in renderer-side confirmation prompts. Rejected because a
  prompt is not an enforcement boundary; the host and capability broker must
  validate every request.

## Reasons

- A persisted grant makes activation, scope, revocation, and capability choices
  inspectable and testable.
- A separate authenticated child makes health checks, rollback, and emergency
  stop concrete process operations rather than UI promises.
- Keeping production systems and commerce outside this grant preserves the
  existing high-risk fences even when local development authority is broad.

## Risks

- Whole-machine access can expose private data, delete files, install software,
  and alter local services; it is therefore an advanced explicit scope.
- A child process can still fail after its initial health check, so the stable
  supervisor remains the control channel and logs are bounded and authenticated.
- The child receives a point-in-time session snapshot, not a live shared
  database. Changes made in the parent after the snapshot are intentionally not
  mirrored into the child, and the child cannot write back into the parent
  control channel.

## Evaluation evidence

- `optimus-policy`: 33 unit tests pass, including scope, capability, stale grant,
  and destructive-pause cases.
- `optimus-host`: 71 unit tests pass, including activation, revocation, child
  cleanup, failed-launch rollback, and isolated session-handoff snapshot
  cases.
- `optimus-runtime`: 51 unit tests plus 11 path-confinement tests pass.
- `optimus-kernel`: 182 unit tests pass, including real-turn tool coverage and
  the no-bridge visibility guard.
- React UI: 155 tests pass; Electron contract tests: 19 pass; UI production
  build passes.
- Live smoke: authenticated headless and windowed child launch, PID-bound Tauri
  readiness, selected-session snapshot/readback, build/action/instance logs,
  failed-build preservation, restart, emergency stop, revocation, and
  zombie-free cleanup pass on rebuilt binaries.
- Agent path: the kernel tool-coverage turn executes `self_development`, proves
  it is hidden without the host bridge, and the host unit test proves selected-
  root defaulting plus a millisecond action-log record on build failure.

## Conditions for reconsideration

Revisit this decision when a desktop shell can transfer a live UI/session to a
healthy child and automatically return to the stable supervisor after a child
failure. That transfer must be tested as a process lifecycle, not inferred from
the existence of a new binary or a successful compile.

## Relevant code

- `crates/optimus-policy/src/developer_access.rs`
- `crates/optimus-host/src/developer.rs`
- `apps/optimus-tauri/src/main.rs`
- `scripts/test_self_development.py`
- `crates/optimus-runtime/src/workspace_identity.rs`
- `crates/optimus-runtime/src/command_envelope.rs`
- `crates/optimus-kernel/src/lib.rs`
- `crates/optimus-kernel/src/config.rs`
- `crates/optimus-kernel/src/tool_dispatch.rs`
- `crates/optimus-packs/src/invocation.rs`
- `crates/optimus-packs/src/lib.rs`
- `apps/optimus-ui/src/components/settings/DeveloperAccessPanel.tsx`

## Relevant tests

- `crates/optimus-policy/src/lib.rs`
- `crates/optimus-host/src/developer.rs`
- `crates/optimus-host/src/chat.rs`
- `crates/optimus-kernel/tests/tool_coverage.rs`
- `scripts/test_self_development.py`
- `crates/optimus-runtime/tests/path_confinement.rs`
- `apps/optimus-electron/test/ipc-allowlist.test.cjs`
- `apps/optimus-ui/src/components/workbench/Composer.test.tsx`
