---
doc_id: spec-008-eval
doc_type: reference
plane: work
status: current
authority: canonical
summary: Offline integrity and trajectory evaluation — versioned baselines, zero-effect fixture replay, and the repository-orientation and docs-authority agent evals.
reviewed_on: 2026-08-05
review_by: 2026-10-31
knowledge_type: specification
covers:
  - crates/optimus-eval/src/**
  - evals/**
depends_on:
  - docs/decisions/0063-documentation-is-a-governed-authority-plane.md

validated_by:
  - crates/optimus-eval/tests/**
  - scripts/tools/repository_ontology.py
  - scripts/tools/docs_system.py
---

# 008 — Evaluation

Status: active
Owner: development agents (main-only)

## Purpose

Prove behavior with executable evidence: offline integrity/trajectory
harnesses, versioned evaluation reports and baselines, zero-effect fixture
replay, and the fresh-agent orientation evals that measure whether a new
session can navigate the repository.

## Requirements

- R1. Evaluations MUST be offline and zero-effect: fixtures replay without
  network or durable side effects.
- R2. Baselines MUST be versioned; a grade moves only with source + tests +
  docs exit criteria, never because a commit landed.
- R3. `evals/repository-orientation` and `evals/docs-authority` MUST stay
  green — they are the self-test that a fresh agent can find the law.
- R4. Evaluation reports MUST be immutable records (never rewritten).

## Acceptance criteria
- [ ] A1. Given the eval crate suite and both benchmarks, when they run, then the ontology benchmark is 11/11 and the docs benchmark is 100% top-one.
- [ ] A2. Given a fixture replay, when the neutral-fixtures gate runs, then no side effects are left behind.

## Out of scope

- Runtime observability (spec 007).

## Open questions

- None.

## Links

Code: crates/optimus-eval, evals · Tests: eval crate + ontology/docs
benchmarks · Ontology: optimus-eval
