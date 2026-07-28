---
knowledge_type: plan
status: current
owns:
  - docs/plans/reliability-autonomy-program.md
watches:
  - crates/optimus-policy/**
  - crates/optimus-runtime/src/lib.rs
  - crates/optimus-graph/src/lib.rs
  - apps/optimus-ui/src/components/workbench/Composer.tsx
  - docs/decisions/0044-bounded-project-trust-and-capability-broker.md
  - docs/maps/security-and-approvals.md
covers:
  - docs/plans/reliability-autonomy-program.md
depends_on:
  - docs/plans/product-complete-program.md
  - docs/decisions/0031-safe-project-work-loop.md
  - docs/decisions/0035-command-capability-envelope.md
  - docs/decisions/0044-bounded-project-trust-and-capability-broker.md
validated_by:
  - crates/optimus-policy/src/lib.rs
  - crates/optimus-runtime/tests/project_trust_profile.rs
  - scripts/check-crate-layers.py
last_verified_commit: null
---

# Reliability and autonomy program — program P30+

**Execution authority** for post–product-complete reliability: turn Optimus from
“technically capable behind a permission wall” into “describe the outcome;
complete and verify the work,” without discarding exact-effect audit.

Architecture S+++ (P10–P19) and product-complete (P20–P29) remain **hold
constraints**. This program does not demote marks for product speed.

## Naming planes

| Plane | Token | Authority |
|---|---|---|
| Program | **program** `P30`, `P31`, … | **this document** |
| Decision | `ADR-NNNN` | `docs/decisions/` |
| Delivery | `PR #N` / `pr/N-…` | GitHub |
| Grade / mark | architecture hold only | `architecture-marks.md` |

## Product promise

Install → connect a model once → choose a project → describe the outcome →
Optimus completes and verifies. The user provides outcomes, not step-by-step
permission for ordinary project files and tests.

**Invariant:** record every exact action; do not require the user to approve
every exact action when a Standard project trust profile already authorizes it.

## Phase map

| Phase | Goal | Status |
|---|---|---|
| **program P30** | Capability broker + autonomy profiles + Standard project trust auto-authorize with exact receipts | **in progress — prerequisite of program P40** |
| **program P31** | Same-run continuation after approval/restart | **parked** |
| **program P32** | Structured failure taxonomy + recovery coordinator | **parked** |
| **program P33** | Capability snapshot + layered readiness + first-run smoke | **parked** |
| **program P34** | Checkpoint/rollback manifests before broader auto-permission | **parked** |
| **program P35** | Activity/error UI polish + release packaging residuals | **parked** |

**Parked (2026-07-29):** the primary roadmap authority is
[github-engineer-program.md](./github-engineer-program.md) (program P40–P46)
until GITHUB-ENGINEER-V1. Program P30's remaining microtasks (R30.5–R30.8) are
carried as prerequisites of program P40 and still land in this wave; P31–P35
resume afterwards. Parking is a sequencing decision, not a cancellation.

Advanced breadth (full PTY I/O, live CUA, Hermes gate, messaging depth) stays
**after** the successful-task loop is strong.

## program P30 — Capability broker + Standard trust

**Decision:** [ADR-0044](../decisions/0044-bounded-project-trust-and-capability-broker.md)

### Microtasks

| ID | Status | Item |
|---|---|---|
| R30.1 | **done** | ADR-0044 accepted |
| R30.2 | **done** | `crates/optimus-policy` profiles, capabilities, broker |
| R30.3 | **done** | Runtime SmartDeny gate uses broker; trust-profile exact grants |
| R30.4 | **done** | Composer autonomy labels; Standard first; map IPC access |
| R30.5 | **done** | Durable project trust grant store (outside repo); applied at `open_dev_run_session` only |
| R30.6 | **done** | Structured package-manager capabilities (`optimus-policy::command_class`) |
| R30.7 | pending | Owned-localhost network lease |
| R30.8 | pending | Product release defaults (Auto provider/model) without breaking offline tests |

R30.4 is marked done for the profile plumbing; [#118](https://github.com/mustbearnold/Optimus-Agent/issues/118)
tracks the part of it that did not land — the access menu still offers full
host authority first, which is the opposite of "Standard first".

**What R30.5 deliberately does not do.** A grant is read in exactly one place:
`Kernel::open_dev_run_session`. A chat session on a trusted project still asks,
because "I authorized this project for engineering runs" and "stop showing me
edits" are different statements and only the first one was made. Widening this
to every session is a decision for a later ADR, not a convenience.

**What R30.6 changes about a decision, not just a label.** `cargo test` and
`cargo install ripgrep` were the same request — `ProcessProjectExecute`,
`Externality::ProjectLocal` — so a project-scoped grant covered both. The
classifier splits sync (reproduces a lockfile a human already committed) from
add (chooses something new, reaches a registry) from host install (writes
outside the project at all, and answers to `SystemModify`).

### Exit gate (P30)

- `cargo test -p optimus-policy` — 21 (14 unit + 7 `command_classification`)
- `cargo test -p optimus-kernel --test dev_run_trust` — 6
- `cargo test -p optimus-runtime --test project_trust_profile`
- `cargo test -p optimus-runtime --test approvals_surface`
- `python3 scripts/check-crate-layers.py`
- Security map updated for trust-profile grants
- Review changes still pauses high-risk effects (ADR-0031 behaviour preserved)

Not exit-gated yet: R30.7 and R30.8 remain open, so P30 stays **in progress**.

### Explicit non-claims (P30)

- Same-run multi-step model continuation after approval (P31)
- Signed auto-updater
- Hermes `gate` PASS
- LLM-as-permission-authority

## Rules

1. Spine reuse: Work Graph → broker → exact terminal; no second approval plane.
2. Autonomy ≠ containment (ADR-0035).
3. Project config cannot self-grant outside-project or unrestricted authority.
4. One primary program phase per PR when possible.
5. Hold architecture marks S+++.

## Immediate next action

1. Land **program P30** broker + Standard trust (this wave), including
   R30.5–R30.8 as prerequisites of
   [program P40](./github-engineer-program.md).
2. Then **program P40**, not P31. P31–P35 resume after GITHUB-ENGINEER-V1.
