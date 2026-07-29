<!--
Title format (emoji-first Conventional Commits — shows on repo Commits tab):
  <emoji> <type>(optional-scope): <imperative summary>

Examples:
  ✨ feat(workflow): add write-then-read DAG handoff
  🔧 fix(runtime): fence SmartDeny grants to effect hash
  📝 docs(architecture): record ADR-0034 control-plane peels
  ♻️ refactor(kernel): peel agent contracts into optimus-agent
  🏗️ architecture: S+++ P12 command capability envelope
  🧹 chore(em): refresh generated indexes

Naming planes (mandatory — see docs/contributing/artifact-naming.md):
  P## = program phase · PR #N = delivery · ADR-NNNN = decision · grade ≠ number
  Local branch: pr/<this-PR-number>-slug  (NOT pr/<program-phase>-…)
  Remote head: stays wip/… (do not rename remote head — closes the PR)
-->

## Summary

<!-- 1–3 bullets: what changed and why (user-facing outcome, not file list). -->

-

## Focused issue

<!-- One issue outcome per PR. Use `Fixes #N` so merge closes it. -->

Fixes #

## Naming planes

<!-- Coding agents: fill every row that applies. Never set Delivery = Program. -->

| Plane | Value |
|---|---|
| **Program** (`P##` or n/a) | |
| **Delivery** (`PR #N` — GitHub assigns) | this PR |
| **Decision** (`ADR-NNNN` or none) | |
| **Mark / grade target** (if architecture) | |
| **Local branch** | `pr/<N>-…` after open |
| **Remote head** | `wip/…` (stable; do not rename) |

## Type / labels

<!-- Apply on the PR. Format: emoji + space + namespace:value. Minimum: one type: + one area: + size: -->

- **type:** `✨ type:feat` | `🔧 type:fix` | `♻️ type:refactor` | `📝 type:docs` | `✅ type:test` | `🧹 type:chore` | `⚙️ type:ci` | `🔒 type:security` | `🏗️ type:architecture` | `⚡ type:perf` | `🐛 type:bug`
- **area:** e.g. `🧠 area:kernel` `🔁 area:runtime` `🔀 area:workflow` `🖥️ area:desktop` …
- **priority:** `🚨 priority:p0` … `⬇️ priority:p3` (if issue-linked)
- **size:** `▫️ size:XS` … `🟪 size:XL`
- **risk:** `🍃 risk:low` | `📀 risk:data` | `🔐 risk:security` | `💥 risk:breaking` as applicable
- **program:** `🏆 program:s+++` / `⚖️ program:parity` when relevant
- **process:** `📋 process:adr` / `🔄 process:em-refresh` when required

## Test plan

- [ ] Focused unit/integration tests for the changed subsystem
- [ ] Relevant gates if touched (e.g. `check-crate-layers.py`, observability, IPC matrix)
- [ ] `python3 scripts/engineering_memory.py generate` + `validate --quick` if EM-owned surface changed
- [ ] `just verify`

## Review and merge automation

<!-- Keep draft until CI and Codex review complete. Do not bypass red or unresolved gates. -->

- **Codex review:** `@codex review` requested / completed
- **Required CI:** `just verify (Linux)` pending / passed
- **Auto-merge:** not enabled / `gh pr merge --auto --merge`
- **Blockers:** none / list exact red check, unresolved finding, conflict, or approval boundary

## Risk & rollout

<!-- Breaking changes, migrations, install/relaunch needs, rollback notes. -->

-

## Checklist

- [ ] Emoji-first Conventional Commit title (and commits if stacked)
- [ ] Local branch is `pr/<this-PR-number>-<short-kebab>` (not the program phase number); run `python3 scripts/github_pr_branch.py adopt` if local name is wrong
- [ ] Remote head remains `wip/…` (never rename remote to `pr/N-…` — that closes the PR)
- [ ] `python3 scripts/github_pr_branch.py check` exits 0
- [ ] Naming planes table has concrete values (`PR #N`, not only “this PR”); program ≠ delivery ≠ ADR number
- [ ] No secrets or home paths in logs/diffs
- [ ] No credentials or private information in the issue, PR, commits, logs, or tracked environment files
- [ ] Docs/ADR updated when contracts or architecture change
- [ ] Labels applied (emoji + `type:` + `area:` + `size:` minimum)
- [ ] Draft PR received `@codex review`; every actionable finding and conversation is resolved
- [ ] Focused checks and `just verify` pass locally; required `just verify (Linux)` CI passes at the latest SHA
- [ ] PR is current, mergeable, ready, and has no `🚫 status:do-not-merge` label
- [ ] Gated merge commit automation is enabled with `gh pr merge --auto --merge`
