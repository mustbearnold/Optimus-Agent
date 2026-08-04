# MIGRATION_REPORT — SDD migration of Optimus Agent

Status: Phase 1 complete (snapshot + inventory)
Started: 2026-08-05
Protocol: SDD_MIGRATION.md (committed in the pre-migration snapshot)

## Stage

**LATE** — mature code (13 crates, 4 apps incl. the Tauri shell), a 61-gate
verify spine (all green at migration start), and a large accumulated
documentation system (236 cataloged docs).

## Toolchain findings

| Toolchain | Manager | Formatter | Linter | Notes |
|---|---|---|---|---|
| Rust | Cargo (workspace, `default-members`) | `cargo fmt` | `cargo clippy` | pinned toolchain, committed `Cargo.lock` |
| TS/JS (UI + e2e) | Bun (workspaces, `bun@1.3.14`) | Prettier (existing config) | ESLint | `bun.lock` frozen |
| Python (gates/scripts) | stdlib only | — (ruff not installed) | unittest self-tests | every gate has a test |
| Shell | — | — | `bash -n` in verify | installer + verify.sh |
| Markdown | docs DB (`scripts/docs_system.py`) | repo conventions | `docs_system.py check` | frontmatter-driven catalog |

Build: `just check` / `just verify` (`bash scripts/verify.sh all`) — the single
land gate. Tests: cargo nextest (`.config/nextest.toml`), bun vitest,
playwright e2e, unittest for scripts.

## Inventory (path → class)

Top-level classes (files under git; generated/ignored paths excluded):

| Path | Class | Fate (Phase 3 plan) |
|---|---|---|
| `crates/**` (186 files) | CODE | Untouched (invariant 9) |
| `apps/**` (237 files) | CODE/TEST | Untouched; `apps/optimus-ui/dist` GENERATED (ignored) |
| `scripts/**` (67 files) | CODE | Untouched; gates adapted as needed |
| `evals/**` (3 files) | TEST | Untouched; question paths re-pointed |
| `skills/**` (2 files) | TOOLING | Kept (agent procedural tooling; see notes) |
| `.githooks/**` (3) | CONFIG | Untouched (main-only enforcement) |
| `.config/nextest.toml` | CONFIG | Untouched (ecosystem-standard) |
| `.claude/settings.local.json` | JUNK | → `_attic/` (Phase 2; untracked agent cruft) |
| `local/**` (224 files, 9 MB) | JUNK | Deleted (Phase 2A: stale compiled-workbench test output, gitignored, regenerable) |
| `README.md` | DOC | Rewritten thin (Phase 3) |
| `AGENTS.md` | DOC | Rewritten → points at `specs/constitution.md` (Phase 4) |
| `OPTIMUS_AGENTS.md` | PRODUCT RUNTIME | Kept in place (instruction-plane firewall; loaded into installed product) |
| `SDD_MIGRATION.md` | DOC | Committed in snapshot; self-deletes at Phase 6 |
| `docs/decisions/**` (81) | DOC | STAYS as `docs/decisions/` (SDD end-state; ADR frontmatter is the repo standard) |
| `docs/architecture/system-overview.md` | DOC | → `docs/architecture.md` (merge) |
| `docs/architecture/desktop-install-relaunch.md` | DOC | → `docs/runbooks/install-relaunch.md` |
| `docs/architecture/*-verification*`, `north-star*`, `capability-baseline*`, `architecture-marks.md`, `product-complete-p23-verification.md` | DOC (records) | → `_attic/` (provenance; human decides) |
| `docs/architecture/module-size-baseline.json`, `parity-capability-ledger.json` | DATA | Kept in place (machine-read by gates) |
| `docs/current/roadmap.md` | DOC | → `specs/BACKLOG.md` (one line per item) |
| `docs/current/status.md` | DOC | Merged into specs + README |
| `docs/current/history-policy.md` | DOC | → constitution/conventions content |
| `docs/plans/**` (13) | DOC (records) | → `_attic/` (historical program plans) |
| `docs/evidence/**` (28) | DOC (records) | → `_attic/` |
| `docs/lessons/**` (1) | DOC (record) | → `_attic/` |
| `docs/history/**` (1) | DOC (record) | → `_attic/` |
| `docs/specifications/**` (31) | DOC | Current/planned → capability specs; historical → `_attic/` |
| `docs/contracts/**` (4) | DOC | Content → capability specs (host-ipc, runtime-effects) |
| `docs/maps/**` (6) | DOC | Content → capability specs (routing, memory, security, eval, dev-tooling) |
| `docs/design/**` (9) | DOC/ASSET | Design contracts → capability specs; mockup PNGs/HTML stay with them |
| `docs/contributing/**` (4) | DOC | → conventions.md / AGENTS.md content |
| `docs/agents/domain.md` | DOC | → AGENTS.md content or runbook |
| `docs/engineering-memory/README.md` | DOC | → `docs/runbooks/engineering-memory.md` |
| `docs/README.md` (router), `catalog.json`, `CATALOG.md`, `COMPONENTS.md`, `verification-lock.json` | DOC/GENERATED | Router merged into README; generated files regenerate |

## Capability map (Phase 3 plan, 5–15 specs)

1. `001-desktop-shell` — Tauri shell, React workbench, installer, wry rollback
2. `002-host-ipc` — Rust host, IPC registry, HTTP surface, bridge security
3. `003-kernel-turns` — turn loop, providers, sessions, routing, browser tools
4. `004-runtime-effects` — jobs, approvals/SmartDeny, cancellation, campaigns, high-risk contracts
5. `005-agents-workflows` — specialist agents, workflows, artifacts
6. `006-memory-skills-packs` — semantic memory, procedural skills, packs
7. `007-ops` — cron schedules, gateway, observability
8. `008-eval` — evaluation harnesses, baselines, fixture replay
9. `009-project-knowledge` — temporal project knowledge DB
10. `010-surfaces` — TUI + CLI faces
11. `011-developer-tooling` — gates, verify.sh, docs DB, EM lenses, ontology

Backlog (from roadmap/status): items not yet spec'd → `specs/BACKLOG.md`.

## Gate-adaptation plan (keep every gate green)

| Gate/script | Adaptation |
|---|---|
| `scripts/docs_system.py` | Scan `specs/` + `docs/`; exclude `_attic/`; retire doc_ids of atticked docs |
| `scripts/engineering_memory.py` | `EXCLUDED_PARTS` += `_attic` |
| `scripts/repository_ontology.py` + components JSON | Re-point component doc links; benchmark questions |
| `evals/docs-authority/questions-v1.json` + `evals/repository-orientation/questions-v1.json` | Re-point moved doc paths |
| `scripts/impact_select.py` + test | `PATH_SUITES` += `specs/`, `_attic/` |
| `scripts/check-instruction-planes.py` + test | Keep main-only markers in rewritten README/AGENTS.md; add constitution markers |
| `scripts/check-product-complete-install.py` | Re-point install doc path if it moves to runbooks |
| `scripts/project_hygiene.py`, `test_*` | Path updates |
| `.gitignore` | Drop dead `/local/` line (Phase 2); toolchain set already covers junk |

## Notes / decisions for the human

- `skills/` and `OPTIMUS_AGENTS.md` are deliberately NOT "documents" for the
  SDD end-state: skills are agent procedural tooling (repo convention,
  AGENTS.md), and OPTIMUS_AGENTS.md is product runtime constitution under the
  instruction-plane firewall. Both stay. Flagged for human confirmation.
- `.config/nextest.toml` stays (ecosystem-standard config, not a document).
- Machine-read data files (module-size baseline, parity ledger) stay in
  `docs/architecture/` — they are data, not documents.
- `_attic/` contents are quarantined, not deleted. Human reviews and empties.

## Phase checklist

- [x] Phase 1 — snapshot + inventory (this report; SDD_MIGRATION.md committed
      with the pre-migration snapshot as part of the Electron-retirement
      commit f3ef3c3)
- [ ] Phase 2 — purge junk, quarantine ambiguity
- [ ] Phase 3 — extract specs, consolidate docs (capability map above)
- [ ] Phase 4 — install constitution + conventions + AGENTS.md
- [ ] Phase 5 — mechanical formatting
- [ ] Phase 6 — verify and seal (all gates green, SDD_MIGRATION.md deleted)
