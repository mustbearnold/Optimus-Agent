# Optimus Agent engineering rules

This file contains repository-wide **development** laws only. It is for humans
and coding agents working on the Optimus source tree.

It is intentionally separate from the product runtime constitution:

| File | Audience | Loaded into Optimus chat? |
|---|---|---|
| `AGENTS.md` | Developers / coding agents building Optimus | No |
| `OPTIMUS_AGENTS.md` | Installed Optimus Agent product sessions | Yes |

Detailed procedures live under `docs/` and `skills/`. For current architecture,
start with `docs/architecture/system-overview.md`.

## Hard project boundary (mandatory)

When developing Optimus Agent, work is confined to the Optimus project tree.

### Canonical root

- Absolute root: `/home/mustbearnold/Projects/Optimus Agent`
- Resolve with `readlink -f` / `pwd -P` before editing.
- If the active workspace is not this root (or a path inside it), **stop**.
  Switch to the Optimus project first. Do not “helpfully” edit elsewhere.

### Allowed write scope

Only create/modify/delete files under:

1. `/home/mustbearnold/Projects/Optimus Agent/**` (source, docs, skills, scripts,
   local build/evidence under this tree)
2. Optimus install/runtime paths **only when the task explicitly requires
   install, relaunch, uninstall, or live desktop verification**:
   - `~/.local/share/optimus-agent/**`
   - `~/.local/share/optimus/**`
   - `~/.local/share/applications/optimus-agent.desktop`
   - `~/.local/share/icons/**/optimus-agent.*`
   - `~/.local/bin/optimus` and `~/.local/bin/optimus-cli` when they are
     Optimus-managed symlinks

### Forbidden without explicit user instruction naming that other target

Do **not** edit, reorganize, install into, or “clean up”:

- Sibling projects under `~/Projects/` (for example `Hermes Next`,
  `Heracles Agent`, `i-have-adhd`, or any future non-Optimus folder)
- Other application source trees under `~/`, `~/Projects/`, or elsewhere
- Hermes product/config trees used for Hermes itself (for example
  `~/.hermes/**`) except read-only inspection when needed for Optimus import
  compatibility
- Other agents’ repos, websites, business projects, or shared utilities outside
  the Optimus root
- Global system packages, user services, or shell config unless the user
  explicitly asked for that host change as part of the Optimus task

### Enforcement checklist (every Optimus development turn)

1. Confirm workspace root is `/home/mustbearnold/Projects/Optimus Agent`.
2. Before any write/patch/rm, assert the target path is inside that root or an
   explicitly allowed Optimus install path above.
3. Refuse cross-project drive-by fixes. If another project is implicated, report
   the path and ask; do not touch it.
4. Keep build artifacts, evidence, and temp outputs under this repo
   (`local/tmp/**` preferred) rather than other project directories.
5. Treat path containment as a hard gate equal to “do not commit unless asked”.

## Naming planes (mandatory — humans and coding agents)

Identifiers from **different planes are never interchangeable**. Coding agents
must enforce this on every branch name, commit subject, PR title, ADR, issue,
plan microtask, and grade claim.

| Plane | Token | Authority |
|---|---|---|
| Decision | `ADR-NNNN` | `docs/decisions/` |
| Program | `P##` (program phase) | **Active:** `docs/plans/product-complete-program.md` (program P20–P29). Historical S+++: `docs/plans/s-plus-plus-plus-program.md` (P10–P19 done). Always say **program P##** in prose. |
| Plan / microtask | plan-local (`M*`, `C*`, `S*`…) | owning `docs/plans/**` (e.g. full-app `S*.*`) |
| Delivery | `PR #N` · local `pr/N-slug` | GitHub + `scripts/github_pr_branch.py` |
| Grade / mark | mark + grade (`S+++`, `A-`…) | `docs/architecture/architecture-marks.md` |
| Runtime product | `id@version` / crate / pack | source contracts, SemVer, EM |

**Hard gates**

1. `P12` ≠ `PR #12` ≠ `ADR-0012` ≠ grade `S+++`. Never “align” numbers across planes.
2. GitHub assigns PR numbers. Do not choose a PR number or force local
   `pr/12-…` because the program phase is P12.
3. After a PR opens: local branch **must** be `pr/<N>-…`; remote head stays
   `wip/…` (renaming remote head **closes** the PR).
4. ADRs are monotonic and permanent; never renumber or invent without scanning
   `docs/decisions/`.
5. Commits and PR titles are **emoji-first Conventional Commits**; labels are
   emoji + `namespace:value` (see contributing docs).
6. Program phase may appear in title **text** (`program P21 …`, `S+++ P12 …`);
   delivery number appears only as `PR #N` / `pr/N-…`. Historical product specs
   named `phase-20*` are **not** program P20.
7. Grades move only with source + tests + docs exit criteria — not because a
   PR merged or a phase label was applied. Product ledger rows are not grades.
8. Runtime product ids are not program phases or ADR numbers.

**Canonical detail:** [`docs/contributing/artifact-naming.md`](docs/contributing/artifact-naming.md)  
**GitHub mechanics:** [`docs/contributing/github-conventions.md`](docs/contributing/github-conventions.md)

If a proposed name collapses two planes, **stop and rename** before commit/PR.

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
   the inner loop; `just verify` is the full gate and is what `pre-push` runs.
   `scripts/verify.sh` is the single source of truth — add new gates there, not
   to a doc. Run `just setup-hooks` once per clone.
1. Identify the owning subsystem and read its Engineering Memory through lenses,
   not by dumping raw `.engineering-memory/*.json` into prompts.
2. Inspect current source, related tests, contracts, and ADRs.
3. Establish a reproducible baseline.
4. Before installed-Desktop or live-model testing, load and follow
   `skills/optimus-native-ui-testing/SKILL.md`; native DOM/CUA evidence is
   primary and deterministic tests are supplementary.
5. Make the smallest coherent change; preserve unrelated work.
6. Test focused behaviour, then relevant integration/evaluation surfaces.
7. Review security, approval, cancellation, terminal outcomes, observability,
   replay implications, and CPU fallback where applicable.
8. Run `just em-check` before refreshing memory.
9. If changed/stale, run `just em-context` and update only owned knowledge from
   that pack.
10. Run `just em-generate` (generate + quick validate).
11. Run full `python3 scripts/engineering_memory.py validate` before
    merge/release; report known gaps via `report`.
12. Do not commit, push, publish, install, or deploy unless explicitly asked.

## Repository conventions

- Rust workspace truth comes from `Cargo.toml` and `cargo metadata`.
- `optimus-packs::ToolDesc` is the canonical implemented tool contract.
- Durable effects go through `optimus-runtime`; do not bypass SmartDeny.
- Runtime memory (`optimus-memory`), session state, skills, project knowledge,
  retrieval indexes, and Engineering Memory are distinct systems.
- `.engineering-memory/*.json` files are generated by
  `scripts/engineering_memory.py`; edit their source code/docs, not the JSON.
- Prefer Engineering Memory lenses (`context`, `impact`, `owner`, `tools`,
  `report`, `stat`) over loading whole generated maps into model context.
- `AGENTS.md` is a developer control artifact. `OPTIMUS_AGENTS.md` is the product
  runtime constitution. The installed Optimus runtime must never mutate either.
- Naming planes and GitHub process are mandatory for coding agents (see Naming
  planes above). Follow `docs/contributing/artifact-naming.md` and
  `docs/contributing/github-conventions.md`; do not invent alternate schemes.
- New reusable procedures belong in a focused repository skill. Keep this file
  concise.
