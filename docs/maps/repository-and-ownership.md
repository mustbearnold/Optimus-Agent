---
knowledge_type: repository-map
status: current
covers:
  - Cargo.toml
  - apps/**/Cargo.toml
  - crates/**/Cargo.toml
  - apps/**/src/**
  - crates/**/src/**
depends_on:
  - README.md
validated_by:
  - scripts/test_engineering_memory.py
last_verified_commit: 09fddbc1b60a6b37f9f80680988ea5036a9b8eec
---

# Repository and ownership map

## Audit basis

**Confirmed current behaviour:** this is a Rust 2021 workspace with Rust 1.85 as
the declared minimum. `cargo metadata --no-deps` reports nine workspace
packages: seven libraries and two applications. The desktop application is not
a default workspace member, but it is a workspace member.

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
| `optimus-packs` | library | Canonical tool/pack descriptor, operational metadata, and capability budgets | none |
| `optimus-kernel` | library | Model/tool turn loop, agent/workflow contracts, execution/replay/trace manifests, routing telemetry, versioned evaluation, credentials, and high-level operator services | graph, runtime, memory, skills, packs |
| `optimus-cli` | binary | Headless/operator command surface and loopback gateway HTTP | kernel, graph, runtime, skills, packs |
| `optimus-desktop` | binary | Wry/Tao desktop shell, native IPC, UI, HTTP test harness | kernel, graph, runtime, packs |

## Application surfaces

### CLI

**Confirmed current behaviour:** commands cover doctor, demo/resume, skills,
packs, offline/live chat, sessions, Codex auth, cron, HTTP browser, approvals,
jobs, gateway, built-in eval, and campaigns.

### Desktop

**Confirmed current behaviour:** the native shell serves one inline UI document
through a custom protocol and communicates through Wry IPC. A separate loopback
HTTP mode exists for Playwright and browser testing. IPC ownership is split into
system, sessions, scheduling, runtime, files, chat, and OS modules.

## Current ownership boundaries

- **Confirmed:** provider adapters and the turn loop belong to `optimus-kernel`.
- **Confirmed:** tool identity/schema/policy/pack availability belong to
  `optimus-packs`; kernel dispatch must not create a second catalog.
- **Confirmed:** durable effects and approvals belong to `optimus-runtime` plus
  graph/store; surfaces should not execute high-risk model effects directly.
- **Confirmed:** runtime semantic memory belongs to `optimus-memory`.
- **Confirmed:** runtime procedural skills belong to `optimus-skills`.
- **Confirmed:** desktop transport and presentation belong to
  `apps/optimus-desktop`; domain behavior should remain in libraries.
- **Confirmed:** `optimus-kernel` owns typed agent/workflow contracts,
  registries/adapters, invocation evidence, canonical routing and telemetry,
  execution/replay/trace contracts, and versioned offline evaluation/baselines.
- **Unknown/unresolved:** no package owns built-in specialist definitions,
  specialist routing/general workflow execution, OpenTelemetry, or GPU adapters.

## Missing top-level domains

**Confirmed current behaviour:** there are no root `agents/`, `workflows/`,
`tools/`, `prompts/`, `evals/`, `fixtures/`, or `packages/` directories. Their
absence is not proof the concepts are absent: tools are in
`optimus-packs`/kernel; general workflow and agent contracts are kernel modules;
execution remains in jobs/campaigns/cron/gateway; prompts are inline.

**Planned behaviour:** add a top-level domain only when it has an implemented,
typed artifact that cannot live clearly in the established Rust package. Do not
restructure the whole repository to imitate a template.

## Adapted Engineering Memory structure

```text
AGENTS.md
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

The JSON files are generated and factual. Human interpretation remains in
versioned Markdown with coverage frontmatter.
