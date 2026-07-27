---
knowledge_type: verification
status: current
owns:
  - docs/architecture/reliability-autonomy-p30-verification.md
covers:
  - crates/optimus-policy/src/lib.rs
  - crates/optimus-runtime/src/lib.rs
  - crates/optimus-runtime/tests/project_trust_profile.rs
  - docs/decisions/0044-bounded-project-trust-and-capability-broker.md
depends_on:
  - docs/plans/reliability-autonomy-program.md
  - docs/decisions/0044-bounded-project-trust-and-capability-broker.md
validated_by:
  - crates/optimus-policy/src/lib.rs
  - crates/optimus-runtime/tests/project_trust_profile.rs
  - crates/optimus-runtime/tests/approvals_surface.rs
  - crates/optimus-runtime/tests/phase1_policy_budget.rs
  - scripts/check-crate-layers.py
last_verified_commit: null
---

# program P30 verification — bounded project trust

Planes: **program P30** · decision **ADR-0044** · delivery pending · mark hold S+++

Date: 2026-07-26

## Goal

Introduce a deterministic capability broker and Standard project trust profile so
ordinary project mutations auto-authorize with exact-effect receipts, without
replacing SmartDeny as the pause mechanism or CommandFsEnvelope as containment.

## Results

| Item | Result | Evidence |
|---|:---:|---|
| ADR-0044 | **PASS** | `docs/decisions/0044-bounded-project-trust-and-capability-broker.md` |
| `optimus-policy` broker + profiles | **PASS** | `cargo test -p optimus-policy` (5) |
| Standard auto-allow project write/cmd | **PASS** | `project_trust_profile` |
| Review changes still pauses | **PASS** | `project_trust_profile` + `approvals_surface` |
| Read only denies mutate | **PASS** | `project_trust_profile` |
| Classic SmartDeny default path | **PASS** | `phase1_policy_budget`, `approvals_surface` |
| Crate layers | **PASS** | `check-crate-layers.py` |
| Composer autonomy labels | **PASS** (source) | `Composer.tsx`; vitest needs local `npm ci` |
| Desktop IPC access map | **PASS** (compile) | `chat.rs` + `cargo check -p optimus-desktop --no-default-features` |

## Hold suite run

```bash
cargo test -p optimus-policy
cargo test -p optimus-runtime --test project_trust_profile
cargo test -p optimus-runtime --test approvals_surface
cargo test -p optimus-runtime --test phase1_policy_budget
python3 scripts/check-crate-layers.py
```

## Live install proof (2026-07-26)

Candidate: rebuilt release install at `~/.local/share/optimus-agent`.
Home: `~/.local/share/optimus`. Model: **gpt-5.6-sol** (Codex OAuth).

### A. Installed CLI

| Scenario | Result |
|---|---|
| `--access standard` write_file | **PASS** — file written, job Succeeded, `trust_profile:standard` |
| `--access review_changes` write_file | **PASS** — `NeedsApproval`, file absent |

Evidence: `local/tmp/cua-evidence/p30-gpt-sol/`.

### B. Installed Electron UI (Playwright + DOM)

Path: live `optimus-desktop` window via CDP `127.0.0.1:9333` →
`optimus-app://ui/index.html`. **DOM only (no chat IPC).**

| Check | Result |
|---|---|
| Composer + New thread + Send | **PASS** |
| UI settings | **5.6 Sol / Low**, **Autonomy: Standard** |
| Workspace file `ui_pw_1785021720877.txt` | **`UI_STANDARD_TRUST_OK`** |
| Job | `write:ui_pw_…` **succeeded** |
| Auto grant | `trust_profile:standard` |

Evidence: `local/tmp/cua-evidence/p30-user-ui/` (`LEDGER-USER-UI.md`, `pw-*.png`,
`pw-user-drive.mjs`).

## Residuals (not P30 exit blockers for broker slice)

- Durable project trust grant store outside repo (R30.5)
- Structured package-manager capabilities (R30.6)
- Owned-localhost leases (R30.7)
- Product Auto provider/model default without breaking offline tests (R30.8)
- Same-run continuation (program P31)

## Non-claims

- Hermes gate PASS
- Unrestricted host as recommended default
- Checkpoint/rollback manifests (P34)
- First-run smoke product readiness (P33)
