---
knowledge_type: process
status: current
owns:
  - .github/labels.yml
  - .github/pull_request_template.md
  - .github/ISSUE_TEMPLATE/**
  - scripts/sync-github-labels.py
  - scripts/github_pr_branch.py
  - docs/contributing/github-conventions.md
watches:
  - AGENTS.md
  - docs/contributing/artifact-naming.md
covers:
  - docs/contributing/github-conventions.md
depends_on:
  - docs/architecture/architecture-marks.md
  - docs/contributing/artifact-naming.md
validated_by:
  - scripts/sync-github-labels.py
  - scripts/github_pr_branch.py
  - scripts/test_github_pr_branch.py
last_verified_commit: null
---

# GitHub conventions (Optimus Agent)

This document is the **single process source** for commits, branches, PRs,
issues, and labels. Labels are defined in [`.github/labels.yml`](../../.github/labels.yml)
and synced with `python3 scripts/sync-github-labels.py`.

**Identity planes** (what `P12` vs `PR #N` vs `ADR-NNNN` mean) are defined in
[artifact-naming.md](./artifact-naming.md) and enforced in `AGENTS.md`. This
file covers **delivery mechanics only**. Coding agents must load both.

## Principles

1. **Emoji-first labels** — each label is `emoji + space + namespace:value`
   (e.g. `✨ type:feat`). Exactly one leading emoji.
2. **Namespaced tokens** — `namespace:value` (lowercase, hyphens in values).
3. **Conventional Commits** for titles and preferred commit subjects.
4. **One concern per PR** — prefer a stack of small PRs over `🟪 size:XL`.
5. **Executable evidence** outranks prose (see `AGENTS.md` status legend).
6. **Do not invent labels ad hoc** — extend `.github/labels.yml` and re-sync.
7. **Do not conflate planes** — `P##` (program) ≠ `PR #N` (delivery) ≠
   `ADR-NNNN` (decision). See [artifact-naming.md](./artifact-naming.md).

## Label format

```text
<emoji> <namespace>:<value>
```

Examples: `🐛 type:bug`, `🔀 area:workflow`, `🔥 priority:p1`,
`👀 status:needs-review`.

When filtering or scripting, match the **full label name including emoji and
space**, or match the `namespace:value` suffix.

## Label namespaces

| Namespace | Purpose | Examples | How many |
|---|---|---|---|
| `type:` | Kind of change | `✨ type:feat`, `🔧 type:fix` | **exactly one** on PRs |
| `area:` | Owning subsystem | `🔀 area:workflow`, `🖥️ area:desktop` | **≥1** (primary first) |
| `priority:` | Urgency | `🚨 priority:p0` … `⬇️ priority:p3` | issues; optional on PRs |
| `status:` | Workflow state | `👀 status:needs-review` | issues/PRs; keep current |
| `size:` | Review bulk | `▪️ size:S` … `🟪 size:XL` | **one** on PRs |
| `risk:` | Blast radius | `🔐 risk:security`, `📀 risk:data` | as applicable |
| `program:` | Initiative | `🏆 program:s+++`, `⚖️ program:parity` | when relevant |
| `process:` | Meta process | `📋 process:adr`, `🔄 process:em-refresh` | as applicable |
| `wayfinder:` | Wayfinder map/ticket kind | `🗺️ wayfinder:map`, `🔥 wayfinder:grilling` | issues; **one** per wayfinder issue |

### Emoji legend (quick)

| Namespace | Emoji map |
|---|---|
| **type** | 🐛 bug · ✨ feat · 🔧 fix · ♻️ refactor · 📝 docs · ✅ test · 🧹 chore · ⚙️ ci · ⚡ perf · 🔒 security · 🏗️ architecture |
| **area** | 🧠 kernel · 🔁 runtime · 🤖 agent · 🔀 workflow · 📦 artifacts · 🧩 memory · 🎯 skills · 📚 packs · 💾 store · 🛰️ ops · 📊 eval · 💻 cli · 🖥️ desktop · 🎨 ui · 🌐 browser · 🛡️ security · 📖 docs · 🧬 em · 🏭 ci |
| **priority** | 🚨 p0 · 🔥 p1 · ⚠️ p2 · ⬇️ p3 |
| **wayfinder** | 🗺️ map · 🔍 research · 🧪 prototype · 🔥 grilling · 🔧 task |
| **status** | 🔍 triage · 🟢 ready · 🚧 in-progress · ⛔ blocked · 👀 needs-review · ✏️ changes-requested · ✔️ approved · 🚫 do-not-merge |
| **size** | ▫️ XS · ▪️ S · ◾ M · ⬛ L · 🟪 XL |
| **risk** | 💥 breaking · 🔐 security · 📀 data · 🍃 low |
| **program** | 🏆 s+++ · ⚖️ parity |
| **process** | 📋 adr · 🔄 em-refresh · 🌱 good-first-issue · 🙋 help-wanted · 👯 duplicate · 🙅 wontfix · ❌ invalid · ❓ question |

### Minimum label set

| Artifact | Required labels |
|---|---|
| **Pull request** | one `type:*` + ≥1 `area:*` + one `size:*` |
| **Bug issue** | `🐛 type:bug` + `🔍 status:triage` (+ area after triage) |
| **Feature issue** | `✨ type:feat` + `🔍 status:triage` |
| **Architecture task** | `🏗️ type:architecture` + `🏆 program:s+++` when S+++ |

### Priority scale

| Label | Use when |
|---|---|
| `🚨 priority:p0` | Production break, security exploitability, data loss |
| `🔥 priority:p1` | Blocks the current milestone / release gate |
| `⚠️ priority:p2` | Important; not blocking the active milestone |
| `⬇️ priority:p3` | Backlog / polish |

### Status lifecycle (issues)

```text
🔍 status:triage → 🟢 status:ready → 🚧 status:in-progress
       ↘ ⛔ status:blocked
🚧 status:in-progress → 👀 status:needs-review → ✔️ status:approved → (closed)
                   ↘ ✏️ status:changes-requested → 🚧 status:in-progress
```

PRs typically use `👀 status:needs-review` / `✏️ status:changes-requested` /
`✔️ status:approved` / `🚫 status:do-not-merge`.

## Conventional Commits (emoji-first)

GitHub’s commit and PR lists show the **subject line only**. Put the type emoji
**first** so it is visible on the repo home / commits tab.

Format:

```text
<emoji> <type>(optional-scope): <imperative summary>

[optional body]

[optional footer]
```

### Types (align with `type:` labels)

| Emoji | Commit type | PR label | Meaning |
|---|---|---|---|
| ✨ | `feat` | `✨ type:feat` | User-visible capability |
| 🔧 | `fix` | `🔧 type:fix` | Bug fix |
| 📝 | `docs` | `📝 type:docs` | Docs only |
| ♻️ | `refactor` | `♻️ type:refactor` | Behaviour-preserving restructure |
| ✅ | `test` | `✅ type:test` | Tests only |
| 🧹 | `chore` | `🧹 type:chore` | Tooling, deps, housekeeping |
| ⚙️ | `ci` | `⚙️ type:ci` | CI workflows |
| ⚡ | `perf` | `⚡ type:perf` | Performance |
| 🔒 | `fix` / `feat` | `🔒 type:security` | Security (prefer `🔧 fix` or `✨ feat` + security label) |
| 🏗️ | `refactor` / `docs` | `🏗️ type:architecture` | Architecture program (use with `type:architecture` label) |
| 🐛 | `fix` | `🐛 type:bug` | Same as fix when filing bugs |
| ⏪ | `revert` | match original | Reverts a prior commit |
| 🧹 | `build` / `style` | `🧹 type:chore` | Build system / formatting only |

### Scopes (optional; align with `area:`)

Prefer short crate/app names: `kernel`, `runtime`, `workflow`, `agent`,
`desktop`, `cli`, `ui`, `eval`, `em`, `docs`, `security`.

### Subject rules

- **Leading emoji required** (exactly one, matching the type table)
- Imperative mood: “add”, “fix”, “peel” — not “added” / “adds”
- ≤72 characters (emoji counts; keep the text short)
- No trailing period
- Reference issues: `Fixes #123` / `Refs #123` in body or footer

### Examples (what you see in the commits list)

```text
✨ feat(workflow): add write-then-read handoff DAG
🔧 fix(runtime): refuse grant transfer across effect hashes
♻️ refactor(kernel): peel agent contracts into optimus-agent
📝 docs(architecture): accept ADR-0034 control-plane peels
✅ test(workflow): cover mid-DAG cancel tree
🧹 chore(em): refresh generated indexes after peel
⚙️ ci: wire check-crate-layers into PR checks
🏗️ architecture: S+++ P12 command capability envelope
```

## Branch naming (matches PR number — not program phase)

**Canonical local branch once a PR exists:**

```text
pr/<PR-number>-<short-kebab>
```

The leading digits are **always the GitHub PR number**, never the S+++ program
phase. Program phase may appear only in the **slug** for humans.

Examples (correct even when numbers diverge):

| Program | Delivery | Local branch | Remote head |
|---|---|---|---|
| P10 | PR #8 | `pr/8-p10-multi-agent-dag` | `wip/p10-multi-agent-dag` |
| P11 | PR #9 | `pr/9-p11-control-plane-peels` | `wip/p11-control-plane-peels` |
| P12 | PR #21 | `pr/21-p12-command-fs-envelope` | `wip/p12-command-fs-envelope` |
| (docs) | PR #20 | `pr/20-branch-pr-number-convention` | `wip/branch-pr-number-convention` |

Wrong: `pr/12-…` because the work is “P12” when the open PR is `#21`.

This keeps **local checkouts, worktrees, and `git branch` output aligned with
the GitHub PR number** (`#21` ↔ `pr/21-…`).

### Lifecycle

| Stage | Local branch | Remote PR head |
|---|---|---|
| Before PR | `wip/<short-kebab>` | (none / same) |
| PR opened | **`pr/<N>-<short-kebab>`** | stays `wip/<short-kebab>` (stable) |
| Merged | delete local | GitHub may auto-delete remote |

**Why local ≠ remote:** renaming or deleting the **remote** head branch closes the
open PR on GitHub. Local rename is safe and still matches the PR number in
`git branch` / worktrees.

### Recommended workflow (scripted)

```bash
# 1) Start work
git checkout main && git pull
git checkout -b wip/p12-command-fs-envelope
# … commits …

# 2) Push + open PR; rename **local** branch to pr/<N>-…
python3 scripts/github_pr_branch.py open \
  --title "🏗️ architecture: S+++ P12 command capability envelope" \
  --slug p12-command-fs-envelope \
  --label "🏗️ type:architecture" \
  --label "🛡️ area:security" \
  --label "🏆 program:s+++" \
  --label "⬛ size:L" \
  --body-file /tmp/pr-body.md
# → PR #N opened; local branch becomes pr/N-p12-command-fs-envelope
# → remote head remains wip/p12-command-fs-envelope
```

If the PR already exists:

```bash
python3 scripts/github_pr_branch.py adopt --slug p12-command-fs-envelope
python3 scripts/github_pr_branch.py check
```

Manual equivalent after `gh pr create` → `#19`:

```bash
git fetch origin
git branch -m pr/19-p12-command-fs-envelope
git branch --set-upstream-to=origin/wip/p12-command-fs-envelope
```

### Rules

- Lowercase, hyphens, no spaces
- Local name: `pr/<digits>-<slug>` (`pr/15-…`, not `pr/#15-…`)
- Slug is stable and descriptive (phase id, area, or outcome)
- No personal names or machine hostnames
- One open PR ↔ one local `pr/<N>-…` checkout

## Pull requests

### Title

Same as emoji-first Conventional Commit subject (often the squash merge
message). This is what appears in the repository **Commits** and PR lists.

### Local branch vs GitHub head (after open)

| Ref | Must be | Must not |
|---|---|---|
| **Local checkout** | `pr/<this-PR-number>-…` | `pr/<program-phase>-…` when that ≠ PR # |
| **Remote PR head** (`headRefName`) | stable `wip/…` (or the original push name) | renamed to `pr/N-…` |

Renaming or deleting the **remote** head **closes the open PR** on GitHub.
Use `python3 scripts/github_pr_branch.py open|adopt|check` so only the **local**
name becomes `pr/<N>-…` while the remote stays put. Before review/merge, `check`
must exit 0.

### Description

Use the PR template. Always include:

1. Summary (why / outcome)
2. Test plan with commands actually run
3. Risk notes (API, schema, install)
4. Naming planes table (program / delivery / ADR / local branch / remote head)
   when any plane applies — see [artifact-naming.md](./artifact-naming.md)

### Labeling on the CLI

```bash
gh pr create ... \
  --label "✨ type:feat" \
  --label "🔀 area:workflow" \
  --label "▪️ size:S" \
  --label "🍃 risk:low"
```

Quote labels: the emoji and space are part of the name.

### Stacking

For multi-phase work (e.g. S+++ P10 then P11 — **program** phases, not PR #s):

1. Open PR A → `main`; local `pr/<A>-…` (A is GitHub’s number)
2. Open PR B → base **A** (or merge A first, then B → `main`); local `pr/<B>-…`
3. Prefer sequential merge after reviews over one mega-PR
4. One primary program phase per PR; cite planes in the PR body
   (`program P12 · delivery PR #N · ADR-…`)

### Review

- Prefer ≥1 review on `🔐 risk:security` / `💥 risk:breaking` / `⬛ size:L+`
- Architecture peels and SmartDeny changes: call out tests that prove fences
- Use `🚫 status:do-not-merge` while CI or EM is red

### Merge style

Prefer **merge commit** or **squash** consistently per stack:

- Single-commit feature branches → squash is fine
- Multi-commit intentional history → merge commit

Never force-push `main`.

## Issues

- Prefer issue templates (bug / feature / architecture)
- Title: `bug: …`, `feat: …`, or `architecture: …`
- After triage: set `area:*`, `priority:*`, and move off `🔍 status:triage`
- Close with commit footer `Fixes #n` when the PR lands

## Syncing labels

```bash
# Preview
python3 scripts/sync-github-labels.py --dry-run

# Create/update from .github/labels.yml
python3 scripts/sync-github-labels.py

# Also delete remote labels not in the YAML (careful)
python3 scripts/sync-github-labels.py --prune
```

Adding a label:

1. Edit `.github/labels.yml` (include a leading emoji + space)
2. Run `sync-github-labels.py`
3. Document the emoji in the legend above if it is a new namespace

## Mapping quick reference (PR)

| Change | type | area examples | risk / process |
|---|---|---|---|
| New workflow vertical | `✨ type:feat` | `🔀 area:workflow` `🤖 area:agent` | `🔄 process:em-refresh` |
| SmartDeny fix | `🔧 type:fix` | `🔁 area:runtime` `🛡️ area:security` | `🔐 risk:security` |
| Crate peel | `♻️ type:refactor` | `🧠 area:kernel` + peels | `🏗️ type:architecture` `📋 process:adr` |
| Docs only | `📝 type:docs` | `📖 area:docs` | — |
| EM generator | `🧹 type:chore` | `🧬 area:em` | `🔄 process:em-refresh` |
| Install script | `⚙️ type:ci` or `🧹 type:chore` | `🏭 area:ci` `🖥️ area:desktop` | — |

## Anti-patterns

- Labels without a leading emoji
- Free-text labels (`WIP`, `Johns PR`, `urgent!!!`)
- Multiple competing `type:` labels
- `🟪 size:XL` without a split plan
- Commit subjects that describe files instead of outcomes
- Branches named `update`, `tmp`, `fix2`
- Claiming Confirmed architecture behaviour without tests/source
- Forcing local/remote branch `pr/<P##>-…` so it “matches” the program phase
- Renaming remote PR head to `pr/N-…` (closes the open PR)
- Using ADR number, program phase, and PR number as if they were one sequence
