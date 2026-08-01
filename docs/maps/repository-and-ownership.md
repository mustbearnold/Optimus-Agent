---
doc_id: repository-and-ownership-map
doc_type: reference
plane: current
status: current
authority: supporting
summary: Supporting map of Optimus domain boundaries and application surfaces; the executable repository component database owns package inventory, lifecycle, distribution, and removal truth.
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: repository-map
owns:
  - Cargo.toml
  - apps/optimus-tui/Cargo.toml
  - crates/optimus-host/Cargo.toml
  - crates/optimus-policy/Cargo.toml
  - apps/optimus-cli/Cargo.toml
  - apps/optimus-desktop/Cargo.toml
  - crates/optimus-browser/Cargo.toml
  - crates/optimus-graph/Cargo.toml
  - crates/optimus-kernel/Cargo.toml
  - crates/optimus-memory/Cargo.toml
  - crates/optimus-packs/Cargo.toml
  - crates/optimus-runtime/Cargo.toml
  - crates/optimus-skills/Cargo.toml
  - crates/optimus-store/Cargo.toml
  - crates/optimus-ops/Cargo.toml
  - crates/optimus-artifacts/Cargo.toml
  - crates/optimus-agent/Cargo.toml
  - crates/optimus-workflow/Cargo.toml
  - crates/optimus-eval/Cargo.toml
  - docs/repository-components.json
watches:
  - apps/**/src/**
  - crates/**/src/**
covers:
  - Cargo.toml
  - apps/optimus-tui/Cargo.toml
  - crates/optimus-host/Cargo.toml
  - crates/optimus-policy/Cargo.toml
  - apps/optimus-cli/Cargo.toml
  - apps/optimus-desktop/Cargo.toml
  - crates/optimus-browser/Cargo.toml
  - crates/optimus-graph/Cargo.toml
  - crates/optimus-kernel/Cargo.toml
  - crates/optimus-memory/Cargo.toml
  - crates/optimus-packs/Cargo.toml
  - crates/optimus-runtime/Cargo.toml
  - crates/optimus-skills/Cargo.toml
  - crates/optimus-store/Cargo.toml
  - crates/optimus-ops/Cargo.toml
  - crates/optimus-artifacts/Cargo.toml
  - crates/optimus-agent/Cargo.toml
  - crates/optimus-workflow/Cargo.toml
  - crates/optimus-eval/Cargo.toml
  - docs/repository-components.json
depends_on:
  - README.md
validated_by:
  - scripts/test_engineering_memory.py
last_verified_commit: 09fddbc1b60a6b37f9f80680988ea5036a9b8eec
---

# Repository and ownership map

## Audit basis and package ownership

**Confirmed current behaviour:** Rust and npm package identity is derived from
their manifests. Component meaning, distribution, lifecycle, common confusion,
generated-output destination, and removal conditions are governed by the
[repository component authority](../repository-components.md) and rendered in
the generated [component wiki](../COMPONENTS.md). This supporting map does not
repeat those tables because a second handwritten inventory inevitably drifts.

## Domain modularity (P13 / ADR-0036)

**Confirmed current behaviour:**

| Plane | Owner | Must not |
|---|---|---|
| Tool identity | `optimus-packs::ToolDesc` | Second catalog in kernel/surfaces |
| Session transcript | `SessionStore` | Authorize host effects |
| Semantic memory | `optimus-memory` | `ActionAuthorize` / live capability grants |
| Procedural skills | `optimus-skills` | Expand closed permissions; grant wrong effect class |
| Work Graph jobs | `optimus-store` / graph / runtime | Own chat UI schema |
| Engineering Memory | repo docs / EM scripts | Load as runtime authorization |

**Gate:** `python3 scripts/check-domain-modularity.py` and
`cargo test -p optimus-kernel --test domain_modularity`.

## Application surfaces

### TUI

**Confirmed current behaviour:** bare `optimus` (no subcommand) opens the
`optimus-tui` terminal face against the chosen home. Turns run through
`optimus_host::chat_turn_cancellable` on a worker thread with streamed text,
footer tool status, cancellation, provider pick/connect, transcript scrolling,
and in-transcript exact approval resolution via `chat_approval_resolve`.

### CLI

**Confirmed current behaviour:** commands cover doctor, demo/resume, skills,
packs, offline/live chat, sessions, Codex auth, cron, HTTP browser, approvals,
jobs, gateway, built-in eval, and campaigns.

### Desktop

**Confirmed current behaviour:** the Wry/Tao shell uses WebKitGTK on Linux and
WebView2 on Windows. It serves one inline UI document through Wry IPC at the
platform custom-protocol URL (`optimus://localhost/` on Linux and
`http://optimus.localhost/` on Windows). A separate loopback HTTP mode exists for
Playwright and browser testing. The IPC method registry and its domain modules
(system, sessions, scheduling, runtime, files, chat, OS) now live in
`crates/optimus-host` (ADR-0045); the desktop binary is a transport over them. Linux user installation and
desktop registration are owned by `scripts/rebuild-install-relaunch.sh` and the
XDG data/bin/application/icon locations it manages. Electron plus React is the
default Linux install; the Windows rebuild/install path still uses Wry, so the
Wry UI is rollback-only but not yet safe to remove.

**Confirmed current behaviour:** `apps/optimus-electron` is the default
repository-level shell and owns the context-isolated preload, host
authentication, foreground stream controller, native preview view, bounded
preview annotations, window actions, and native-view overlay lifecycle.
`apps/optimus-ui` owns the React presentation, Codex-measured token layer,
responsive panel composition, local project `rootPaths[]` catalog,
session-to-project grouping, and layout/theme/density state. None of that local
project state grants Rust filesystem access.

## Current ownership boundaries

- **Confirmed:** provider adapters and the turn loop belong to `optimus-kernel`.
- **Confirmed:** tool identity/schema/policy/pack availability belong to
  `optimus-packs`; kernel dispatch must not create a second catalog.
- **Confirmed:** durable effects and approvals belong to `optimus-runtime` plus
  graph/store; surfaces should not execute high-risk model effects directly.
- **Confirmed:** runtime semantic memory belongs to `optimus-memory`.
- **Confirmed:** runtime procedural skills belong to `optimus-skills`.
- **Confirmed:** the Rust host and Wry rollback belong to
  `apps/optimus-desktop`; the default Electron transport boundary belongs to
  `apps/optimus-electron`; React presentation and local multi-folder grouping
  belong to `apps/optimus-ui`. Domain behavior remains in Rust libraries.
- **Confirmed:** `optimus-agent` / `optimus-workflow` own typed agent/workflow
  contracts, registries, invocation evidence, and DAG verticals; kernel owns
  canonical routing and telemetry, sessions, execution/trace production paths,
  and re-exports the peels. Offline evaluation/baselines live in `optimus-eval`.
- **Confirmed:** built-in specialists and registered DAG verticals are owned by
  `optimus-agent` + `optimus-workflow` (kernel re-exports).
- **Unknown/unresolved:** model-chosen specialist routing, OTLP/OpenTelemetry
  export (local causal export exists — ADR-0037), or GPU adapters.

## Top-level domains

**Confirmed current behaviour:** every tracked top-level domain and every
immediate app, crate, evaluation suite, and developer skill must exist in the
component database. `evals/` contains reproducible definitions; generated
results belong in `Development/`. Adding an unclassified domain is a red gate.

## Adapted Engineering Memory structure

```text
AGENTS.md
OPTIMUS_AGENTS.md
scripts/engineering_memory.py
scripts/test_engineering_memory.py
skills/update-engineering-memory/SKILL.md
docs/
  architecture/system-overview.md
  engineering-memory/README.md
  maps/
    repository-and-ownership.md
    memory-and-retrieval.md
    model-routing.md
    security-and-approvals.md
    observability-and-evaluations.md
  contracts/high-risk-contracts.md
  decisions/0017-engineering-memory-separation.md
  decisions/0026-separate-development-and-runtime-agents.md
  lessons/ai-agent-mistakes.md
  plans/engineering-memory-phases.md
.engineering-memory/
  repository-index.json
  agent-registry.json
  tool-registry.json
  workflow-registry.json
  prompt-registry.json
  model-registry.json
  dependency-graph.json
  source-to-test-map.json
  contract-coverage.json
  evaluation-coverage.json
  change-impact.json
  knowledge-staleness.json
```

`AGENTS.md` is development-only. `OPTIMUS_AGENTS.md` is the product runtime
constitution injected into Optimus chat system prompts.
Instructions about how coding agents should develop this repository remain in
the development plane; they are not product requirements and must not be copied
into runtime prompts, routing, permissions, or approval defaults without an
explicit product-change request.
**Confirmed current behaviour:** generated repository identity is the sorted
source-record `tree_sha256`; UTF-8 text uses canonical LF and binary bytes remain
exact. Generated maps deliberately exclude ambient Git
commit, branch, worktree, and remote metadata so identical indexed bytes produce
identical authority in a checkout or source archive.

The JSON files are generated and factual. Human interpretation remains in
versioned Markdown with coverage frontmatter.
