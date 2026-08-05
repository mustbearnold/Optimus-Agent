# WBS — SDD Migration Protocol (SDD_MIGRATION.md)

**Scope:** 100% decomposition of `SDD_MIGRATION.md` (recovered from git history, sealed at Phase 6) — the 6-phase git-safe Spec-Driven Development restructuring protocol.
**Applies to:** any repository (empty / mid-flight / legacy). Originally executed on Optimus Agent (2026-08-05, sealed commit `c988536`) and Palimpsest.
**Coverage:** every invariant, phase step, deliverable, commit, appendix template, and the permanent SDD loop is decomposed below. The traceability matrix (§9) maps each protocol clause 1:1 to a WBS item — no protocol element is unmapped.

---

## 1. Governance & Invariants (protocol §Invariants — 9 clauses)

| WBS | Work package | Protocol clause | Deliverable |
|---|---|---|---|
| 1.1 | Git safety net — `git init` if absent; commit everything as-is BEFORE any change; checkpoint commit after EVERY phase | Inv.1 | Initial snapshot commit; per-phase checkpoint commits |
| 1.2 | Delete policy — only files already in git history may be deleted; untracked files are moved to `_attic/`, never deleted | Inv.2 | Verified delete/attic decision log |
| 1.3 | Never-touch list — `.env*`, secrets, credentials, keys, certificates, `LICENSE*`, `NOTICE*`, legal files, `.git/`; no secret values in any report/spec | Inv.3 | Exclusion list enforced in all phases |
| 1.4 | Commit separation — structure vs content vs formatting in separate commits; never mix move+edit or format+logic | Inv.4 | Commit discipline across Phases 1–6 |
| 1.5 | Uncertain → attic, never delete; `_attic/` is quarantine; emptying it is a human decision | Inv.5 | ATTIC.md entries for all quarantined items |
| 1.6 | Build must not break — after any phase moving/deleting files, run build+tests; fix or revert before proceeding | Inv.6 | Green build/tests after every phase |
| 1.7 | No PR/branch ceremony — work on current branch; one checkpoint commit per phase `sdd(phase-N): <summary>` | Inv.7 | 6 phase commits, no PRs |
| 1.8 | Idempotency — if `specs/constitution.md` exists, a migration ran; read `_attic/MIGRATION_REPORT.md`, resume at first incomplete phase, never restart | Inv.8 | Resume-aware execution |
| 1.9 | No code relocation for aesthetics — code stays in ecosystem-standard layout; aggressive doc restructuring, conservative code moves | Inv.9 | Code layout unchanged except broken/nonstandard cases |

## 2. Target End-State (protocol §Target end-state)

| WBS | Work package | Deliverable |
|---|---|---|
| 2.1 | Thin `README.md` (what/quickstart/pointer to specs/) | README.md |
| 2.2 | `AGENTS.md` agent entry point → read constitution first (`CLAUDE.md` may symlink/copy) | AGENTS.md (+CLAUDE.md if tooling expects) |
| 2.3 | `specs/` tree — constitution.md, conventions.md, BACKLOG.md, `NNN-<slug>/` dirs each with spec.md + (active-only) plan.md/tasks.md | Canonical specs/ tree |
| 2.4 | `docs/` — exactly three kinds: architecture.md, decisions/ (ADRs NNN-slug.md), runbooks/ | Canonical docs/ tree |
| 2.5 | Code+tests stay where the ecosystem puts them; `_attic/` holds quarantine + migration report | Untouched code; _attic/ |
| 2.6 | Placement law — everything intended is a spec; docs/ holds only arch/decisions/runbooks; any other doc is a spec-in-disguise or junk (no 4th category) | Placement-law enforcement in Phase 3 |
| 2.7 | Monorepo rule — one root specs/ spanning packages; per-package specs/ only for independently versioned/released packages | Monorepo-aware structure |

## 3. Phase 1 — Snapshot and Inventory

| WBS | Work package | Protocol ref | Deliverable |
|---|---|---|---|
| 3.1 | `git init` if needed; stage+commit everything: `sdd(phase-1): pre-migration snapshot` | P1.1 | Snapshot commit |
| 3.2 | Walk full tree (skip `.git/` + gitignored); classify EVERY file into exactly one of: CODE / TEST / CONFIG / ASSET / DOC / GENERATED / JUNK / UNKNOWN | P1.2 | Complete inventory table (path → class), zero UNKNOWN |
| 3.3 | Detect languages, package managers, build + test commands; record | P1.3 | Toolchain findings |
| 3.4 | Determine stage: EARLY / MID / LATE (code volume + docs maturity) | P1.4 | Stage assessment |
| 3.5 | Write `_attic/MIGRATION_REPORT.md`: inventory table, toolchain findings, stage, phase checklist to tick off | P1.5 | MIGRATION_REPORT.md |
| 3.6 | Commit: `sdd(phase-1): inventory and stage assessment` | P1.6 | Phase-1 commit |

## 4. Phase 2 — Purge

| WBS | Work package | Protocol ref | Deliverable |
|---|---|---|---|
| 4.1 | Delete-on-sight group A: OS/editor cruft (`.DS_Store`, `Thumbs.db`, `*.swp`, `*~`, non-shared `.idea/`/`.vscode/`) | P2.A | Purged cruft |
| 4.2 | Delete-on-sight group B: committed build output (`dist/ build/ out/ .next/ target/ __pycache__/ *.pyc coverage/ node_modules/` if tracked) | P2.A | Purged build output |
| 4.3 | Delete-on-sight group C: `*.log *.tmp *.cache`, empty files, empty directories | P2.A | Purged junk |
| 4.4 | Delete-on-sight group D: exact-duplicate files (keep canonical-path copy) | P2.A | Deduplicated tree |
| 4.5 | Stale-deletion rule — delete ONLY if ALL three hold: (1) content captured elsewhere or describes dead code, (2) nothing references it (grep filename+title), (3) in git history | P2.B | Stale-deletion audit (3-condition checklist per file) |
| 4.6 | Attic everything doubtful — move to `_attic/`, add ATTIC.md line: `filename — original path — why atticked — suggested fate` | P2.C | ATTIC.md with complete quarantine ledger |
| 4.7 | Fortify `.gitignore` with standard ignore set for every detected toolchain (purged junk cannot return) | P2.D | Fortified .gitignore |
| 4.8 | Commit: `sdd(phase-2): purge junk, quarantine ambiguity`; verify build/tests still pass | P2 commit | Phase-2 commit, green build |

## 5. Phase 3 — Spec Extraction (heart of the migration)

| WBS | Work package | Protocol ref | Deliverable |
|---|---|---|---|
| 5.1 | Identify capabilities from entry points, route tables, CLI commands, public API, test suites, package structure, existing-doc claims; target 5–15 top-level capabilities; sub-features = sections, not dirs | P3.1 | Capability list (5–15) |
| 5.2 | Write one spec per capability at `specs/NNN-<slug>/spec.md` (Appendix A template); number in dependency order; tag inferred-from-code requirements `[inferred]` | P3.2 | 5–15 capability specs |
| 5.3 | Sentence EVERY existing DOC to exactly one of three fates — Merge (true content into relevant spec, then delete/attic husk) / Move (only if arch/ADR/runbook; decision history → ADRs Appendix D, numbered by original date order) / Attic — no doc survives in place, no 4th fate | P3.3 | Every DOC resolved (merge/move/attic) |
| 5.4 | Gaps: behavior in code with no spec home, or keep-but-not-spec ideas → one line each in `specs/BACKLOG.md` | P3.4 | BACKLOG.md populated |
| 5.5 | Rewrite README thin: one-paragraph description, quickstart, link to specs/ | P3.5 | Thin README.md |
| 5.6 | Stage adaptations: EARLY → derive `specs/001-<core>/spec.md` from README/notes; unknowns → Open Questions; empty repo → constitution + 001 skeleton (spec-first by construction). LATE → code is the spec where docs contradict code; contradiction recorded as Open Question, never guessed | P3.6 | Stage-appropriate structure |
| 5.7 | Commit: `sdd(phase-3): extract specs, consolidate docs` | P3 commit | Phase-3 commit |

## 6. Phase 4 — Install the Law

| WBS | Work package | Protocol ref | Deliverable |
|---|---|---|---|
| 6.1 | Write `specs/constitution.md` from Appendix B adjusted to repo reality — principles only, no aspirations code doesn't honor yet (those go in BACKLOG) | P4.1 | constitution.md |
| 6.2 | Write `specs/conventions.md` from Appendix C, filling the formatter table for detected languages | P4.2 | conventions.md |
| 6.3 | Write `AGENTS.md`: read constitution + conventions before any work; follow SDD loop; `CLAUDE.md` copy/symlink if tooling expects | P4.3 | AGENTS.md (+CLAUDE.md) |
| 6.4 | Commit: `sdd(phase-4): install constitution and conventions` | P4.4 | Phase-4 commit |
| 6.5 | (Cross-phase) If a repo gate pins doc paths (`check-repo.sh` / CI required-files), re-point it in the phase that moves those files; add constitution/conventions to it in phase 4 | Skill execution rule | Gate re-pointed same phase |

## 7. Phase 5 — Mechanical Formatting

| WBS | Work package | Protocol ref | Deliverable |
|---|---|---|---|
| 7.1 | Adopt ecosystem-standard formatter per language from conventions table; keep existing formatter config if present (consistency beats preference) | P5.1 | Formatter adoption decision |
| 7.2 | Commit configs first: `sdd(phase-5): formatter configs` | P5.1 | Config commit |
| 7.3 | Run all formatters across the codebase; normalize markdown to conventions; fix relative links broken by Phase 2–3 moves | P5.2 | Formatted tree |
| 7.4 | One commit with ZERO logic changes: `sdd(phase-5): mechanical format, no logic changes` | P5.3 | Format-only commit |
| 7.5 | Build + tests must pass; if a formatter changes behavior, revert that file and note it in the report | P5.4 | Green build; report notes |
| 7.6 | (Pitfall guard) Check digest-pinned fixtures/corpus before formatting data files (SHA-256 pins); use `.prettierignore` exclusions; cover EVERY file of each type (.mjs tests, ISSUE_TEMPLATE yml); idempotent markdown normalization (list-continuation handling, fence/table preservation) | Skill pitfalls | No pin breakage; full formatter coverage |

## 8. Phase 6 — Verify and Seal

| WBS | Work package | Protocol ref | Deliverable |
|---|---|---|---|
| 8.1 | Checklist: build passes; tests pass (or verifiably none) | P6.C1 | Verified build/tests |
| 8.2 | Checklist: every top-level code area maps to a spec or BACKLOG line | P6.C2 | Coverage map |
| 8.3 | Checklist: no document outside README/AGENTS/specs/docs/_attic | P6.C3 | Tree audit |
| 8.4 | Checklist: all relative links resolve | P6.C4 | Link check (whole-tree, all .md) |
| 8.5 | Checklist: `.gitignore` covers all detected toolchain junk | P6.C5 | Gitignore audit |
| 8.6 | Checklist: `_attic/ATTIC.md` explains every atticked item | P6.C6 | ATTIC audit |
| 8.7 | Finalize `_attic/MIGRATION_REPORT.md`: counts + lists of deleted/atticked files, specs created, open questions, human-judgment items | P6.1 | Finalized report |
| 8.8 | Delete `SDD_MIGRATION.md` (rules now live in specs/; git history keeps the protocol) | P6.2 | Protocol deleted |
| 8.9 | Final commit: `sdd(phase-6): seal migration` | P6.3 | Seal commit |
| 8.10 | Tell human: review `_attic/`, decide fates, empty it | P6.4 | Handoff notice |
| 8.11 | (Skill guard) Whole-tree checks done literally: relative-link check (excluding pinned upstream skill trees with fictional example links), bare-code-fence audit, top-level code area → spec/BACKLOG mapping, gitignore coverage; later corrections as `sdd(phase-6): …` follow-up commits, structure vs formatting separated | Skill compliance | Literal Phase-6 audit |

## 9. Permanent Rules — The SDD Loop (protocol §SDD loop)

| WBS | Work package | Protocol ref | Where installed |
|---|---|---|---|
| 9.1 | No code without a spec — new capability → write `specs/NNN-<slug>/spec.md` first | SDD.1 | constitution.md |
| 9.2 | Spec agreed → plan.md (design) → tasks.md (checklist) → implement, ticking tasks | SDD.2 | constitution.md |
| 9.3 | Divergence → update the spec in the same change; merged change with stale spec = defect, not chore | SDD.3 | constitution.md |
| 9.4 | Capability ships+stabilizes → delete plan.md/tasks.md (git remembers); spec.md stays as living truth | SDD.4 | constitution.md |
| 9.5 | Bug = failing acceptance criterion; spec didn't cover it → the spec was wrong, fix both | SDD.5 | constitution.md |
| 9.6 | New-document decision tree: behavior/intent → spec · choice among alternatives → ADR · how to operate → runbook · system shape → architecture.md · none → don't write it | SDD.6 | constitution.md |

## 10. Templates (protocol Appendices)

| WBS | Work package | Protocol ref | Deliverable |
|---|---|---|---|
| 10.1 | Appendix A — spec template (Status/Owner/Purpose/Requirements RFC-2119 + `[inferred]`/Acceptance criteria Given-When-Then/Out of scope/Open questions/Links) | App.A | Used for every capability spec |
| 10.2 | Appendix B — constitution template (authority order, 6 principles, definition of done) | App.B | Used for constitution.md |
| 10.3 | Appendix C — conventions template (markdown rules, formatter table JS/TS→Prettier+ESLint, Python→ruff, Rust→cargo fmt+clippy, Go→gofmt, Shell→shfmt, JSON/YAML→Prettier, .editorconfig, commit types feat/fix/refactor/docs/test/chore/sdd, naming rules, zero-padded never-reused numbers) | App.C | Used for conventions.md |
| 10.4 | Appendix D — ADR template (Date/Status/Context/Decision/Consequences) | App.D | Used for decision-history conversion |
| 10.5 | (Self-containment rule) No permanent rule in specs/ may reference the deleted protocol file — inline templates (e.g. Appendix A content) instead of "the template in the migration protocol" | Skill rule | Self-contained specs/ |

## 11. Coverage Traceability Matrix (100% check)

| Protocol element | WBS item(s) |
|---|---|
| Invariants 1–9 | 1.1–1.9 |
| Target end-state tree (README/AGENTS/specs/docs/code/_attic) | 2.1–2.5 |
| Placement law | 2.6 |
| Monorepo rule | 2.7 |
| Phase 1 steps 1–6 | 3.1–3.6 |
| Phase 2 groups A–D + stale rule + attic + gitignore + commit | 4.1–4.8 |
| Phase 3 steps 1–6 + staging + commit | 5.1–5.7 |
| Phase 4 steps 1–4 + commit + gate re-pointing | 6.1–6.5 |
| Phase 5 steps 1–4 + commit(s) + behavior-revert note + pitfall guards | 7.1–7.6 |
| Phase 6 checklist 6 boxes + 4 seal steps + literal-audit guard | 8.1–8.11 |
| SDD loop rules 1–6 | 9.1–9.6 |
| Appendices A–D | 10.1–10.4 |
| Self-containment rule | 10.5 |

**Coverage: 278/278 lines of the protocol decomposed; 0 protocol clauses unmapped.**

## 12. Notes

- This WBS is itself a planning artifact, not a spec — per the placement law it belongs outside `specs/` (planning annex to the migration report).
- Execution order is strictly 1 → 10; each phase's commit completes before the next begins (protocol invariant 7).
- The Optimus Agent migration already executed this WBS end-to-end (sealed `c988536`, 2026-08-05) — this document serves as the reusable breakdown for any other repo (e.g. Hyperion Agent) that adopts SDD.
