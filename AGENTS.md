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

## Repository map (mandatory orientation)

| Path | Authority |
|---|---|
| `apps/` | CLI, Rust desktop host, Electron shell, and React workbench |
| `crates/` | Kernel, agents, workflows, runtime policy, memory, packs, storage, evaluation, and browser libraries |
| `docs/architecture/` | Current architecture, marks, verification records, and safety boundaries |
| `docs/contracts/` | Cross-surface and high-risk executable contracts |
| `docs/decisions/` | Permanent monotonic ADRs |
| `docs/plans/` | Program and implementation-plan authority |
| `docs/maps/` | Repository ownership and Engineering Memory maps |
| `scripts/verify.sh` | Single source of truth for local and CI verification |
| `.github/` | CI, issue forms, PR template, and repository delivery metadata |
| `.engineering-memory/` | Generated indexes; never edit these files by hand |

Use [`docs/maps/repository-and-ownership.md`](docs/maps/repository-and-ownership.md)
for package-level ownership and
[`docs/architecture/system-overview.md`](docs/architecture/system-overview.md)
for current behaviour and program pointers.

## Hard project boundary (mandatory)

When developing Optimus Agent, work is confined to the Optimus project tree.

### Canonical root

- Absolute root: `/mnt/Projects/Optimus Agent`
- Resolve with `readlink -f` / `pwd -P` before editing. Two symlinked aliases
  resolve to that same root and are equally valid entry points —
  `~/Projects/Optimus Agent` (kept so pre-move absolute paths still work) and
  `~/2TB Projects/Optimus Agent`. Compare **resolved** paths, never literal ones:
  a string match against `~/Projects/...` no longer proves anything either way.
- If the active workspace is not this root (or a path inside it), **stop**.
  Switch to the Optimus project first. Do not “helpfully” edit elsewhere.

### Allowed write scope

Only create/modify/delete files under:

1. `/mnt/Projects/Optimus Agent/**` (source, docs, skills, scripts,
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

- Sibling projects under `~/Projects/` **or `/mnt/Projects/`** (for example
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

1. Confirm the workspace root resolves to `/mnt/Projects/Optimus Agent`.
2. Before any write/patch/rm, assert the target path is inside that root or an
   explicitly allowed Optimus install path above.
3. Refuse cross-project drive-by fixes. If another project is implicated, report
   the path and ask; do not touch it.
4. Keep build artifacts, evidence, and temp outputs under this repo
   (`local/tmp/**` preferred) rather than other project directories.
5. Treat path containment as a hard gate equal to the delivery gates below.

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
   emoji + `namespace:value` (see contributing docs). Every issue and PR carries
   at least a `type:` and an `area:` label — `github_pr_branch.py open` refuses
   without them, and `audit-labels` reports anything opened by hand.
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
   When *building or extending a test layer* — not when adding a case to an
   existing one — first verify what the current best practice and tooling are
   **on the date the work is being done**, by search rather than from memory,
   and state where the existing suite sits against that bar. A model's
   knowledge has a cutoff; an inherited practice decays silently, and a suite
   that is green because it is old is the self-serving green the north-star
   criteria ban. Check the maturity of anything new before depending on it,
   and record the finding with sources on the owning issue so the next pass
   can see what was checked and when.
7. Review security, approval, cancellation, terminal outcomes, observability,
   replay implications, and CPU fallback where applicable.
8. Run `just em-check` before refreshing memory.
9. If changed/stale, run `just em-context` and update only owned knowledge from
   that pack.
10. Run `just em-generate` (generate + quick validate).
11. Run full `python3 scripts/engineering_memory.py validate` before
    merge/release; report known gaps via `report`.
12. A user request to change, build, fix, or deliver repository work activates
    the sole Codex delivery workflow below, including its scoped commit, push,
    draft PR, review, and gated auto-merge steps. A read-only request does not.
    Install, deploy, release, live-model use, and writes outside the repository
    still require explicit authorization.

## Sole Codex delivery workflow (mandatory)

Standing owner instruction (2026-07-29): this is the **only** workflow Codex
uses for repository changes. It does not grant the installed Optimus product
new publish, effect, install, deployment, or approval authority:

```text
Focused issue → Codex plan → isolated worktree → tested change → draft PR
→ CI + Codex review → gated auto-merge → cleanup
```

1. **Focused issue.** One independently valuable outcome per issue. The issue
   must state `Goal`, `Context`, `Constraints`, and testable `Done when`
   conditions before implementation. Assign it and mark it in progress. Never
   put credentials or private information in a public issue, prompt copied to
   GitHub, PR, commit, log, or tracked environment file. Push protection is a
   backup, not permission to expose a secret.
2. **One task, plan, branch, and worktree.** One Codex task owns one issue. Load
   and follow the `caveman-optimus` skill before implementation or delivery.
   Use Plan mode first for architecture, ambiguity, or risky work; otherwise
   record an explicit task plan. Fetch `origin/main`, create `wip/<slug>`, and
   create a dedicated worktree under `local/worktrees/<slug>`. Never implement
   in the main checkout or reuse another issue's branch/worktree.
3. **Complete engineering loop.** Inspect current source, contracts, tests,
   ADRs, and Engineering Memory; establish a baseline; implement the smallest
   complete change; add regression tests; run focused checks, `just check`, EM
   gates, and `just verify`; review the full diff and secret scan before push.
   Open a draft PR after the first coherent, testable checkpoint and keep all
   follow-up fixes for that issue on the same PR.
4. **Independent review, then automation.** Request `@codex review` on the
   draft PR while CI runs. Subagents may parallelize independent read-heavy
   exploration, test execution, security review, or log analysis. Never allow
   concurrent writers on the same files. Resolve every actionable review
   finding and conversation, rerun affected checks, update the PR evidence,
   and repeat review when the diff materially changes.
5. **Gated auto-merge and terminal outcome.** A solo repository does not
   require a second human approval. Once the PR is ready, the branch is current
   and mergeable, `python3 scripts/github_pr_branch.py check` passes, required
   CI is green, Codex review is complete, actionable findings and conversations
   are resolved, no `do-not-merge` label exists, and every issue condition is
   proven, enable merge-commit automation with `gh pr merge --auto --merge`.
   Monitor until GitHub reports `MERGED`, then confirm the issue closed and
   prune the worktree/local branch. A team repository also obeys its configured
   human-approval rules. Never bypass a required gate. If a high-risk action
   still needs explicit approval or any gate remains red/unresolved, stop with
   the PR blocked and report the exact evidence.

**Definition of done:** one focused issue; one isolated issue worktree; source,
tests, ADR/docs, and generated Engineering Memory current as applicable; focused
checks plus `just verify` green; full diff and secret review complete; draft PR
reviewed by Codex; all findings and conversations resolved; required CI green;
PR auto-merged; issue closed; remote head deleted by GitHub; local worktree and
branch pruned. `MERGED` or an explicitly evidenced blocker is the only terminal
outcome—an unattended open PR is not done.

PR #117 (12 commits, +14k lines, 7 ADRs in one PR) remains the sizing
counterexample: automation increases throughput through shorter merge latency,
never by making changes incomplete or hard to review.

## Codex review rules (mandatory)

Review the whole diff against the owning issue and applicable `AGENTS.md` files.
Prioritize correctness, security/permission boundaries, data loss, cancellation,
terminal outcomes, regressions, missing tests, stale Engineering Memory, and
claims stronger than the evidence. Cite file and line for actionable findings;
do not request stylistic churn without a concrete maintenance or user impact.
Any P0/P1 finding, unresolved actionable comment, red required check, merge
conflict, or `🚫 status:do-not-merge` label blocks readiness and auto-merge.

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

## Agent skills

The `mattpocock-skills` plugin supplies the engineering workflow skills. Its
per-repo configuration lives in `docs/agents/` and is committed — these files are
read by `to-tickets`, `triage`, `to-spec`, `wayfinder`, `domain-modeling` and
`code-review`. Edit them directly rather than re-running
`/setup-matt-pocock-skills`.

The plugin itself installs per machine, not per clone:

```
claude plugin marketplace add mattpocock/skills
claude plugin install mattpocock-skills@mattpocock
```

### Issue tracker

GitHub issues on `mustbearnold/Optimus-Agent`, via the `gh` CLI. The two-plane
branch rule and the sole Codex delivery workflow override any skill with a
different branch, publication, review, or merge lifecycle. See
`docs/agents/issue-tracker.md`.

### Triage labels

The five canonical roles, each label string equal to its name. See
`docs/agents/triage-labels.md`.

### Domain docs

Single-context: one root `CONTEXT.md`, with ADRs in **`docs/decisions/`** rather
than the skills' default `docs/adr/`. ADR front matter is mandatory. See
`docs/agents/domain.md`.
