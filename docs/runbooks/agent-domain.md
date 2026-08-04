---
doc_id: agents-domain
doc_type: reference
plane: current
status: current
authority: supporting
summary: How coding agents should consume this repository's domain documentation.
reviewed_on: 2026-07-31
review_by: 2026-10-31
---

# Domain Docs

> Developer-agent configuration only. Never inject this file into an installed
> Optimus product prompt.

How coding agents should consume this repository's domain documentation.

This repo is **single-context**. It is a Rust workspace with several crates, but
one domain — there is no `CONTEXT-MAP.md` and no per-context glossary.

## Before exploring, read these

- **`CONTEXT.md`** at the repo root — concise foundational domain language.
- **`docs/architecture/system-overview.md`** — current architecture authority
  and implemented topology.
- **`docs/decisions/`** — the ADRs. Read the ones that touch the area you're
  about to work in. `docs/decisions/README.md` is the index; use it to find the
  relevant numbers rather than reading the whole directory.
- **`AGENTS.md`** — the engineering rules. Its hard project boundary and its
  numbered rules bind skills exactly as they bind a human.

## File structure

ADRs live in **`docs/decisions/`**, not `docs/adr/`.

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
doc_id: decisions-NNNN-short-title
doc_type: decision
plane: decision
authority: record
summary: The decision and the system boundary it governs.
reviewed_on: YYYY-MM-DD
review_by: YYYY-MM-DD
knowledge_type: decision
status: current
covers:
  - path/to/the/code/this/decides.rs
depends_on:
  - docs/architecture/some-blueprint.md
validated_by:
  - path/to/the/test/that/proves/it.rs
---
```

An ADR without it will fail the documentation or Engineering Memory gate. After
adding one, run `just docs-generate`, then explicitly acknowledge reviewed
bindings with `just docs-refresh <doc-id>`. `just em-generate` only rebuilds the
disposable local lens cache; never hand-edit `.engineering-memory/*.json`.

## When an ADR is required

AGENTS.md rule 19: important architectural decisions require an ADR. Rule 18:
bug fixes require a regression test. A skill that lands a design change without
an ADR has not finished.

## Use the glossary's vocabulary

When output names a domain concept in a plan, change, or test, use the term as
defined in `CONTEXT.md`. Do not drift to synonyms the glossary explicitly
avoids.

If the concept is absent, either use established source vocabulary or update the
glossary as part of the same verified change.

## Flag ADR conflicts

If your output contradicts an existing ADR, surface it explicitly rather than
silently overriding:

> _Contradicts ADR-0046 (approving resumes the turn) — but worth reopening because…_
