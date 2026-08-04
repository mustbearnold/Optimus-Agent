---
doc_id: specs-conventions
doc_type: reference
plane: work
status: current
authority: canonical
summary: Repository conventions — documents, code formatting, package managers, commits, naming, and the spec template every capability follows.
reviewed_on: 2026-08-05
review_by: 2026-10-31
knowledge_type: specification
covers:
  - scripts/verify.sh
  - justfile
  - .editorconfig
  - .config/**
depends_on:
  - specs/constitution.md
validated_by:
  - scripts/test_verify_gate_parity.py
  - scripts/check-lockfile-discipline.py
---

# Conventions — Optimus Agent

The second authority in this repository, after
[`specs/constitution.md`](constitution.md).

## Documents (Markdown)

- Frontmatter: every cataloged doc keeps `doc_id`/`plane`/`status`/… so the
  docs DB (`scripts/docs_system.py`) and Engineering Memory keep working.
  Specs additionally carry the SDD template fields (Status, Owner).
- ATX headings (`#`), exactly one H1 per file (the title), sentence case.
- Blank line before and after headings, lists, and code fences.
- Code fences always declare a language.
- No hard line-wrapping inside paragraphs; let editors soft-wrap.
- Tables only for genuinely tabular data; otherwise lists or prose.
- Relative links only within the repo. Kebab-case filenames.
- Placement law: required behavior or intent → **spec**
  (`specs/NNN-<slug>/spec.md`). A choice among alternatives → **ADR**
  (`docs/decisions/`). How to operate it → **runbook** (`docs/runbooks/`).
  System shape → **architecture** (`docs/architecture.md`). None of these →
  don't write it.

## Code

Formatter output is law. Never hand-format; never argue with the formatter.

| Language   | Formatter     | Linter         |
| ---------- | ------------- | -------------- |
| Rust       | cargo fmt (gate) | clippy      |
| JS / TS    | — (editor config) | ESLint (UI) |
| Python     | — (stdlib)    | unittest gates |
| Shell      | —             | `bash -n` in verify |
| JSON/YAML  | — (curated)   | —              |

Package-manager law: Cargo for Rust, Bun for JS/TS (gate-pinned; no foreign
lockfiles anywhere). Configs are committed (`.config/nextest.toml`,
`.editorconfig` at root sets charset/indent/EOL).

## Commits

Emoji-first Conventional Commits: `<emoji> <type>(<scope>): <summary>` —
types: feat, fix, refactor, docs, test, chore, sdd. One logical change per
commit. Formatting commits contain no logic. Migration phases use
`🧹 sdd(phase-N): …` as the summary. Commits are pushed directly to
`origin/main`.

## Naming

Docs and spec slugs: kebab-case. Code identifiers: the language's standard.
Spec/ADR numbers: zero-padded, never reused. ADRs are monotonic and permanent
in `docs/decisions/`; never renumber or invent without scanning them first.

## Spec template (every `specs/NNN-<slug>/spec.md`)

```markdown
# <Capability name>

Status: active
Owner: <owner>

## Purpose
What this capability is for, in a few sentences. No requirements here.

## Requirements
- R1. <requirement> (MUST/SHOULD/MAY; RFC 2119)
- R2. <requirement>
  - [inferred] mark anything derived from code, not from a written decision.

## Acceptance criteria
- [ ] A1. Given <context>, when <action>, then <outcome>.
- [ ] A2. …

## Out of scope
Explicit non-goals, so nobody re-adds them by accident.

## Open questions
- None.

## Links
- Related ADRs, runbooks, or generated systems (paths or `doc_id`s).
```
