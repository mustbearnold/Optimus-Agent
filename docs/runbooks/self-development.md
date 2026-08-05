---
doc_id: architecture-self-development
doc_type: how-to
plane: current
status: current
authority: supporting
summary: How to use Optimus from the desktop app to develop Optimus itself — the Developer Full Access grant, the development supervisor, the self_development agent tool, session handoff, and the acceptance gates that prove the lifecycle.
reviewed_on: 2026-08-05
review_by: 2026-10-31
knowledge_type: process
covers:
  - crates/optimus-policy/src/developer_access.rs
  - crates/optimus-host/src/developer.rs
  - crates/optimus-kernel/src/developer_runtime.rs
  - apps/optimus-ui/src/components/settings/DeveloperAccessPanel.tsx
  - scripts/tests/test_self_development.py
---

# Using Optimus for self-development

Spec: `specs/013-self-development/spec.md` · Decisions: ADR-0076, ADR-0077,
ADR-0078.

## What this is

Optimus can develop and rebuild itself without losing the conversation that is
doing the work. The mechanism is **Developer Full Access**: an explicit,
revocable grant (scope + capability toggles) that activates the
`self_development` agent tool and the **development supervisor**, which
builds and launches a separate Optimus instance with its own home, port,
token, workspace binding, and logs. The current window stays alive and
healthy while the child is built, health-checked, restarted, or rolled back.

## Enable Developer Full Access from the desktop app

1. Open the desktop app (Tauri shell, `optimus-desktop` entry).
2. Open **Settings → Terminal & execution → Developer mode**.
3. Pick a scope:
   - **Selected repository** — the normal choice for self-development: one
     repository root.
   - **Selected directories** — several roots without whole-machine access.
   - **Entire local machine** — explicit advanced opt-in; every local path is
     in scope.
4. Choose capabilities. Production systems and commerce can never be enabled
   here (ADR-0076/0078); a request that would need them asks for approval
   instead.
5. Click **Enable Developer Full Access** and confirm the one-time risk
   sentence. The banner flips to active and the supervisor section appears.

The grant is persisted in product settings (`settings.json` under the
home). It is local product state, not graph data, and ordinary runtimes do
not acquire it.

## Drive a self-development build

With the grant active and a session selected in the workbench:

- **Build + launch development desktop** — builds the current workspace
  (`cargo build --locked -p optimus-tauri --bin optimus-agent --features
  optimus-tauri/custom-protocol`), launches the child as a separate windowed
  Tauri instance, health-checks it (PID-bound readiness marker + `[optimus-tauri]
  ready ui=react`), and hands the selected session over to it.
- **Launch development copy** — launches an already-built binary without
  rebuilding.
- **Restart / Rollback / Stop / Emergency stop** — concrete process
  operations on the child. A failed build never displaces the previous
  healthy instance; rollback returns to it.
- **View live logs** — instance log, build log, and the bounded action log
  (every grant/supervisor action with start/end milliseconds and duration).

The handoff is a point-in-time snapshot: the child gets a copy of the parent
session and execution stores, and subsequent child work stays isolated from
the parent. The supervisor refuses the handoff while the parent session has
an active turn.

## Agent-driven self-development

In a `developer_full_access` chat turn the kernel advertises one extra tool,
`self_development`. It is invisible to ordinary profiles and to sessions
without the host bridge. Calling it uses the same supervisor route as the
UI: scope, capability, build, readiness, rollback, and revocation are still
enforced by the host supervisor and the capability broker — the tool is an
invocation seam, not a second command executor.

## Acceptance

`scripts/tests/test_self_development.py` exercises the real lifecycle against
a built binary: parent health, grant enable, handoff-session create,
build + launch, handoff snapshot verification, logs, a failed-build probe
that must preserve the healthy child, restart, emergency stop, and
revocation. It runs in the gate spine on both surfaces:

```bash
bash scripts/verify.sh all            # includes both surfaces
just self-development                 # host surface only
just self-development-desktop         # windowed Tauri child surface
```

The desktop e2e suite (`apps/optimus-desktop/e2e/08-self-development.spec.js`)
pins the host IPC surface: disabled-by-default state, one-time confirmation
enforcement, and the enable → status → revoke round-trip.
