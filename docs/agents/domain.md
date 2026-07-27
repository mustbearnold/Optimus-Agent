# Domain Docs

How the engineering skills should consume this repo's domain documentation when exploring the codebase.

This repo is **single-context**. It is a Rust workspace with several crates, but
one domain — there is no `CONTEXT-MAP.md` and no per-context glossary.

## Before exploring, read these

- **`CONTEXT.md`** at the repo root — the domain glossary (Job, Node, Event,
  Interrupted, Resume, Workspace) and the public seams.
- **`docs/decisions/`** — the ADRs. Read the ones that touch the area you're
  about to work in. `docs/decisions/README.md` is the index; use it to find the
  relevant numbers rather than reading the whole directory.
- **`AGENTS.md`** — the engineering rules. Its hard project boundary and its
  numbered rules bind skills exactly as they bind a human.

If any of these files don't exist, **proceed silently**. Don't flag their
absence; don't suggest creating them upfront. The `/domain-modeling` skill
(reached via `/grill-with-docs` and `/improve-codebase-architecture`) creates
them lazily when terms or decisions actually get resolved.

## File structure

ADRs live in **`docs/decisions/`**, not `docs/adr/` — the skills' default. When
a skill says "write an ADR" or "read `docs/adr/`", it means this directory.

```
/
├── CONTEXT.md
├── AGENTS.md
├── docs/decisions/
│   ├── README.md                 ← the index; every ADR gets a row
│   ├── 0046-approving-resumes-the-turn.md
│   └── 0049-module-size-is-measured-honestly.md
├── crates/                       ← library crates
└── apps/                         ← binaries (tui, desktop, cli, electron, ui)
```

Numbering is a zero-padded four-digit sequence with a kebab-case slug. Take the
next free number from `docs/decisions/README.md`.

## ADR front matter is mandatory

Every ADR carries YAML front matter that the Engineering Memory generator reads:

```yaml
---
knowledge_type: decision
status: current
covers:
  - path/to/the/code/this/decides.rs
depends_on:
  - docs/architecture/some-blueprint.md
validated_by:
  - path/to/the/test/that/proves/it.rs
last_verified_commit: null
---
```

An ADR without it will fail the `engineering-memory-valid` gate. After adding
one, run `just em-generate` — never hand-edit `.engineering-memory/*.json`.

## When an ADR is required

AGENTS.md rule 19: important architectural decisions require an ADR. Rule 18:
bug fixes require a regression test. A skill that lands a design change without
an ADR has not finished.

## Use the glossary's vocabulary

When your output names a domain concept (in an issue title, a refactor proposal,
a hypothesis, a test name), use the term as defined in `CONTEXT.md`. Don't drift
to synonyms the glossary explicitly avoids.

If the concept you need isn't in the glossary yet, that's a signal — either
you're inventing language the project doesn't use (reconsider) or there's a real
gap (note it for `/domain-modeling`).

## Flag ADR conflicts

If your output contradicts an existing ADR, surface it explicitly rather than
silently overriding:

> _Contradicts ADR-0046 (approving resumes the turn) — but worth reopening because…_
