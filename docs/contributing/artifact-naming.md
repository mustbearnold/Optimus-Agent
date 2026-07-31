---
doc_id: contributing-artifact-naming
doc_type: how-to
plane: current
status: current
authority: supporting
summary: This is the canonical identity model for Optimus engineering artifacts. It is a development document and is never loaded as installed-product behaviour.
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: process
owns:
  - docs/contributing/artifact-naming.md
watches:
  - AGENTS.md
  - docs/plans/s-plus-plus-plus-program.md
  - docs/plans/product-complete-program.md
  - docs/plans/github-engineer-program.md
  - docs/architecture/architecture-marks.md
  - docs/decisions/README.md
covers:
  - docs/contributing/artifact-naming.md
depends_on:
  - docs/plans/s-plus-plus-plus-program.md
  - docs/plans/product-complete-program.md
  - docs/plans/github-engineer-program.md
  - docs/architecture/architecture-marks.md
---

# Artifact naming planes

This is the canonical identity model for Optimus engineering artifacts. It is a
development document and is never loaded as installed-product behaviour.

## Core law

> Identifiers from different planes are never interchangeable.
>
> `P12` ≠ task id ≠ delivery SHA ≠ `ADR-0012` ≠ grade `S+++` ≠ runtime
> `agent@version`.

## The six planes

| Plane | Token shape | Authority | Example |
|---|---|---|---|
| Decision | `ADR-NNNN` | `docs/decisions/NNNN-*.md` | `ADR-0060` |
| Program | `P##` | Active owning plan under `docs/plans/` | program P21 |
| Plan / microtask | Plan-local id (`S1`, `C3`, `M7`…) | Owning plan | P21 M7 |
| Delivery | task id + full SHA on `origin/main` | managed land record + remote main | `instruction-plane-cleanup`, `a081…` |
| Grade / mark | Mark name + grade | `docs/architecture/architecture-marks.md` | Security A- |
| Runtime product | `id@version`, crate, or pack id | Source contracts and SemVer | `workspace_writer@1` |

### Decision

- ADR numbers are zero-padded, monotonic, and permanent.
- Scan `docs/decisions/README.md` before allocating the next number.
- Do not rewrite or renumber accepted history to align it with another plane.
- A program phase may appear in ADR title text, but does not determine the ADR
  number.

### Program

`P##` is meaningful only with its owning program:

| Program | Phases | Authority | Status |
|---|---|---|---|
| Architecture S+++ climb | P10-P19 | `s-plus-plus-plus-program.md` | historical |
| Product-complete daily app | P20-P29 | `product-complete-program.md` | closed / historical |
| Reliability and autonomy | P30-P35 | `reliability-autonomy-program.md` | P30 prerequisite; P31-P35 parked |
| GitHub Engineer product capability | P40-P46 | `github-engineer-program.md` | historical |

Always say “program P##” in prose. Historical specification filenames such as
`phase-20*` are document-local names, not program identifiers.

### Plan / microtask

Microtask ids are local to their owning plan. Never mint a global ticket,
delivery, ADR, or branch identity from a plan-local id.

### Delivery

The primary coding agent derives a stable task id from the user outcome and
selects an actually available producing model and reasoning effort. Only
`just land <task-id> --model <model> --effort <level>` may create delivery
history. The land record binds:

- task id and affected seam;
- symbols touched;
- fixture and gate results;
- producing model and reasoning effort;
- the full commit SHA placed on `origin/main`.

The SHA read back from `origin/main` is delivery truth. A worktree, checkpoint,
temporary branch, local diff, issue, or pull request is not delivery.

### Grade / mark

Grades measure architecture quality. They move only when source, tests, docs,
and exit criteria support the claim. A landed task or finished program phase
does not automatically change a grade.

### Runtime product

Agent ids, workflow ids, tool ids, pack ids, and crate versions belong to the
runtime product plane. Do not name a product type after an ADR, task id, or
program phase.

## Worked example

| Plane | Correct | Wrong |
|---|---|---|
| Program | program P21 | “task 21” |
| Decision | next free ADR number after scanning | ADR-0021 because the program is P21 |
| Task | `pack-registry-integrity` | `P21` |
| Delivery | exact SHA reported by `just land` | a feature branch or checkpoint |
| Grade | measured result after exit gates | “S+++ because it landed” |
| Runtime id | `pack_registry@1` | `p21_agent` |

## Coding-agent checklist

1. Name the plane whenever a bare identifier could be ambiguous.
2. Derive task ids from outcomes, not program or ADR numbers.
3. Never invent or manually edit commit messages; the land system owns them.
4. Record only the model and reasoning effort that actually produced the work.
5. Keep historical GitHub identifiers as historical evidence, never current
   delivery authority.
