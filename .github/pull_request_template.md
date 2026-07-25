<!--
Title format (matches Conventional Commits):
  <type>(optional-scope): <imperative summary ≤72 chars>

Examples:
  feat(workflow): add write-then-read DAG handoff
  fix(runtime): fence SmartDeny grants to effect hash
  docs(architecture): record ADR-0034 control-plane peels
-->

## Summary

<!-- 1–3 bullets: what changed and why (user-facing outcome, not file list). -->

-

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

## Risk & rollout

<!-- Breaking changes, migrations, install/relaunch needs, rollback notes. -->

-

## Checklist

- [ ] Conventional commit title (and commits if stacked)
- [ ] Branch named `<type>/<short-kebab>` (or `agent/<topic>` for agent-driven work)
- [ ] No secrets or home paths in logs/diffs
- [ ] Docs/ADR updated when contracts or architecture change
- [ ] Labels applied (emoji + `type:` + `area:` + `size:` minimum)
