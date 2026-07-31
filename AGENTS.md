# Optimus Agent engineering rules

This file contains repository-wide **development** laws only. It is for humans
and coding agents working on the Optimus source tree.

It is intentionally separate from the product runtime constitution:

| File | Audience | Loaded into Optimus chat? |
|---|---|---|
| `AGENTS.md` | Developers / coding agents building Optimus | No |
| `OPTIMUS_AGENTS.md` | Installed Optimus Agent product sessions | Yes |

Detailed procedures live under `docs/` and `skills/`. Start every documentation
lookup at `docs/README.md`; it routes current status, roadmap, architecture,
operations, decisions, and history without making agents scan the whole tree.
Start every development turn with `just orient`; use `just explain-path <path>`
before guessing whether a directory ships, is development-only, or is removable.

## Instruction-plane firewall (mandatory)

A request about **how a coding agent should develop Optimus** is not a product
requirement. Development instructions about autonomy, orchestration, model
selection, reasoning effort, permissions, tools, VCS, testing, or reporting
govern the agents changing this repository only. Never copy them into product
prompts, policy defaults, UI behaviour, or runtime capabilities unless the user
explicitly asks to change Optimus Agent itself.

Interpret these common requests precisely:

| User request | Plane | Meaning |
|---|---|---|
| “Work autonomously on Optimus” | Development | Make progress in the assigned worktree without repeatedly asking for routine choices. |
| “Use agents/models appropriate to the task” | Development | The primary coding agent selects and orchestrates bounded engineering subtasks. |
| “Make Optimus more autonomous” | Product | Change runtime behaviour, with source, tests, docs, and safety review. |
| “Change how Optimus asks for approval” | Product | Change the product policy/UX, not the coding-agent permission model. |

When wording is ambiguous, preserve the existing product behaviour and treat
the request as development-process guidance. A product change needs explicit
product/runtime intent and executable evidence.

## Hard project boundary (mandatory)

When developing Optimus Agent, work is confined to the Optimus project tree.

### Canonical root

- Workspace wrapper: `/home/mustbearn/Projects/Optimus Agent`
- Clean landed-repository view: `/home/mustbearn/Projects/Optimus Agent/Repository`
- Development happens only in an assigned linked worktree under
  `/home/mustbearn/Projects/Optimus Agent/Development/worktrees/`.
- `Repository/` is the complete reproducible GitHub repository: product source,
  tests, evaluation definitions, documentation, and build logic. It is a
  detached, clean view of remote `main`; never develop there.
  The wrapper's `local` and `.git` compatibility links resolve into
  `Development/` and exist only for older automation.
- Resolve both the repository and active worktree with `readlink -f` / `pwd -P`
  before editing. Compare resolved paths, never remembered aliases.
- If the active workspace is not an assigned Optimus worktree, **stop**. Never
  edit the bare repository root or another worktree directly.

### Allowed write scope

Only create/modify/delete files under:

1. The assigned worktree under
   `/home/mustbearn/Projects/Optimus Agent/Development/worktrees/**` (source, docs,
   skills, scripts, and worktree-local build/evidence)
2. Optimus install/runtime paths **only when the task explicitly requires
   install, relaunch, uninstall, or live desktop verification**:
   - `~/.local/share/optimus-agent/**`
   - `~/.local/share/optimus/**`
   - `~/.local/share/applications/optimus-agent.desktop`
   - `~/.local/share/icons/**/optimus-agent.*`
   - `~/.local/bin/optimus` and `~/.local/bin/optimus-cli` when they are
     Optimus-managed symlinks
3. `/home/mustbearn/Projects/Optimus Agent/Development/land/**` only through
   `just checkpoint`, `just undo`, or `just land`; it holds locks, immutable
   task receipts, private checkpoint records, and verification evidence.

### Forbidden without explicit user instruction naming that other target

Do **not** edit, reorganize, install into, or “clean up”:

- Sibling projects under `~/Projects/` (for example
  `Hermes Next`, `Heracles Agent`, `i-have-adhd`, `spicybrowse`, or any future
  non-Optimus folder). Optimus shares `/mnt/Projects/` with several unrelated
  trees, so being on the same disk grants nothing.
- Other application source trees under `~/`, `~/Projects/`, `/mnt/Projects/`, or
  elsewhere
- Hermes product/config trees used for Hermes itself (for example
  `~/.hermes/**`) except read-only inspection when needed for Optimus import
  compatibility
- Other agents’ repos, websites, business projects, or shared utilities outside
  the Optimus root
- Global system packages, user services, or shell config unless the user
  explicitly asked for that host change as part of the Optimus task

### Enforcement checklist (every Optimus development turn)

1. Run `just orient` and confirm the workspace resolves to the assigned linked
   worktree under the canonical repository.
2. Before any write/patch/rm, assert the target path is inside that assigned
   worktree or an explicitly allowed Optimus install path above.
3. Refuse cross-project drive-by fixes. If another project is implicated, report
   the path and ask; do not touch it.
4. Keep build artifacts, evidence, and temp outputs inside the assigned
   worktree or the Optimus `Development/` plane rather than other projects.
5. Treat path containment as a hard gate equal to “do not land unverified work”.

## Naming planes (mandatory — humans and coding agents)

Identifiers from **different planes are never interchangeable**. Coding agents
must enforce this on every branch name, commit subject, ADR, issue, plan
microtask, and grade claim.

| Plane | Token | Authority |
|---|---|---|
| Decision | `ADR-NNNN` | `docs/decisions/` |
| Program | `P##` (historical program phase) | `docs/plans/**`; no program document overrides the current roadmap in `docs/current/roadmap.md`. Always say **program P##** in historical prose. |
| Plan / microtask | plan-local (`M*`, `C*`, `S*`…) | owning `docs/plans/**` (e.g. full-app `S*.*`) |
| Delivery | full Git commit SHA on `origin/main` | remote `refs/heads/main` |
| Grade / mark | mark + grade (`S+++`, `A-`…) | `docs/architecture/architecture-marks.md` |
| Runtime product | `id@version` / crate / pack | source contracts, SemVer, EM |

**Hard gates**

1. `P12` ≠ a delivery SHA ≠ `ADR-0012` ≠ grade `S+++`. Never “align” identifiers
   across planes.
2. ADRs are monotonic and permanent; never renumber or invent without scanning
   `docs/decisions/`.
3. Commits are **emoji-first Conventional Commits**.
4. Program phase may appear in commit text (`program P21 …`, `S+++ P12 …`);
   delivery identity is the full landed commit SHA. Historical product specs
   named `phase-20*` are **not** program P20.
5. Grades move only with source + tests + docs exit criteria — not because a
   commit landed or a phase label was applied. Product ledger rows are not
   grades.
6. Runtime product ids are not program phases or ADR numbers.
7. A temporary or feature branch is never delivery. Only the SHA read back from
   remote `refs/heads/main` proves completion.

**Canonical detail:** [`docs/contributing/artifact-naming.md`](docs/contributing/artifact-naming.md)

If a proposed name collapses two planes, **stop and rename** before commit.

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
21. No module exceeds 800 production lines. Enforced as a ratchet by
    `scripts/check-module-size.py`: new files must comply, and the 14
    grandfathered modules in `docs/architecture/module-size-baseline.json` may
    only shrink. Never add a baseline entry by hand — split the file instead.
    Production lines exclude every `#[cfg(test)]` item and bare `mod x;`
    declaration, so inline coverage is never penalised and splitting a module
    never pushes its declaring file over the ratchet (ADR-0049).

## Evidence and status

- Label architecture claims as **Confirmed current behaviour**, **Inferred
  behaviour**, **Planned behaviour**, or **Unknown or unresolved behaviour**.
- Source code and executable tests outrank prose. ADRs preserve decisions and
  history; do not rewrite them to hide superseded reasoning.
- Do not claim specialist agents, model routing, cancellation, replay, GPU, or
  project integrations exist unless their real implementation and tests exist.
- Generated Engineering Memory identity is the content-addressed source tree
  hash. Do not invent commit SHAs as generated self-identity. Git commits remain
  external delivery/provenance evidence only.

## Development workflow

0. Gates run through `just`, never as hand-typed command lists. `just check` is
   the inner loop and `just verify` is the complete land gate.
   `scripts/verify.sh` is the single source of truth — add new gates there, not
   to a hook, workflow, or doc. Managed `just land` is the only delivery path.
1. Identify the owning subsystem and read its Engineering Memory through lenses,
   not by dumping raw `.engineering-memory/*.json` into prompts.
2. Inspect current source, related tests, contracts, and ADRs.
3. Establish a reproducible baseline.
4. Before installed-Desktop or live-model testing, load and follow
   `skills/optimus-native-ui-testing/SKILL.md`; native DOM/CUA evidence is
   primary and deterministic tests are supplementary.
5. Make the smallest coherent change; preserve unrelated work.
6. Test focused behaviour, then relevant integration/evaluation surfaces.
   When *building or extending a test layer* — not when adding a case to an
   existing one — first verify what the current best practice and tooling are
   **on the date the work is being done**, by search rather than from memory,
   and state where the existing suite sits against that bar. A model's
   knowledge has a cutoff; an inherited practice decays silently, and a suite
   that is green because it is old is the self-serving green the north-star
   criteria ban. Check the maturity of anything new before depending on it,
   and record the finding with sources in the managed local task/provenance record so the next pass
   can see what was checked and when.
7. Review security, approval, cancellation, terminal outcomes, observability,
   replay implications, and CPU fallback where applicable.
8. Run `just docs-check`. If a current or planned document or one of its source
   bindings changed, review only the reported document ids, run
   `just docs-refresh <doc-id>...`, and regenerate the deterministic catalog
   with `just docs-generate`. Generation never acknowledges semantic review.
9. Run `just em-check` before refreshing Engineering Memory.
10. If changed/stale, run `just em-context` and update only owned knowledge from
   that pack.
11. Run `just em-generate` only when warming or rebuilding the disposable local
    cache is useful; generated JSON is not delivery state.
12. Run full `python3 scripts/engineering_memory.py validate` before
    delivery/release; report known gaps via `report`.
13. VCS changes go only through the managed delivery commands below. Publishing,
    installing, or deploying outside that Git delivery remains task-scoped.

## Managed autonomous delivery (mandatory)

Standing delivery contract (owner instruction, 2026-07-31):

- Never run `gh` or use pull requests, issues, or GitHub workflow ceremony.
- Never run raw history-changing Git commands, including `git commit`,
  `git push`, `git merge`, `git rebase`, `git stash`, or `git reset`. Read-only
  `git status`, `git diff`, `git log`, and `git show` remain allowed.
- Work only in the assigned isolated worktree. Save recoverable progress with
  `just checkpoint <label>`.
- Finish only through
  `just land <task-id> --model <model> --effort <level>`. `land` alone runs the
  affected gates and fixtures, fast-forwards main, and generates the commit
  message and provenance record.
- The primary agent owns routine orchestration: derive a stable task id from the
  task, assess difficulty, select an actually available model and reasoning
  effort, and delegate bounded subtasks when useful. Do not ask the user to make
  these routine implementation choices, and never record a model or effort that
  did not produce the work.
- If `land` refuses, fix the reported gate/fixture and retry, or use
  `just undo <label>`. Never bypass a red land with raw Git.
- If `checkpoint`, `land`, or `undo` is unavailable or cannot express the
  required operation, leave the verified work in the assigned worktree and
  report the tooling limitation. Do not improvise with raw Git.
- Delivery means the SHA that `land` places on `origin/main`. Worktree changes,
  checkpoints, and feature branches are not completion.

## Repository conventions

- Rust workspace truth comes from `Cargo.toml` and `cargo metadata`.
- `optimus-packs::ToolDesc` is the canonical implemented tool contract.
- Durable effects go through `optimus-runtime`; do not bypass SmartDeny.
- Runtime memory (`optimus-memory`), session state, skills, project knowledge,
  retrieval indexes, and Engineering Memory are distinct systems.
- `.engineering-memory/` is an ignored disposable cache generated by
  `scripts/engineering_memory.py`; never edit or commit its JSON.
- Prefer Engineering Memory lenses (`context`, `impact`, `owner`, `tools`,
  `report`, `stat`) over loading whole generated maps into model context.
- `AGENTS.md` is a developer control artifact. `OPTIMUS_AGENTS.md` is the product
  runtime constitution. The installed Optimus runtime must never mutate either.
- Naming planes are mandatory for coding agents (see Naming planes above).
  `docs/contributing/artifact-naming.md` defines identity; the managed delivery
  policy above controls all VCS delivery.
- New reusable procedures belong in a focused repository skill. Keep this file
  concise.

## Agent skills

Provider-specific skills are optional accelerators, never repository
requirements. Any Codex, Claude, or other coding-agent skill remains subordinate
to this file, the assigned-worktree boundary, and managed delivery. Do not add
provider installation ceremony to routine tasks.

### Task records

The managed land system owns task and provenance records. Repository development
does not use GitHub issues or pull requests as an execution or delivery plane.

### Domain docs

Single-context: one root `CONTEXT.md`, with ADRs in **`docs/decisions/`** rather
than the skills' default `docs/adr/`. ADR front matter is mandatory. See
`docs/agents/domain.md`.
