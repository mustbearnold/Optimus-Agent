---
doc_id: spec-011-developer-tooling
doc_type: reference
plane: work
status: current
authority: canonical
summary: The repository's own meta-capability — the verify gate spine, docs DB, Engineering Memory lenses, ontology, instruction-plane firewall, and main-only law enforcement that keep a prompt-only project honest.
reviewed_on: 2026-08-05
review_by: 2026-10-31
knowledge_type: specification
covers:
  - scripts/**
  - justfile
  - .githooks/**
  - AGENTS.md
  - evals/repository-orientation/**
  - evals/docs-authority/**
depends_on:
  - docs/decisions/0049-module-size-is-measured-honestly.md
  - docs/decisions/0062-source-and-development-are-separate-workspace-planes.md
  - docs/decisions/0063-documentation-is-a-governed-authority-plane.md

validated_by:
  - scripts/verify.sh
  - scripts/tests/test_verify_gate_parity.py
  - scripts/tests/test_instruction_planes.py
  - scripts/tests/test_verify_skip_report.py
---

# 011 — Developer tooling

Status: active
Owner: development agents (main-only)

## Purpose

The gates and law that make a prompt-only, agent-maintained repository
trustworthy: `scripts/verify.sh` as the single land gate, the docs DB and
Engineering Memory as anti-staleness mechanisms, the instruction-plane
firewall, and the main-only delivery law enforced by `.githooks/`.

## Requirements

- R1. `bash scripts/verify.sh all` MUST be the single source of truth; gates
  run through `just`, never as hand-typed command lists; `scripts/verify.sh`
  is the only place new gates are added.
- R2. Gates MUST NOT be weakened or deleted to pass; the underlying truth is
  fixed instead. Gate parity between `tier_gates` and `tier_all` is tested.
- R3. The instruction-plane firewall MUST hold: development instructions
  (autonomy, orchestration, model selection, permissions, VCS, testing) never
  leak into `OPTIMUS_AGENTS.md`, product prompts, or runtime policy.
- R4. Main-only development is enforced by `.githooks/` (off-main commits
  blocked, forced return to main, branch create/move refused); no ceremony
  markers (`just land`, worktrees, checkpoint) may reappear in README or the
  justfile.
- R5. The docs DB MUST catalog every doc in `specs/` and `docs/`; Engineering
  Memory MUST exclude `_attic/`, `Development/`, and generated state; every
  tracked top-level root MUST be classified in the ontology.
- R6. Package-manager law: Cargo for Rust, Bun for JS/TS — foreign lockfiles
  are gate failures.
- R7. No module exceeds 800 production lines (ratchet, ADR-0049); baseline
  entries never grow by hand.

## Acceptance criteria
- [ ] A1. Given the managed path, when `bash scripts/verify.sh all` runs, then it passes with zero skips.
- [ ] A2. Given both tiers, when the gate parity test runs, then static and self-test spawns are identical.
- [ ] A3. Given the firewall and ontology, when the instruction-plane gate and tests and the ontology benchmark run, then all pass (benchmark 11/11).

## Out of scope

- Product runtime behavior (the firewall's other side).

## Open questions

- None.

## Links

Code: scripts/verify.sh, scripts/tools/docs_system.py, scripts/tools/engineering_memory.py,
scripts/tools/repository_ontology.py, .githooks · Tests: gate self-tests · ADRs:
0049, 0062 (historical) · Ontology: repository-root, development-hooks
