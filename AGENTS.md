# Optimus Agent engineering rules

Operational entry point for humans and coding agents on the Optimus
source tree. Governing law: [`specs/constitution.md`](specs/constitution.md)
(highest authority), then [`specs/conventions.md`](specs/conventions.md).
Conflict order: constitution → conventions → specs → code comments.

This is a developer control artifact. The installed Optimus runtime must
never mutate it. The product runtime constitution is
[`OPTIMUS_AGENTS.md`](OPTIMUS_AGENTS.md) — loaded into Optimus chat sessions;
it is intentionally separate from this file.

## Instruction-plane firewall (mandatory)

A request about **how a coding agent should develop Optimus** is not a product
requirement. Development instructions (autonomy, orchestration, model
selection, reasoning effort, permissions, tools, VCS, testing, reporting)
govern the agents changing this repository only. Never copy them into product
prompts, policy defaults, UI behaviour, or runtime capabilities unless the
user explicitly asks to change Optimus Agent itself.

| User request | Plane | Meaning |
|---|---|---|
| "Work autonomously on Optimus" | Development | Make progress directly on `main` without repeatedly asking for routine choices. |
| "Use agents/models appropriate to the task" | Development | Primary agent selects and orchestrates bounded engineering subtasks. |
| "Make Optimus more autonomous" | Product | Change runtime behaviour, with source, tests, docs, and safety review. |
| "Change how Optimus asks for approval" | Product | Change the product policy/UX, not the coding-agent permission model. |

When wording is ambiguous, preserve the existing product behaviour and treat
the request as development-process guidance. A product change needs explicit
product/runtime intent and executable evidence.

## Main-only development (mandatory)

All work happens directly on `main` in the project root. Zero linked
worktrees, zero feature branches — enforced by `.githooks/` (`pre-commit`,
`post-checkout`, `reference-transaction`). Commits off `main` are blocked,
leaving `main` forces a return, and creating or moving any other branch is
refused. Do not attempt to bypass these hooks.

- Resolve the repository path with `readlink -f` / `pwd -P` before editing.
  If the active workspace is not the Optimus project root, stop.
- Commit directly on `main`: small, verified, emoji-first Conventional
  Commits (`<emoji> <type>(<scope>): <summary>`). `gh issue` is the task
  plane: open issues and resolve them with verified commits on `main`,
  pushed to `origin/main` — issues and commits run in parallel, local
  commits and remote issue state advancing together. Never run `gh pr`,
  pull requests, or other GitHub workflow ceremony. Never run
  history-changing Git commands. Delivery means a verified commit on
  `main` pushed to `origin/main`, closing the issue it resolves.
- Allowed write scope: the repository tree plus the Optimus install/runtime
  paths only when a task explicitly requires install, relaunch, uninstall,
  or live desktop verification. Never edit sibling projects, other agents'
  repos, global system packages, or shell config.

## Architectural laws

1. Optimus is a modular agent system, not a single giant agent prompt.
2. Each specialist agent must have a narrow, documented responsibility.
3. Agents communicate through typed inputs and outputs.
4. Deterministic work belongs in tools, not prompts.
5. Workflow state belongs to the runtime, not individual prompts.
6. Tools must not silently broaden their permissions.
7. High-risk actions require explicit approval.
8. Every workflow must define success, failure, cancellation, and retry behaviour.
9. Every long-running operation must support cancellation.
10. Every execution must produce exactly one terminal outcome.
11. Runtime events must be observable and ordered.
12. Prompts, tools, agents, workflows, and evaluations must be versioned.
13. Generated files must not be manually edited.
14. GPU acceleration must have a CPU fallback.
15. Security boundaries must be enforced by code and permissions.
16. Source-backed outputs must retain provenance.
17. Model-generated claims are not automatically trusted.
18. Bug fixes require regression tests.
19. Important architectural decisions require an ADR.
20. A feature is incomplete when its Engineering Memory is stale.
21. No module exceeds 800 production lines (ratchet, ADR-0049).

## Evidence and status

Label architecture claims as **Confirmed current behaviour**, **Inferred
behaviour**, **Planned behaviour**, or **Unknown or unresolved behaviour**.
Source code and executable tests outrank prose; ADRs are never rewritten to
hide superseded reasoning. Do not claim specialist agents, model routing,
cancellation, replay, GPU, or project integrations exist unless their real
implementation and tests exist.

## Development workflow

0. Gates run through `just`, never as hand-typed command lists. `just check`
   is the inner loop and `just verify` is the complete land gate.
   `scripts/verify.sh` is the single source of truth — add new gates there,
   not to a hook, workflow, or doc. Start every development turn with
   `just orient`; use `just explain-path <path>` before guessing whether a
   directory ships, is development-only, or is removable. Documentation
   lookups start at `specs/conventions.md` and `docs/architecture.md`.
1. Identify the owning spec (SDD loop: no code without a spec) and read its
   Engineering Memory through lenses (`just em-context`), not by dumping raw
   `.engineering-memory/*.json` into prompts.
2. Inspect current source, related tests, contracts, and ADRs.
3. Establish a reproducible baseline.
4. Before installed-Desktop or live-model testing, load and follow
   `skills/optimus-native-ui-testing/SKILL.md`; native DOM/CUA evidence is
   primary and deterministic tests are supplementary.
5. Make the smallest coherent change; preserve unrelated work.
6. Test focused behaviour, then relevant integration/evaluation surfaces.
7. Review security, approval, cancellation, terminal outcomes, observability,
   replay implications, and CPU fallback where applicable.
8. Run `just docs-check`; on staleness `just docs-refresh <ids>` +
   `just docs-generate`. Generation never acknowledges semantic review.
9. Run `just em-check` before refreshing Engineering Memory; `just em-context`
   updates owned knowledge; run full
   `python3 scripts/tools/engineering_memory.py validate` before delivery.
10. VCS changes go only through direct commits on `main`.

## Repository conventions

- Rust workspace truth comes from `Cargo.toml` and `cargo metadata`.
- `optimus-packs::ToolDesc` is the canonical implemented tool contract.
- Durable effects go through `optimus-runtime`; do not bypass SmartDeny.
- Runtime memory (`optimus-memory`), session state, skills, project
  knowledge, retrieval indexes, and Engineering Memory are distinct systems.
- `.engineering-memory/` is an ignored disposable cache generated by
  `scripts/tools/engineering_memory.py`; never edit or commit its JSON.
- `AGENTS.md` is a developer control artifact. `OPTIMUS_AGENTS.md` is the
  product runtime constitution. The installed Optimus runtime must never
  mutate either.
- Naming planes are mandatory (see `specs/constitution.md` and
  `specs/conventions.md`); `docs/decisions/` holds ADRs; `docs/runbooks/`
  holds operations.
- New reusable procedures belong in a focused repository skill. Keep this
  file concise.

## Agent skills

Provider-specific skills are optional accelerators, never repository
requirements. Any Codex, Claude, or other coding-agent skill remains
subordinate to this file, the main-only boundary, and direct-on-main delivery.

### Task records

GitHub issues (`gh issue`) are the task plane, opened and resolved by
verified commits on `main` pushed to `origin/main`; issue and commit progress
run in parallel. Pull requests remain outside the delivery plane.

### Domain docs

Single-context: one root `CONTEXT.md`, with ADRs in **`docs/decisions/`**.
ADR front matter is mandatory. See `docs/runbooks/agent-domain.md`.
