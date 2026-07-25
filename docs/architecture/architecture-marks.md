---
knowledge_type: architecture
status: current
owns:
  - docs/architecture/architecture-marks.md
  - docs/plans/s-plus-trust-spine.md
  - docs/plans/s-plus-plus-plus-program.md
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
  - docs/plans/s-plus-plus-plus-program.md
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
| Security boundary design | **S+++** | SmartDeny high-risk effects; exact grants; path preflight; skill class grants. Linux commands: confined bwrap (workspace-only RW; no full-root bind) under default `CommandFsEnvelope::Confined`; `UnrestrictedHost` is explicit break-glass. Windows: Job Object residual for Confined; `ConfinedNoNetwork` fail-closed. Shared egress helper for browser/web_search. Credential encryption residual is not a runtime authorization hole (documented). |
| Domain modularity (packs/memory/skills/store) | **S+++** | Single `ToolDesc` authority in packs; kernel dispatches via `ToolInvocation` only. Memory/session/skills/EM planes separated with fail-closed ActionAuthorize + class-scoped skill grants. Store has no chat schema. Gate: `check-domain-modularity.py` + `domain_modularity` tests. |
| Control-plane modularity | **S+++** | Peels: `optimus-eval`, `optimus-ops`, `optimus-agent`, `optimus-workflow` (defs+DAG+verticals), `optimus-artifacts`. Kernel turn waist with re-exports. Layer lint: `scripts/check-crate-layers.py`. Residual: HTTP browser facade in kernel; CDP in `optimus-browser`. |
| Multi-agent readiness | **S+++** | Two specialists (`workspace_writer`, `workspace_reader`); three registered workflows including `write_then_read_handoff` DAG; durable `WorkflowRunStore`; parent cancel tree. P12 closed the command-FS residual that blocked S+++ after P10. Still registered-only (no open-ended model spawn — out of P10 scope). |
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

**Two bars:**

1. **Foundation floor** (below) — minimum achieved by Phases 0–5. Meeting a floor
   does **not** by itself justify S+++ if residual structural holes remain.
2. **Adversarial S+++** — full criteria live in
   [s-plus-plus-plus-program.md](../plans/s-plus-plus-plus-program.md). Marks may
   only move to **S+++** when that plan’s per-dimension criteria and exit gate
   are met in source, tests, and docs.

### Durability (foundation floor)

- Crash at any phase yields exactly one terminal outcome.
- Effect receipts and session/tool transcript either share a transaction or have deterministic **repair on open**.
- Resume never invents success for `running` work.

### Security (foundation floor)

- Every host-mutating Work Graph effect is high-risk under SmartDeny (or an explicit Unrestricted break-glass policy).
- Approvals bind exact job/node/effect hash and do not transfer.
- Approved `RunCommand` runs under a documented capability envelope (cwd, env sanitisation, workspace identity); residual host-escape risk is product-visible.
- Renderer project state is never filesystem authority.

### Control-plane modularity (foundation floor)

- Kernel owns turn + provider + tool dispatch + session projection only.
- Eval/replay/trace, gateway/cron, and similar operator services are separate crates with public APIs.

### Multi-agent (foundation floor)

- ≥1 registered specialist and ≥1 executed workflow path through Work Graph + SmartDeny + cancel tree + handoff artifact.
- EM agent count ≥ 1 with tests.

### Observability / eval (foundation floor)

- Every terminal turn has a reconstructible causal chain from stores (not only logs).
- Offline integrity suite is a merge gate for kernel/runtime/packs.

### UI (foundation floor)

- One default install binary story; legacy shell optional.
- Frozen IPC contract tested on host HTTP + Electron preload for critical methods.

### Doc / claim hygiene (foundation floor)

- system-overview, this marks file, scorecard, and install scripts agree.
- Planned never graded as Confirmed.

### Release / parity (foundation floor)

- Version and ledger gates remain fail-closed.
- Architecture S+++ does **not** require claiming Hermes parity early.

## Program phases

**Foundation (done):** [s-plus-trust-spine.md](../plans/s-plus-trust-spine.md).

**Active S+++ climb (lowest grade → highest):** [s-plus-plus-plus-program.md](../plans/s-plus-plus-plus-program.md).

| Phase | Focus | Marks moved | Status |
|---|---|---|---|
| 0 | Truth freeze | Doc | done |
| 1 | Trust spine (policy, session repair, cancel honesty) | Security, Durability | done |
| 2 | Kernel waist extraction (`optimus-eval`, `optimus-ops`) | Control-plane | done (partial; agents/artifacts remain) |
| 3 | One multi-agent vertical (`workspace_writer` + `write_file_handoff`) | Multi-agent | done |
| 4 | One shell matrix + IPC contract checker | UI | done |
| 5 | Causal observability (`optimus trace`, denial codes, obs gate) | Observability | done |
| P10 | Multi-agent platform (DAG, ≥2 specialists, cancel tree) | Multi-agent B→**S** (S+++ after P12) | **done** |
| P11 | Control-plane peels (agent/workflow/artifacts crates) | Control-plane B+→**S+++** | **done** |
| P12 | Command capability envelope (real FS confinement) | Security A-→**S+++**; Multi-agent S→**S+++** | **done** |
| P13 | Domain modularity audit (single catalogs, plane separation) | Domain A-→**S+++** | **done** |
| P14 | Observability export + gate strength | Observability A-→S+++ | pending |
| P15 | UI/IPC completeness + shell truth | UI A-→S+++ | pending |
| P16 | Doc / claim hygiene pass | Doc A-→S+++ | pending |
| P17 | Release / parity gate completeness | Release A→S+++ | pending |
| P18 | Durability chaos + multi-DB doctor/backup contract | Durability A+→S+++ | pending |
| P19 | All-marks adversarial review board | **All S+++** | pending |
