---
doc_id: decisions-0035-command-capability-envelope
doc_type: decision
plane: decision
status: current
authority: record
summary: Decision record for ADR-0035: Command capability envelope + Unrestricted break-glass (P12), including its context, consequences, and current documentary status.
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: decision
covers:
  - crates/optimus-runtime/src/command_envelope.rs
  - crates/optimus-runtime/src/lib.rs
  - crates/optimus-graph/src/lib.rs
  - crates/optimus-kernel/src/network_policy.rs
  - crates/optimus-kernel/src/product_settings.rs
  - crates/optimus-kernel/src/browser.rs
  - crates/optimus-kernel/src/web_search.rs
depends_on:
  - docs/decisions/0003-phase1-policy-budgets.md
  - docs/decisions/0018-fail-closed-runtime-safety.md
  - docs/decisions/0020-work-graph-integrity-and-loopback-security.md
  - docs/decisions/0027-settings-driven-work-isolation.md
  - docs/decisions/0031-safe-project-work-loop.md
validated_by:
  - crates/optimus-runtime/tests/command_envelope.rs
  - crates/optimus-runtime/tests/path_confinement.rs
  - crates/optimus-runtime/tests/approvals_surface.rs
  - crates/optimus-runtime/src/command_envelope.rs
  - crates/optimus-kernel/src/network_policy.rs
---

# ADR-0035: Command capability envelope + Unrestricted break-glass (P12)

- **Status:** Accepted
- **Date:** 2026-07-25

## Context

SmartDeny and exact-effect approvals already gate host-mutating Work Graph
effects. File effects resolve through a retained `cap-std` workspace directory
capability. **Approved `RunCommand` / `ProjectRunCommand` did not:** on Linux
the process tree was owned via `systemd-run`, but bwrap used `--bind / /`, so
an approved shell retained full host filesystem write reach. That residual kept
Security boundary design at **A-** and Multi-agent readiness at interim **S**.

PolicyMode::Unrestricted auto-grants approvals for tests/break-glass but must
not be confused with “full host FS for commands.”

## Decision

1. **`CommandFsEnvelope` is orthogonal to `PolicyMode`.**
   - `PolicyMode` decides whether high-risk effects wait for SmartDeny grants.
   - `CommandFsEnvelope` decides how far an **approved** (or Unrestricted-auto)
     command may reach on the host.
2. **Default envelope is `Confined`.**
   - **Linux:** bwrap profile with workspace as the **only** host path bound
     read-write; system trees (`/usr`, `/bin`, `/lib*`, `/etc`, optional
     `/opt`/`/nix`) ro-bind when present; no full-root `--bind / /`; optional
     `--unshare-net` for `ConfinedNoNetwork`.
   - **Windows:** Job Object process-tree ownership remains; FS residual is
     product-visible for `Confined`. `ConfinedNoNetwork` **fail-closes** until
     AppContainer (or equivalent) exists.
3. **`UnrestrictedHost` is explicit break-glass** for command FS (full host
   bind on Linux). It is never the product default. Operators must set it on
   `RuntimeConfig` (or a future product control); it is distinct from
   `PolicyMode::Unrestricted` (approval auto-grant).
4. **Product settings map isolation → envelope:**
   - `shared` / `project_bound` → `Confined`
   - `isolated_profiles` → `ConfinedNoNetwork`
   Kernel loads `settings.json` when `KernelConfig.command_fs_envelope` is
   unset.
5. **Shared egress policy** for browser + web_search lives in
   `optimus_kernel::network_policy` (`assert_public_http_url`). Provider TLS
   adapters may remain adapter-local when documented.

## Consequences

- Positive: approved shells cannot write outside the runtime workspace under
  default Linux Confined; residual is either closed or explicit break-glass.
- Positive: Multi-agent interim command-FS residual for write/confined paths is
  closed → Multi-agent mark may re-grade to **S+++** with registered
  specialists only (still no open-ended model-spawned agents).
- Negative: Confined Linux children cannot read unbound host trees (e.g. other
  projects under a sibling directory) — intentional.
- Negative: Windows FS confinement for commands is still Job Object residual
  under `Confined`; stronger modes fail closed rather than fake a sandbox.
- Negative: RO system binds still allow **reading** some system paths; S+++
  claims writable confinement + SmartDeny, not a full air-gap.

## Alternatives considered

- **Landlock-only without bwrap.** Rejected for now: bwrap already in the
  path; Landlock can complement later.
- **Always remount workspace at `/workspace`.** Rejected: breaks absolute
  paths in agent scripts; same-path bind preferred.
- **Collapse UnrestrictedHost into PolicyMode::Unrestricted.** Rejected:
  tests need approval auto-grant without host FS free-for-all.

## Risks

- Missing ro-binds for exotic toolchains (e.g. tools only under `/home/...`).
  Mitigate by documenting Confined as system-package + workspace, or operator
  UnrestrictedHost for break-glass.
- systemd-run user session unavailable: spawn fails closed (existing).

## Conditions for reconsideration

- When Windows AppContainer (or equivalent) lands, relax
  `ConfinedNoNetwork` fail-closed and re-document residual.
- If product needs selectable network policy for Confined without isolation
  profile change, promote network into an independent config field.

## Documentation completion addendum (2026-07-31)

## Reasons

The decision makes the invariant in the Decision section explicit and testable. It is preferred because the failure described in Context cannot be managed reliably through prompt convention or caller discipline alone.

## Evaluation evidence

- `crates/optimus-runtime/tests/command_envelope.rs`
- `crates/optimus-runtime/tests/path_confinement.rs`
- `crates/optimus-runtime/tests/approvals_surface.rs`
- `crates/optimus-runtime/src/command_envelope.rs`
- `crates/optimus-kernel/src/network_policy.rs`

## Relevant code

- `crates/optimus-runtime/src/command_envelope.rs`
- `crates/optimus-runtime/src/lib.rs`
- `crates/optimus-graph/src/lib.rs`
- `crates/optimus-kernel/src/network_policy.rs`
- `crates/optimus-kernel/src/product_settings.rs`
- `crates/optimus-kernel/src/browser.rs`
- `crates/optimus-kernel/src/web_search.rs`

## Relevant tests

- `crates/optimus-runtime/tests/command_envelope.rs`
- `crates/optimus-runtime/tests/path_confinement.rs`
- `crates/optimus-runtime/tests/approvals_surface.rs`
- `crates/optimus-runtime/src/command_envelope.rs`
- `crates/optimus-kernel/src/network_policy.rs`
