---
doc_id: decisions-0079-keep-bun-as-the-js-package-manager
doc_type: decision
plane: decision
status: current
authority: record
summary: The JS/TS workspace stays on Bun. pnpm is not adopted: its monorepo advantages — strict non-hoisted isolation, content-addressed store dedup, workspace protocol — all target internal dependency graphs, and this flat three-package workspace has none, while adoption would force a spec amendment, a gate rewrite, and a second tool alongside the bun runtime.
reviewed_on: 2026-08-05
review_by: 2026-11-05
knowledge_type: decision
covers:
  - specs/conventions.md
  - specs/011-developer-tooling/spec.md
  - scripts/gates/check-lockfile-discipline.py
  - package.json
  - justfile
  - docs/runbooks/install-relaunch.md
validated_by:
  - scripts/tests/test_lockfile_discipline.py
---

# ADR-0079: Keep Bun as the JS/TS package manager for the flat monorepo

- **Status:** Accepted
- **Date:** 2026-08-05

## Context

Optimus Agent is a flat monorepo. The Rust side is a Cargo workspace; the JS/TS
side is a bun workspace declared in the root `package.json`:

- `apps/optimus-ui` — the React 19 / Vite 7 surface (vitest, tailwind).
- `apps/optimus-tauri` — the Tauri v2 CLI wrapper around the UI.
- `apps/optimus-desktop` — Playwright end-to-end tests only.

All three packages are `private`. There are no `workspace:*` protocol links and
no cross-package imports between them — the lockfile's only workspace entries
are the three roots themselves. The root carries the shared Playwright
devDependency and delegates scripts with `bun run --cwd …`.

Bun is not only the installer here. The toolchain runs on it: `bun run` drives
every root script, `bunx playwright` installs and drives the browser runner,
and `bun scripts/tests/tui_layout_playwright.cjs` executes the TUI layout
probe. The root declares `"packageManager": "bun@1.3.14"` and `bun.lock` is
committed and installed frozen (`bun install --frozen-lockfile` in
`docs/runbooks/install-relaunch.md`).

The package-manager law is deliberate and gate-pinned: `specs/conventions.md`
states "Cargo for Rust, Bun for JS/TS (gate-pinned; no foreign lockfiles
anywhere)", spec-011 `R6` repeats it, and
`scripts/gates/check-lockfile-discipline.py` fails the gate when a foreign
lockfile (`package-lock.json`, `yarn.lock`, **`pnpm-lock.yaml`**, or
`npm-shrinkwrap.json`) is tracked anywhere, when a root lockfile is missing, or
when `packageManager` stops declaring `bun@`. Commit `8f8065d` introduced the
gate in 2026-08-05.

Question under decision: is pnpm a better option because the project is a
monorepo?

## Decision

**No — keep Bun. pnpm is not adopted as the JS/TS package manager.**

The monorepo argument does not bind here. pnpm's concrete advantages are all
about internal package-graph complexity:

1. **Strict, non-hoisted `node_modules`** detects phantom dependencies
   (importing a package the manifest never declared). An audit of every bare
   import in `apps/optimus-ui/src` finds all 20 bare specifiers resolve to
   declared dependencies; the only exceptions are the `@/` path aliases and
   `node:` builtins. There is no hoisting casualty today to fix.
2. **Content-addressable global store with hard links** deduplicates disk
   usage across many packages sharing dependencies. This workspace is three
   independent packages — UI, Tauri CLI, Playwright — with nearly disjoint
   dependency sets; there is no cross-package duplication to reclaim, and
   ~606 MB of `node_modules` is not a constraint on this development machine.
3. **`workspace:*` protocol, filters, catalogs, patch management** are mature
   in pnpm, but they manage links between packages, and this repo has none.

Adoption would not replace bun, it would add pnpm beside it: bun stays as the
script runtime (`bun run`, `bunx`, the `.cjs` probe runner), so the toolchain
would carry two package tools where it carries one. The migration would also
amend the package-manager law in two specs, rewrite the enforcement gate that
explicitly names `pnpm-lock.yaml` as a failure, reverse a decision made and
pinned earlier the same day, migrate `bun.lock` → `pnpm-lock.yaml`, and touch
the runbook, `justfile`, and `scripts/verify.sh` — all to solve problems the
repository does not currently have.

## Consequences

- The package-manager law, the lockfile-discipline gate, and the bun toolchain
  stay as they are. No spec amendment, no gate rewrite, no lockfile migration.
- The decision is recorded, so the question does not resurface as folklore:
  any future reconsideration starts from the topology facts below.
- Discipline that pnpm would have *enforced* (declared dependencies only)
  remains a convention: bun hoists, so new JS code must keep imports inside
  declared dependencies. The audit script used as evaluation evidence is the
  cheap way to check that.

## Alternatives considered

- **Adopt pnpm now, replacing bun as installer.** Rejected: no current problem
  it fixes — no phantom dependencies, no internal links, no disk pressure, no
  install-speed complaint — against a real migration cost and a same-day
  reversal of a gate-pinned law.
- **Adopt pnpm while keeping bun as runtime.** Rejected: strictly worse — two
  package tools, the migration cost, and none of the strict-layout benefits
  compounded by hoisting still being the effective layout for the runtime.
- **Keep bun and record nothing.** Rejected: the question was asked and will be
  asked again; a recorded decision with evidence and reconsideration
  conditions is the ADR's job (placement law: a choice among alternatives →
  ADR).

## Evaluation evidence

- Workspace topology: root `package.json` `workspaces` lists exactly three
  packages; `bun.lock` contains no `workspace:` protocol references other than
  the three workspace roots; no cross-package imports exist between the apps.
- Dependency hygiene: scripted audit of `apps/optimus-ui/src` — 20 bare
  imports, all resolved to `dependencies`/`devDependencies`; remainder are
  `@/` aliases and `node:` builtins.
- Gate enforcement is live and tested: `check-lockfile-discipline.py` plus
  `scripts/tests/test_lockfile_discipline.py`; spec-011 R6 and
  `specs/conventions.md` state the law.
- The installed toolchain already standardizes on bun (`packageManager`,
  `justfile` `setup-bun`, `verify.sh` gates, install runbook).

## Conditions for reconsideration

Revisit pnpm when any of these becomes true:

1. The JS workspace grows to several packages with internal `workspace:*`
   dependencies and real cross-package imports (the strict-layout and
   workspace-protocol benefits start paying).
2. A phantom-dependency or hoisting-collision bug is found and blamed on
   bun's hoisted layout.
3. Disk or CI constraints make the content-addressable store and single-store
   cache materially valuable.
4. The toolchain drops bun as the script runtime, so pnpm would replace the
   package tool instead of joining it.

## Reasons

A package-manager choice should be driven by the shape of the dependency graph
it manages, not by the label "monorepo". This graph is three independent
packages with no edges between them; every advantage pnpm offers is a function
of graph complexity this repository does not have, and every cost of adopting
it is real and immediate — a deliberate, gate-enforced law overturned and a
second tool added next to the bun runtime that already serves the workspace
well.
