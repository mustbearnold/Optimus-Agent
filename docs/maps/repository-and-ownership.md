---
knowledge_type: repository-map
status: current
owns:
  - Cargo.toml
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
watches:
  - apps/**/src/**
  - crates/**/src/**
covers:
  - Cargo.toml
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
depends_on:
  - README.md
validated_by:
  - scripts/test_engineering_memory.py
last_verified_commit: 09fddbc1b60a6b37f9f80680988ea5036a9b8eec
---

# Repository and ownership map

## Audit basis

**Confirmed current behaviour (P16):** this is a Rust 2021 workspace with Rust
1.85 as the declared minimum. `cargo metadata --no-deps` reports **fifteen**
workspace packages: libraries `optimus-store`, `optimus-graph`,
`optimus-runtime`, `optimus-memory`, `optimus-skills`, `optimus-packs`,
`optimus-ops`, `optimus-artifacts`, `optimus-agent`, `optimus-workflow`,
`optimus-kernel`, `optimus-eval`, `optimus-browser`; applications `optimus-cli`
and `optimus-desktop`. Electron (`apps/optimus-electron`) and React UI
(`apps/optimus-ui`) are npm packages outside Cargo metadata but are the default
desktop shell.

**Confirmed current behaviour:** the repository is a Git checkout on `main` with
GitHub `origin`; Engineering Memory records both commit identity and deterministic
file/tree SHA-256 values.

**Confirmed current behaviour:** a standalone Leptos CSR experiment exists at
`spikes/001-leptos-wry-csr`. It declares its own workspace and is not a member of
the root Cargo workspace.

## Package ownership

| Package | Kind | Owns | Direct local dependencies |
|---|---|---|---|
| `optimus-store` | library | SQLite Work Graph projections, approvals, ordered events | none |
| `optimus-graph` | library | Job/node/effect domain and transition helpers | store |
| `optimus-runtime` | library | Job execution, SmartDeny, process bounds/capture, crash resume, campaigns | graph, store, skills |
| `optimus-memory` | library | Evidence-native runtime memory and temporal recall | none |
| `optimus-skills` | library | Runtime procedural-skill lifecycle and permission closure | none |
| `optimus-packs` | library | **Sole** `ToolDesc` / pack catalog, operational metadata, capability budgets | none |
| `optimus-ops` | library | Operator gateway delivery authority and cron schedule store | none |
| `optimus-artifacts` | library | Content-addressed artifact store | none (serde/sha2/fs2 only) |
| `optimus-agent` | library | Specialist contracts, registry, invocation ledger | packs, runtime, graph |
| `optimus-workflow` | library | Workflow contracts, run ledger, built-in DAG verticals | agent, artifacts, packs, runtime, graph |
| `optimus-kernel` | library | Model/tool turn loop, sessions, execution/trace, routing, credentials; re-exports agent/workflow/artifacts/ops; **no second tool catalog** | graph, runtime, memory, skills, packs, ops, agent, workflow, artifacts |
| `optimus-eval` | library | Offline integrity/trajectory eval, evaluation reports, fixture replay | kernel, graph, runtime, memory, packs |
| `optimus-browser` | library | CDP browser backend (optional agent tools; not the Electron preview view) | (see crate Cargo.toml) |
| `optimus-cli` | binary | Headless/operator command surface and loopback gateway HTTP | kernel, eval, graph, runtime, skills, packs |
| `optimus-desktop` | binary | Rust host (`--host-only` for Electron) + Legacy Wry shell, native IPC, HTTP test harness | kernel, graph, runtime, packs |
| `apps/optimus-electron` | npm app | Default Electron shell, preload, preview `WebContentsView` | host IPC |
| `apps/optimus-ui` | npm app | React workbench presentation (no FS authority) | Electron transport |

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

### CLI

**Confirmed current behaviour:** commands cover doctor, demo/resume, skills,
packs, offline/live chat, sessions, Codex auth, cron, HTTP browser, approvals,
jobs, gateway, built-in eval, and campaigns.

### Desktop

**Confirmed current behaviour:** the native Wry/Tao shell uses WebKitGTK on Linux
and WebView2 on Windows. It serves one inline UI document through Wry IPC at the
platform custom-protocol URL (`optimus://localhost/` on Linux and
`http://optimus.localhost/` on Windows). A separate loopback HTTP mode exists for
Playwright and browser testing. IPC ownership is split into system, sessions,
scheduling, runtime, files, chat, and OS modules. Linux user installation and
desktop registration are owned by `scripts/rebuild-install-relaunch.sh` and the
XDG data/bin/application/icon locations it manages.

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

## Missing top-level domains

**Confirmed current behaviour:** there are no root `agents/`, `workflows/`,
`tools/`, `prompts/`, `evals/`, `fixtures/`, or `packages/` directories. Their
absence is not proof the concepts are absent: tools are in
`optimus-packs`; general workflow and agent contracts live in `optimus-workflow`
and `optimus-agent` (kernel re-exports);
execution remains in jobs/campaigns/cron/gateway; prompts are inline.

**Planned behaviour:** add a top-level domain only when it has an implemented,
typed artifact that cannot live clearly in the established Rust package. Do not
restructure the whole repository to imitate a template.

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
**Confirmed current behaviour:** generated repository identity is the sorted
source-record `tree_sha256`; UTF-8 text uses canonical LF and binary bytes remain
exact. Generated maps deliberately exclude ambient Git
commit, branch, worktree, and remote metadata so identical indexed bytes produce
identical authority in a checkout or source archive.

The JSON files are generated and factual. Human interpretation remains in
versioned Markdown with coverage frontmatter.
