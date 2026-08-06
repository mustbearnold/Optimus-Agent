---
doc_id: architecture-system-overview
doc_type: explanation
plane: current
status: current
authority: canonical
summary: This document describes the repository as it exists now. The historical blueprint in optimus-exceeds-hermes.md remains useful product direction, but it contains planned components and is not proof of implementation.
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: architecture
owns:
  - Cargo.toml
  - crates/optimus-kernel/src/lib.rs
  - crates/optimus-runtime/src/lib.rs
  - crates/optimus-packs/src/lib.rs
  - crates/optimus-memory/src/lib.rs
  - crates/optimus-store/src/lib.rs
  - crates/optimus-graph/src/lib.rs
  - crates/optimus-skills/src/lib.rs
  - apps/optimus-cli/src/main.rs
  - apps/optimus-desktop/src/main.rs
  - specs/004-runtime-effects/spec.md
watches:
  - apps/optimus-cli/src/**
  - apps/optimus-desktop/src/**
  - crates/*/src/**
covers:
  - Cargo.toml
  - crates/optimus-kernel/src/lib.rs
  - crates/optimus-runtime/src/lib.rs
  - crates/optimus-packs/src/lib.rs
  - crates/optimus-memory/src/lib.rs
  - crates/optimus-store/src/lib.rs
  - crates/optimus-graph/src/lib.rs
  - crates/optimus-skills/src/lib.rs
  - apps/optimus-cli/src/main.rs
  - apps/optimus-desktop/src/main.rs
depends_on:
  - docs/decisions/0017-engineering-memory-separation.md
  - docs/decisions/0020-work-graph-integrity-and-loopback-security.md
  - docs/decisions/0026-separate-development-and-runtime-agents.md
  - docs/decisions/0031-safe-project-work-loop.md
  - docs/decisions/0032-engineering-memory-compact-lenses.md
validated_by:
  - crates/optimus-kernel/tests/kernel_turn.rs
  - crates/optimus-runtime/tests/cancellation.rs
  - apps/optimus-cli/tests/gateway_http.rs
  - apps/optimus-desktop/e2e/03-runtime-and-sessions.spec.js
last_verified_commit: 09fddbc1b60a6b37f9f80680988ea5036a9b8eec
---

# Optimus Agent system overview

This document describes the repository as it exists now. The historical
blueprint in `optimus-exceeds-hermes.md` remains useful product direction, but
it contains planned components and is not proof of implementation.

## Status legend

- **Confirmed current behaviour** — observed in source or tests.
- **Inferred behaviour** — a bounded architectural interpretation.
- **Planned behaviour** — a target without a complete implementation.
- **Unknown or unresolved behaviour** — evidence or a settled contract is
  missing.

## Instruction planes

**Confirmed current behaviour:** Optimus has two deliberately separate root
instruction surfaces:

| Surface | Audience | Runtime loading |
|---|---|---|
| `AGENTS.md` | Humans and coding agents developing Optimus | Never injected into product chat |
| `OPTIMUS_AGENTS.md` | Installed Optimus product sessions | Embedded by `optimus-kernel` |

Development requests about autonomy, orchestration, model/reasoning selection,
VCS, testing, or reporting remain in the development plane. They do not alter
product prompts, permission defaults, routing, or approval behaviour unless the
user explicitly requests a product/runtime change.

`crates/optimus-kernel/src/system_prompt.rs` constructs the product system
message from `OPTIMUS_AGENTS.md` and has regression coverage excluding the
development-only body. ADR-0026 owns this boundary. A selected third-party
project may contribute task-local project instructions; those remain distinct
from both Optimus root surfaces.

## Current topology

**Confirmed current behaviour**

```text
CLI, legacy Wry Desktop, or Tauri React workbench
        (bounded preload -> authenticated loopback Rust host)
                         |
                         v
          provider selection at each surface
                         |
                         v
       optimus-kernel::Kernel turn/tool loop
          |          |          |          |
          v          v          v          v
       packs       memory      skills    sessions + effect links
          |                                  |
          v                                  v
 canonical ToolDesc                    SQLite transcript
          |
          v
 optimus-runtime durable jobs ----> optimus-graph state machine
          |                                  |
          +--------------------------> optimus-store SQLite ledger
```

**Inferred behaviour:** the kernel is the current control-plane waist because
both implemented user surfaces construct it and it assembles providers, packs,
memory, skills, sessions, and durable effects. There is no separately
implemented `optimus-control-plane` or `optimus-orchestrator` package.

## Applications and packages

| Component | State | Current responsibility |
|---|---|---|
| `apps/optimus-cli` | Confirmed current behaviour | CLI for jobs, approvals, skills, packs, chat, sessions, auth, cron, browser, gateway, evals, and campaigns. It also hosts a loopback webhook gateway. |
| `apps/optimus-desktop` | Confirmed current behaviour | Rust host (`--host-only`) for the Tauri shell + legacy Wry/Tao shell (WebKitGTK / WebView2), frozen IPC registry, bounded worker queues, inline legacy UI, and loopback HTTP. |
| `apps/optimus-tauri` | Confirmed current behaviour | **Default installed** React shell; Tauri commands bridge the frozen IPC registry, bounded chat streams with cancellation, window chrome, and native folder selection; embeds the built workbench. |
| `apps/optimus-ui` | Confirmed current behaviour | React 19 workbench; typed `DesktopMethod` transport (Tauri bridge); multi-folder presentation with Rust scope authority; Browser surface drives the kernel `browser_*` effector. |
| `crates/optimus-kernel` | Confirmed current behaviour | Provider-agnostic turn loop, strict tool dispatch, sessions, execution manifests, credential protection, canonical routing, browser/search effectors, and filesystem sandbox. Re-exports agent/workflow/artifacts/ops for surfaces. |
| `crates/optimus-agent` | Confirmed current behaviour | Versioned specialist descriptors, immutable registry, durable invocation/cancellation/retry/terminal ledger, effect provenance links. |
| `crates/optimus-workflow` | Confirmed current behaviour | Workflow definitions/registry, durable DAG `WorkflowRunStore`, built-in specialist verticals and registered-definition executor. |
| `crates/optimus-artifacts` | Confirmed current behaviour | Content-addressed handoff/workbench artifact store under `{home}/artifacts`. |
| `crates/optimus-ops` | Confirmed current behaviour | Operator services: durable local gateway delivery authority and cron schedule store. Kernel re-exports for surface convenience; does not own the turn loop. |
| `crates/optimus-eval` | Confirmed current behaviour | Offline integrity/trajectory harnesses, versioned evaluation reports/baselines, and zero-effect fixture replay. Depends on kernel; kernel does not depend on eval. |
| `crates/optimus-browser` | Confirmed current behaviour | Optional CDP browser backend for agent tools and the workbench Browser surface. |
| `crates/optimus-packs` | Confirmed current behaviour | Canonical pack/tool descriptors, provider-visible input schemas, tool policy/invocation identity, availability, validation, and schema-token budgets. |
| `crates/optimus-runtime` | Confirmed current behaviour | Durable ordered jobs, effect intents/receipts, bounded command execution, exact-action SmartDeny approvals, cancellation, crash recovery, output capture, and leased ordered campaigns. |
| `crates/optimus-graph` | Confirmed current behaviour | Job/node/effect domain and state-transition helpers. |
| `crates/optimus-store` | Confirmed current behaviour | Versioned SQLite jobs, nodes, exact-action approval decisions, cancellation requests, effect attempts, atomic transitions, quarantine state, and ordered append-only events. |
| `crates/optimus-memory` | Confirmed current behaviour | SQLite evidence-native claim ledger, bitemporal correction, scoped recall, non-authorizing FTS5 free-text recall with per-hit standing, conflict sets, injected monotonic clock, sensitivity/allowed-use gates, retention, tombstone/privacy erase, sanitized audit events, and evidence packets. |
| `crates/optimus-skills` | Confirmed current behaviour | SQLite versioned procedural-skill registry with closed permissions, outcome counts, promotion, pinning, and deprecation. |

## Architecture documents

The sections above are the current-state orientation. Detail lives in
focused documents under `docs/architecture/`:

- `control-plane-and-workflows.md` — control plane, workflow runtime,
  terminal outcomes, agent execution.
- `tools-and-modularity.md` — tool system contract and domain modularity
  (P13 / ADR-0036).
- `state-and-memory.md` — state/persistence, memory/retrieval, model routing.
- `security-and-observability.md` — security/approvals, events/observability/
  replay, GPU/CPU fallback, architectural debt.
- `desktop-ops.md` — desktop daily use, run/build/streaming, durability and
  backup, doctor, crash/resume operator notes.
- `exceeds-blueprint.md` — the historical north-star blueprint (planned components; not proof of implementation). Its **Optimus rule** (no module > ~800 LOC without a forced split) is enforced by the ADR-0049 module-size ratchet.
- `versioning-and-parity.md` — versioning, release/parity gates, SOTA
  scorecard, honest status.
