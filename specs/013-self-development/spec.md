---
doc_id: spec-013-self-development
doc_type: reference
plane: work
status: current
authority: canonical
summary: The self-development vertical — Developer Full Access as an explicit scoped grant, the stable supervisor that builds and health-checks a separate development instance, the agent-facing self_development tool, session handoff, the desktop UI panel, and the gate wiring that proves the whole lifecycle.
reviewed_on: 2026-08-05
review_by: 2026-10-31
knowledge_type: specification
covers:
  - crates/optimus-policy/src/developer_access.rs
  - crates/optimus-policy/src/lib.rs
  - crates/optimus-host/src/developer.rs
  - crates/optimus-host/src/developer_build.rs
  - crates/optimus-host/src/developer_handoff.rs
  - crates/optimus-host/src/developer_process.rs
  - crates/optimus-host/src/chat.rs
  - crates/optimus-kernel/src/developer_runtime.rs
  - crates/optimus-kernel/src/tool_dispatch.rs
  - crates/optimus-kernel/src/system_prompt.rs
  - apps/optimus-ui/src/components/settings/DeveloperAccessPanel.tsx
  - scripts/tests/test_self_development.py
depends_on:
  - docs/decisions/0076-developer-full-access-is-a-scoped-grant-with-a-stable-supervisor.md
  - docs/decisions/0077-verified-progress-per-token-is-the-development-objective.md
  - docs/decisions/0078-a-transcript-is-a-provider-contract-and-an-unreachable-toggle-asks.md
  - specs/001-desktop-shell/spec.md
  - specs/002-host-ipc/spec.md
validated_by:
  - scripts/tests/test_self_development.py
  - crates/optimus-policy/src/developer_access.rs
  - crates/optimus-host/src/developer.rs
  - crates/optimus-kernel/tests/tool_coverage.rs
  - apps/optimus-ui/src/components/settings/DeveloperAccessPanel.test.tsx
  - apps/optimus-desktop/e2e/08-self-development.spec.js
---

# Self-development

Status: current
Owner: optimus-agent-development (prompt-only owner)

## Purpose

Optimus develops and rebuilds itself without turning the process that serves
the current conversation into an unavailable control channel. The vertical
combines an explicit, revocable local capability grant (Developer Full
Access), a stable supervisor that launches the development binary as a
separate authenticated instance, an agent-facing `self_development` tool that
uses the same supervisor route as the UI, selected-session handoff into the
child, and a desktop panel that exposes the whole lifecycle.

## Requirements

- R1. Developer Full Access MUST be an explicit autonomy profile backed by a
  persisted `DeveloperAccessGrant`: one-time confirmation, one of three
  scopes (selected repository, selected directories, entire local machine),
  and independent capability toggles (workspace files, terminal execution,
  process management, package installation, network, external services,
  production systems, secrets). [ADR-0076]
- R2. The capability broker MUST remain authoritative: a disabled, stale,
  malformed, out-of-scope, or capability-disabled request is denied;
  `production_systems` and `commerce.spend` can never be enabled by the
  grant or the UI. A request whose capability toggle the user cannot enable
  asks for approval of the exact action instead of denying with impossible
  advice. [ADR-0078]
- R3. The host MUST own a durable supervisor state machine: separate child
  home, loopback port, bearer token, workspace binding, and bounded logs; a
  headless child is authenticated through its health endpoint and a windowed
  Tauri child writes a PID-bound readiness marker before it is considered
  healthy. [ADR-0076]
- R4. Build-before-stop MUST hold: a failed or unhealthy replacement leaves
  the previous healthy instance in place; restart, rollback, emergency stop,
  and revocation must be concrete process operations with exactly one
  terminal outcome each.
- R5. Selected-session handoff MUST be explicit: refused while the parent
  session has an active turn, atomically snapshots the parent session and
  execution stores into the child home, writes a bounded handoff marker, and
  passes the selected UUID to the windowed child; restart and rollback do
  not silently re-snapshot the parent.
- R6. In `developer_full_access` chat turns, the kernel MUST advertise
  exactly one additional agent tool, `self_development`; ordinary profiles,
  invalid grants, and sessions without the host bridge MUST NOT advertise
  it. The callback is an invocation seam, not a second command executor:
  scope, capability, build, readiness, rollback, and revocation stay
  enforced by the host supervisor and broker. [ADR-0076]
- R7. Every supervisor and grant action MUST be recorded in the bounded
  Developer Full Access action log with a stable action id, start/end
  Unix milliseconds, monotonic `duration_ms`, method, and outcome.
  [ADR-0077]
- R8. The desktop UI MUST expose the grant (scope, capabilities, pause,
  checkpoint) and the supervisor controls (build + launch, launch, restart,
  rollback, stop, emergency stop, logs) through the typed renderer surface
  of spec-002, and MUST NOT offer a toggle for production systems.
- R9. The acceptance for this vertical MUST run in the gate spine
  (`scripts/verify.sh`): the real host lifecycle (`test_self_development.py`,
  host surface) and the windowed Tauri child lifecycle
  (`test_self_development.py --surface desktop`, display-guarded), so a
  regression turns the release gate red instead of living behind a `just`
  recipe. [spec-011 R1]

## Acceptance criteria

- [x] A1. Given a built host binary and a real repository workspace, when
      `test_self_development.py` runs, then the full lifecycle passes:
      parent health, grant enable, handoff-session create, build + launch,
      handoff snapshot verified, logs, failed-build probe preserves the
      healthy child, restart, emergency stop, and revocation, and it prints
      `SELF_DEVELOPMENT_OK`. (proven 2026-08-05: host surface OK; desktop
      surface OK with `[optimus-tauri] ready ui=react`)
- [x] A2. Given the kernel tool-coverage suite, when a real turn invokes
      `self_development`, then it dispatches through `Kernel::turn` with
      supervisor evidence, and an ordinary session without the host bridge
      does not advertise it. (proven 2026-08-05: tool_coverage tests green)
- [x] A3. Given the renderer panel tests, when the Developer Access panel
      drives the fixture transport, then enable/revoke and
      build-and-launch-with-handoff invoke the exact supervisor methods with
      the selected session id. (proven 2026-08-05: vitest green)
- [x] A4. Given the desktop e2e suite over the real host, when the settings
      dialog drives the Developer Access panel, then enable shows the active
      banner and revoke returns the panel to disabled. (proven 2026-08-05:
      `08-self-development.spec.js` green)
- [x] A5. Given the gate spine, when `bash scripts/verify.sh all` runs, then
      the `self-development acceptance (host)` and
      `self-development acceptance (desktop)` gates run and pass (desktop
      surface display-guarded like the tauri launch acceptance gate).
      (proven 2026-08-05)

## Out of scope

- Runtime product policy for ordinary (non-developer) sessions — the
  firewall in spec-011 and `OPTIMUS_AGENTS.md` governs that plane.
- Whole-machine access as an ordinary profile alias: it is an explicit,
  advanced scope only.
- Live session transfer between shells: the child receives a point-in-time
  snapshot; automatic shell replacement or focus transfer is future work
  (ADR-0076 conditions for reconsideration).

## Open questions

- None.

## Links

- `docs/decisions/0076-developer-full-access-is-a-scoped-grant-with-a-stable-supervisor.md`
- `docs/decisions/0077-verified-progress-per-token-is-the-development-objective.md`
- `docs/decisions/0078-a-transcript-is-a-provider-contract-and-an-unreachable-toggle-asks.md`
- `specs/001-desktop-shell/spec.md` — the surface the windowed child opens.
- `specs/002-host-ipc/spec.md` — the typed renderer method surface.
- `docs/runbooks/self-development.md` — how to use the vertical from the
  desktop app.
