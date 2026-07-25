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

1. **Namespaced labels** — `namespace:value` (lowercase, hyphens in values).
2. **Conventional Commits** for titles and preferred commit subjects.
3. **One concern per PR** — prefer a stack of small PRs over `size:XL`.
4. **Executable evidence** outranks prose (see `AGENTS.md` status legend).
5. **Do not invent labels ad hoc** — extend `.github/labels.yml` and re-sync.

## Label namespaces

| Namespace | Purpose | Examples | How many |
|---|---|---|---|
| `type:` | Kind of change | `type:feat`, `type:fix`, `type:refactor` | **exactly one** on PRs |
| `area:` | Owning subsystem | `area:workflow`, `area:desktop` | **≥1** (primary first) |
| `priority:` | Urgency | `priority:p0` … `priority:p3` | issues; optional on PRs |
| `status:` | Workflow state | `status:needs-review` | issues/PRs; keep current |
| `size:` | Review bulk | `size:S` … `size:XL` | **one** on PRs |
| `risk:` | Blast radius | `risk:security`, `risk:data` | as applicable |
| `program:` | Initiative | `program:s+++`, `program:parity` | when relevant |
| `process:` | Meta process | `process:adr`, `process:em-refresh` | as applicable |

### Minimum label set

| Artifact | Required labels |
|---|---|
| **Pull request** | `type:*` + ≥1 `area:*` + `size:*` |
| **Bug issue** | `type:bug` + `status:triage` (+ area after triage) |
| **Feature issue** | `type:feat` + `status:triage` |
| **Architecture task** | `type:architecture` + `program:s+++` when S+++ |

### Priority scale

| Label | Use when |
|---|---|
| `priority:p0` | Production break, security exploitability, data loss |
| `priority:p1` | Blocks the current milestone / release gate |
| `priority:p2` | Important; not blocking the active milestone |
| `priority:p3` | Backlog / polish |

### Status lifecycle (issues)

```text
status:triage → status:ready → status:in-progress
       ↘ status:blocked
status:in-progress → status:needs-review → status:approved → (closed)
                   ↘ status:changes-requested → status:in-progress
```

PRs typically use `status:needs-review` / `status:changes-requested` /
`status:approved` / `status:do-not-merge` rather than issue-only states.

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
| `feat` | `type:feat` | User-visible capability |
| `fix` | `type:fix` | Bug fix |
| `docs` | `type:docs` | Docs only |
| `refactor` | `type:refactor` | Behaviour-preserving restructure |
| `test` | `type:test` | Tests only |
| `chore` | `type:chore` | Tooling, deps, housekeeping |
| `ci` | `type:ci` | CI workflows |
| `perf` | `type:perf` | Performance |
| `build` | `type:chore` | Build system (map to chore) |
| `style` | `type:chore` | Formatting only |
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

Example: `agent/s-plus-p11-control-plane-peels` (historical series OK).

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

### Stacking

For multi-phase work (e.g. S+++ P10 then P11):

1. Open PR A → `main`
2. Open PR B → base **A** (or merge A first, then B → `main`)
3. Prefer sequential merge after reviews over one mega-PR

### Review

- Prefer ≥1 review on `risk:security` / `risk:breaking` / `size:L+`
- Architecture peels and SmartDeny changes: call out tests that prove fences
- Use `status:do-not-merge` while CI or EM is red

### Merge style

Prefer **merge commit** or **squash** consistently per stack:

- Single-commit feature branches → squash is fine
- Multi-commit intentional history → merge commit

Never force-push `main`.

## Issues

- Prefer issue templates (bug / feature / architecture)
- Title: `bug: …`, `feat: …`, or `architecture: …`
- After triage: set `area:*`, `priority:*`, and move off `status:triage`
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

1. Edit `.github/labels.yml`
2. Run `sync-github-labels.py`
3. Document the label here if it introduces a new namespace

## Mapping quick reference (PR)

| Change | type | area examples | risk / process |
|---|---|---|---|
| New workflow vertical | `type:feat` | `area:workflow` `area:agent` | `process:em-refresh` |
| SmartDeny fix | `type:fix` | `area:runtime` `area:security` | `risk:security` |
| Crate peel | `type:refactor` | `area:kernel` + peels | `type:architecture` `process:adr` |
| Docs only | `type:docs` | `area:docs` | — |
| EM generator | `type:chore` | `area:em` | `process:em-refresh` |
| Install script | `type:ci` or `type:chore` | `area:ci` `area:desktop` | — |

## Anti-patterns

- Free-text labels (`WIP`, `Johns PR`, `urgent!!!`)
- Multiple competing `type:` labels
- `size:XL` without a split plan
- Commit subjects that describe files instead of outcomes
- Branches named `update`, `tmp`, `fix2`
- Claiming Confirmed architecture behaviour without tests/source
