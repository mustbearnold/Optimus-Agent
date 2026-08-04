---
doc_id: specs-constitution
doc_type: reference
plane: work
status: current
authority: canonical
summary: The highest repository authority — principles, the SDD loop, definition of done, and naming planes for the Optimus repository.
reviewed_on: 2026-08-05
review_by: 2026-10-31
knowledge_type: specification
covers:
  - AGENTS.md
  - specs/**
  - docs/decisions/**
depends_on:
  - docs/decisions/0062-source-and-development-are-separate-workspace-planes.md
validated_by:
  - scripts/gates/check-instruction-planes.py
  - scripts/tools/docs_system.py
---

# Constitution — Optimus Agent

The highest authority in this repository. Conflicts resolve in this order:

**constitution → conventions → specs → code comments.**

Development law that is not in this file lives in AGENTS.md (operational
entry point) and the gates (`scripts/verify.sh`). This document is the
source of truth for the repository's development plane; it never governs the
installed product's runtime behavior (see Principle 7).

## Purpose

Optimus Agent is built by a durable loop: a capability is specified before
any code, code exists to satisfy the spec, and the spec is the living truth
that outlives the implementation. The repository is the complete reproducible
artifact — source, tests, evaluation definitions, documentation, and build
logic — developed directly on `main`.

## Principles

1. **Specs are the source of truth**; code exists to satisfy them. A merged
   change with a stale spec is a defect, not a chore.
2. **The SDD loop is mandatory for all changes:**
   - No code without a spec. New capability → `specs/NNN-<slug>/spec.md` first.
   - Spec agreed → `plan.md` (design) → `tasks.md` (checklist) → implement.
   - If implementation diverges from the spec, update the spec in the same
     change.
   - When a capability ships and stabilizes, delete its `plan.md` and
     `tasks.md` (git remembers). `spec.md` remains as living truth.
   - A bug is a failing acceptance criterion. If the spec did not cover it,
     the spec was wrong — fix both.
3. **Main-only development** on `main`: zero worktrees, zero feature
   branches, enforced by `.githooks/`. No PR ceremony, no `gh`, no
   history-changing Git.
4. **Delete freely** — git remembers. Never keep dead code or stale docs
   "just in case"; when uncertain, attic — `_attic/` is the quarantine, and
   emptying it is a human decision.
5. **Small, reversible steps.** Separate commits for structure, content, and
   formatting.
6. **Secrets never enter** the repo, specs, or reports.
7. **The instruction-plane firewall:** development instructions (autonomy,
   orchestration, model selection, permissions, VCS, testing) govern the
   agents changing this repository only — never the installed product's
   runtime constitution (`OPTIMUS_AGENTS.md`) or product prompts. A request
   about how a coding agent should develop Optimus is not a product
   requirement.
8. **Evidence before claims:** source code and executable tests outrank
   prose. Label architecture claims as confirmed/inferred/planned/unknown.
   Do not claim a capability exists unless its real implementation and tests
   exist.

## Definition of done

- `bash scripts/verify.sh all` passes with zero skips on the managed path;
- every acceptance criterion in the owning spec is met;
- the spec matches implemented reality (no stale spec rides a merged change);
- conventions are followed;
- Engineering Memory is regenerated and valid.

## Decision protocol

A choice among alternatives is an ADR (`docs/decisions/ADR-NNNN.md`):
monotonic, permanent, with mandatory frontmatter. ADRs are never renumbered
or rewritten to hide superseded reasoning; a superseded decision is marked
`status: historical` and stays as the record.

## Naming planes (mandatory)

Identifiers from different planes are never interchangeable:

| Plane | Token |
|---|---|
| Decision | `ADR-NNNN` |
| Program | `P##` (historical prose only) |
| Plan / microtask | plan-local (`M*`, `C*`, `S*` …) |
| Delivery | full Git commit SHA on `origin/main` |
| Grade / mark | mark + grade (`S+++`, `A-` …) |
| Runtime product | `id@version` / crate / pack |

Never collapse two planes into one identifier. Commits are emoji-first
Conventional Commits. Full detail: `specs/conventions.md`.
