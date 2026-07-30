---
knowledge_type: verification
status: current
owns:
  - docs/architecture/reliability-autonomy-p30-verification.md
covers:
  - apps/optimus-cli/src/parsers.rs
  - apps/optimus-desktop/ui/app.js
  - apps/optimus-desktop/ui/index.html
  - apps/optimus-desktop/ui/style.css
  - apps/optimus-ui/src/components/workbench/Composer.tsx
  - apps/optimus-ui/src/state/composerStore.ts
  - crates/optimus-graph/src/lib.rs
  - crates/optimus-host/src/chat.rs
  - crates/optimus-policy/src/lib.rs
  - crates/optimus-runtime/src/lib.rs
  - crates/optimus-runtime/tests/project_trust_profile.rs
  - docs/decisions/0044-bounded-project-trust-and-capability-broker.md
  - scripts/check-autonomy-profiles.py
depends_on:
  - docs/plans/reliability-autonomy-program.md
  - docs/decisions/0044-bounded-project-trust-and-capability-broker.md
validated_by:
  - apps/optimus-desktop/e2e/02-shell-and-composer.spec.js
  - apps/optimus-ui/src/components/workbench/Composer.test.tsx
  - apps/optimus-ui/src/state/composerStore.test.ts
  - crates/optimus-host/src/chat.rs
  - crates/optimus-policy/src/lib.rs
  - crates/optimus-runtime/tests/project_trust_profile.rs
  - crates/optimus-runtime/tests/approvals_surface.rs
  - crates/optimus-runtime/tests/phase1_policy_budget.rs
  - scripts/check-crate-layers.py
  - scripts/check-autonomy-profiles.py
  - scripts/test_autonomy_profiles.py
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
| `optimus-policy` broker + profiles | **PASS** | `cargo test -p optimus-policy` (15 unit + 7 integration) |
| Standard auto-allow project write/cmd | **PASS** | `project_trust_profile` |
| Review changes still pauses | **PASS** | `project_trust_profile` + `approvals_surface` |
| Read only denies mutate | **PASS** | `project_trust_profile` |
| Classic SmartDeny default path | **PASS** | `phase1_policy_budget`, `approvals_surface` |
| Crate layers | **PASS** | `check-crate-layers.py` |
| Composer autonomy labels (both composers) | **PASS** | `Composer.test.tsx`, `check-autonomy-profiles.py` — see the correction below |
| Desktop IPC access map | **PASS** (compile) | `chat.rs` + `cargo check -p optimus-desktop --no-default-features` — the *mapping*, not the menu that feeds it; see the correction below |

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

## Correction: "Composer autonomy labels" was recorded against a tree that never existed (#118)

The row above read **PASS (source)** from this file's first writing until
2026-07-29, and it was wrong for the whole of that time. The React composer
offered `Full access` / `Ask before effects` / `Read only`, and `full` parsed
to `AutonomyProfile::UnrestrictedHost` paired with `PolicyMode::Unrestricted` —
so the *first* item of the menu handed over the host, the exact opposite of
ADR-0044 decision 7.

Which of the two failure modes it was, since the issue asked: the change was
never made. `git log -S standard -- Composer.tsx` returns nothing across the
component's whole history, so no branch lost it in a merge and no commit
reverted it. The row was recorded from reading a hold-suite screenshot whose
`Autonomy: Standard` line came from the *runtime* profile plumbing (R30.2/R30.3,
which did land) rather than from the menu the row names.

The second composer had it worse. `apps/optimus-desktop/ui/index.html` — the
Wry surface, reached through `OPTIMUS_ELECTRON_UI=legacy` and compiled into the
binary by `include_str!` — offered `SmartDeny` / `Full` / `Read-only` with
**`Full` pre-selected**, so that surface booted at unrestricted host without
anyone choosing it. The *Desktop IPC access map* row above did not catch this
and was never wrong to pass: it verifies that `chat.rs` maps an access string to
a profile and a policy, which it does correctly. Nothing verified the menu that
decides which string gets sent. A mapping is only as good as its inputs, and no
row owned the inputs.

Three things now stand behind these rows instead of a reading:

- `apps/optimus-ui/src/components/workbench/Composer.test.tsx` asserts the React
  menu offers Standard first, `unrestricted_host` last, and break-glass under an
  Expert group.
- `scripts/check-autonomy-profiles.py` holds *both* composers, their persisted
  access values, the two Rust profile parsers, and the CLI policy parser on
  every `just verify`. It proves that each menu defaults to `standard`, neither
  composer restores break-glass after restart, and only the explicit
  `unrestricted` CLI word disables effect checks. The Wry render contract is
  scoped to the live `kind === 'access'` branch, requires the same tier and
  explanatory-hint vocabulary as React, and cannot pass from matching dead
  code. Exact migration tables keep the old Wry `smart_deny` value at Review
  changes and legacy `full` at Standard. The parser recognizes explicit bare
  and quoted literal properties, then rejects computed, spread, duplicate, or
  otherwise unclassifiable table entries rather than silently omitting them.
  Wry menu extraction removes balanced HTML comments and rejects malformed
  ones, so dead markup cannot stand in for the shipped access menu. Its 37
  self-tests begin by proving the pre-fix tree of each composer fails and include
  adversarial fixtures for those bypass shapes.
- `apps/optimus-desktop/e2e/02-shell-and-composer.spec.js` checks the live Wry
  listbox order and grouping, the break-glass warning's accessible name and
  explanatory hint, and reload behavior for current and legacy persisted
  access words.
- `apps/optimus-electron/e2e/support/workbench-flow.cjs` asserts a fresh profile
  boots at `Access: Standard` through the compiled bundle, so the claim survives
  the build rather than only the source.

A verification row whose evidence column names a *file* rather than a command
that fails is worth exactly what this one was. A row that verifies a mapping is
not a row that verifies the surface feeding it, however similar the two sound.

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
