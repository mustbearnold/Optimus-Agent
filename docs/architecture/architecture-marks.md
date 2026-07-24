---
knowledge_type: architecture
status: current
owns:
  - docs/architecture/architecture-marks.md
  - docs/plans/s-plus-trust-spine.md
watches:
  - docs/architecture/system-overview.md
  - docs/maps/security-and-approvals.md
  - crates/optimus-graph/src/lib.rs
  - crates/optimus-runtime/src/lib.rs
  - crates/optimus-kernel/src/session.rs
covers:
  - docs/architecture/architecture-marks.md
depends_on:
  - docs/architecture/system-overview.md
  - docs/decisions/0001-kernel-and-work-graph.md
  - docs/decisions/0028-electron-react-shell-rust-host.md
validated_by:
  - crates/optimus-runtime/tests/approvals_surface.rs
  - crates/optimus-runtime/tests/path_confinement.rs
last_verified_commit: null
---

# Architecture marks (S+++ program)

This is the long-horizon scorecard for Optimus architectural quality. It is
**not** a Hermes feature ledger and **not** a product-completeness checklist.

**Rule:** a mark may only move up when exit criteria below are met in source and
tests. Planned work must not be graded as current behaviour.

## Grade scale

| Grade | Meaning |
|---|---|
| **S+++** | Adversarial review cannot find a structural hole in this dimension; invariants enforced by code + tests; docs match. |
| **S / S+** | Strong; residual known boundaries are documented and gated. |
| **A / A+** | Solid production architecture; debt is explicit. |
| **B** | Workable; material gaps under hard review. |
| **C** | Contracts or shells exist without full product reality. |

## Current grades (2026-07-25)

| Mark | Grade | Notes |
|---|:---:|---|
| Durability / crash safety | **A+** | Work Graph, ambiguous effects, process-tree ownership. Session open repairs missing tool messages from effect links. |
| Security boundary design | **A-** | SmartDeny high-risk: WriteFile, ProjectWriteFile, RunCommand, ProjectRunCommand. Path preflight before approval. Skill grants class-scoped. Linux commands use bwrap/systemd-run; residual absolute-path access outside cap-std still known. |
| Domain modularity (packs/memory/skills/store) | **A-** | Deep modules; keep second catalogs out of kernel. |
| Control-plane modularity | **B+** | Offline eval/replay extracted to `optimus-eval`; gateway+cron store to `optimus-ops`. Kernel still owns agents/workflows/artifacts/routing/turn loop. Further peel planned. |
| Multi-agent readiness | **B** | Built-in `workspace_writer` specialist + `write_file_handoff` executor (Work Graph + SmartDeny + artifact handoff). Not a general multi-agent DAG platform yet. |
| Observability / eval | **A-** | Offline integrity gate + causal reconstruction CLI (`optimus trace show`); stable security-denial codes. No OTel export yet. |
| UI architecture | **A-** | Electron + React default installed shell; Wry legacy only. IPC matrix enforces host registry ⊇ Electron allowlist = React types; critical paths gated. Preview browser product language separated from agent tools. |
| Doc / claim hygiene | **A-** | Status legends strong; scorecard/shell drift closed by this program. |
| Release / parity gating | **A** | Fail-closed Hermes/version gates; keep them. |

## Default product shell (truth freeze)

**Confirmed current behaviour:**

- **Default install / daily desktop:** Electron + React workbench (`apps/optimus-electron` + `apps/optimus-ui`) over `optimus-desktop --host-only` (Rust host).
- **Legacy rollback:** Wry/Tao shell (`optimus-desktop` native window) via desktop action / `LegacyWry` path.
- **Repository-level default shell for development:** Electron React (see ADR-0028).
- **HTTP mode on the host:** development/Playwright only, not the installed daily path.

Installer authority: `scripts/rebuild-install-relaunch.sh` stages Electron as the primary desktop entry and exposes Legacy Wry as a secondary action.

## S+++ exit criteria (by mark)

### Durability

- Crash at any phase yields exactly one terminal outcome.
- Effect receipts and session/tool transcript either share a transaction or have deterministic **repair on open**.
- Resume never invents success for `running` work.

### Security

- Every host-mutating Work Graph effect is high-risk under SmartDeny (or an explicit Unrestricted break-glass policy).
- Approvals bind exact job/node/effect hash and do not transfer.
- Approved `RunCommand` runs under a documented capability envelope (cwd, env sanitisation, workspace identity); residual host-escape risk is product-visible.
- Renderer project state is never filesystem authority.

### Control-plane modularity

- Kernel owns turn + provider + tool dispatch + session projection only.
- Eval/replay/trace, gateway/cron, and similar operator services are separate crates with public APIs.

### Multi-agent

- ≥1 registered specialist and ≥1 executed workflow path through Work Graph + SmartDeny + cancel tree + handoff artifact.
- EM agent count ≥ 1 with tests.

### Observability / eval

- Every terminal turn has a reconstructible causal chain from stores (not only logs).
- Offline integrity suite is a merge gate for kernel/runtime/packs.

### UI

- One default install binary story; legacy shell optional.
- Frozen IPC contract tested on host HTTP + Electron preload for critical methods.

### Doc / claim hygiene

- system-overview, this marks file, scorecard, and install scripts agree.
- Planned never graded as Confirmed.

### Release / parity

- Version and ledger gates remain fail-closed.
- Architecture S+++ does **not** require claiming Hermes parity early.

## Program phases

See [s-plus-trust-spine.md](../plans/s-plus-trust-spine.md).

| Phase | Focus | Marks moved | Status |
|---|---|---|---|
| 0 | Truth freeze | Doc | done |
| 1 | Trust spine (policy, session repair, cancel honesty) | Security, Durability | done |
| 2 | Kernel waist extraction (`optimus-eval`, `optimus-ops`) | Control-plane | done (partial; agents/artifacts remain) |
| 3 | One multi-agent vertical (`workspace_writer` + `write_file_handoff`) | Multi-agent | done |
| 4 | One shell matrix + IPC contract checker | UI | done |
| 5 | Causal observability (`optimus trace`, denial codes, obs gate) | Observability | done |
| 6–7 | Breadth + permanent hygiene | All hold under growth | pending |
