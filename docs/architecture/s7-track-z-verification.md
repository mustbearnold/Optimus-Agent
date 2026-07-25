---
knowledge_type: verification
status: current
owns:
  - docs/architecture/s7-track-z-verification.md
covers:
  - docs/plans/full-app-microtasks.md
depends_on:
  - docs/plans/product-complete-program.md
validated_by:
  - crates/optimus-kernel/src/profile.rs
  - crates/optimus-workflow/src/child_lease.rs
  - crates/optimus-ops/src/pty_session.rs
  - crates/optimus-ops/src/hermes_import.rs
  - crates/optimus-ops/src/surfaces.rs
  - crates/optimus-ops/src/media_fixtures.rs
  - crates/optimus-ops/src/channel_adapters.rs
  - crates/optimus-eval/src/comparative.rs
  - crates/optimus-packs/src/lib.rs
last_verified_commit: null
---

# S7 operator depth + Track Z scaffolds verification

Planes: **program residual S7 + Track Z** · delivery **PR #40** · product-complete
held · **Hermes gate NOT claimed**

Date: 2026-07-25

## Goal

Close optional operator-depth and ecosystem scaffolds with tests, without
greenwashing Hermes `optimus_version.py gate` PASS or live multi-tab ConPTY.

## S7 results

| ID | Item | Result | Evidence |
|---|---|:---:|---|
| S7.1–S7.2 | Profile homes + cross-profile deny | **PASS** | `profile.rs` |
| S7.3–S7.5 | Leased child + cancel + N≤4 | **PASS** | `child_lease.rs` + specialist DAG |
| S7.6 | Multi-tab PTY | **PARTIAL** | Linux session store; real PTY I/O residual |
| S7.7 | Computer-use pack | **PASS** | desktop pack unavailable tools scaffold |
| S7.8–S7.9 | Hermes importers | **PASS** | session/skill/memory fixture import |

## Track Z results

| ID | Item | Result | Evidence |
|---|---|:---:|---|
| Z.1 | Comparative runner | **PASS** | offline Optimus suite; Hermes `not_run` |
| Z.2 | Performance scenarios | **PARK** | structure exists; scenarios empty; gate blocked |
| Z.3 | Feature contracts first batch | **PARK** | inventory IDs required; no invented claim ids |
| Z.4–Z.6 | Proxy / TUI / ACP | **PASS** | offline scaffolds + tests |
| Z.7–Z.9 | Vision / imagegen / voice | **PASS** | offline fixtures |
| Z.10 | Breadth packs | **PASS** | home/office/devex packs |
| Z.11 | Discord/Slack | **PASS** | mock gateway enqueue |

## Residuals

| Residual | Owner |
|---|---|
| Real multi-tab PTY I/O + UI | S7.6 partial |
| Live Discord/Slack Bot APIs | after mock adapters |
| Hermes gate PASS / full 2063 contracts | Track Z performance + inventory audit |
| Live computer-use effectors | SmartDeny + heavy approval |
| `projects.scope` concurrent lease | S2.14 |
| `release.updater` signed channel | ADR-0043 residual |

## Hold suite

```bash
cargo test -p optimus-kernel --lib profile
cargo test -p optimus-workflow --lib child_lease
cargo test -p optimus-ops --lib
cargo test -p optimus-eval --lib comparative
cargo test -p optimus-packs --lib
python3 scripts/check-parity-ledger.py
python3 scripts/check-domain-modularity.py
python3 scripts/check-crate-layers.py
python3 scripts/optimus_version.py release-check
```

## Non-claims

- Hermes comparative **gate PASS**
- Full performance scenario parity
- Live PTY product UI
- Live computer-use automation
- Signed auto-updater

## Verdict

**S7 + Track Z scaffolds: PASS** with named partials (pty, updater, projects.scope)
and Hermes gate still **unverified**.
