---
knowledge_type: process
status: current
owns:
  - .github/labels.yml
  - .github/pull_request_template.md
  - .github/ISSUE_TEMPLATE/**
  - scripts/sync-github-labels.py
  - docs/contributing/github-conventions.md
watches:
  - AGENTS.md
covers:
  - docs/contributing/github-conventions.md
depends_on:
  - docs/architecture/architecture-marks.md
validated_by:
  - scripts/sync-github-labels.py
last_verified_commit: null
---

# GitHub conventions (Optimus Agent)

This document is the **single process source** for commits, branches, PRs,
issues, and labels. Labels are defined in [`.github/labels.yml`](../../.github/labels.yml)
and synced with `python3 scripts/sync-github-labels.py`.

## Principles

1. **Emoji-first labels** — each label is `emoji + space + namespace:value`
   (e.g. `✨ type:feat`). Exactly one leading emoji.
2. **Namespaced tokens** — `namespace:value` (lowercase, hyphens in values).
3. **Conventional Commits** for titles and preferred commit subjects.
4. **One concern per PR** — prefer a stack of small PRs over `🟪 size:XL`.
5. **Executable evidence** outranks prose (see `AGENTS.md` status legend).
6. **Do not invent labels ad hoc** — extend `.github/labels.yml` and re-sync.

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

### Emoji legend (quick)

| Namespace | Emoji map |
|---|---|
| **type** | 🐛 bug · ✨ feat · 🔧 fix · ♻️ refactor · 📝 docs · ✅ test · 🧹 chore · ⚙️ ci · ⚡ perf · 🔒 security · 🏗️ architecture |
| **area** | 🧠 kernel · 🔁 runtime · 🤖 agent · 🔀 workflow · 📦 artifacts · 🧩 memory · 🎯 skills · 📚 packs · 💾 store · 🛰️ ops · 📊 eval · 💻 cli · 🖥️ desktop · 🎨 ui · 🌐 browser · 🛡️ security · 📖 docs · 🧬 em · 🏭 ci |
| **priority** | 🚨 p0 · 🔥 p1 · ⚠️ p2 · ⬇️ p3 |
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

## Conventional Commits

Format:

```text
<type>(optional-scope): <imperative summary>

[optional body]

[optional footer]
```

### Types (align with `type:` labels)

| Commit type | PR label | Meaning |
|---|---|---|
| `feat` | `✨ type:feat` | User-visible capability |
| `fix` | `🔧 type:fix` | Bug fix |
| `docs` | `📝 type:docs` | Docs only |
| `refactor` | `♻️ type:refactor` | Behaviour-preserving restructure |
| `test` | `✅ type:test` | Tests only |
| `chore` | `🧹 type:chore` | Tooling, deps, housekeeping |
| `ci` | `⚙️ type:ci` | CI workflows |
| `perf` | `⚡ type:perf` | Performance |
| `build` | `🧹 type:chore` | Build system (map to chore) |
| `style` | `🧹 type:chore` | Formatting only |
| `revert` | match original | Reverts a prior commit |

### Scopes (optional; align with `area:`)

Prefer short crate/app names: `kernel`, `runtime`, `workflow`, `agent`,
`desktop`, `cli`, `ui`, `eval`, `em`, `docs`, `security`.

### Subject rules

- Imperative mood: “add”, “fix”, “peel” — not “added” / “adds”
- ≤72 characters
- No trailing period
- Reference issues: `Fixes #123` / `Refs #123` in body or footer

### Examples

```text
feat(workflow): add write-then-read handoff DAG
fix(runtime): refuse grant transfer across effect hashes
refactor(kernel): peel agent contracts into optimus-agent
docs(architecture): accept ADR-0034 control-plane peels
test(workflow): cover mid-DAG cancel tree
chore(em): refresh generated indexes after peel
```

## Branch naming

```text
<type>/<short-kebab-description>
```

Examples:

- `feat/write-then-read-handoff`
- `fix/smartdeny-grant-transfer`
- `refactor/control-plane-peels`
- `docs/github-conventions`
- `chore/sync-labels`

**Agent-driven work** may use:

```text
agent/<topic-kebab>
```

Rules:

- Lowercase, hyphens, no spaces
- No personal names or machine hostnames
- One primary type prefix; don’t stack `feat/fix/...`

## Pull requests

### Title

Same as Conventional Commit subject (often the squash merge message).

### Description

Use the PR template. Always include:

1. Summary (why / outcome)
2. Test plan with commands actually run
3. Risk notes (API, schema, install)

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

For multi-phase work (e.g. S+++ P10 then P11):

1. Open PR A → `main`
2. Open PR B → base **A** (or merge A first, then B → `main`)
3. Prefer sequential merge after reviews over one mega-PR

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
