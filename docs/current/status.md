---
doc_id: current-status
doc_type: reference
plane: current
status: current
authority: canonical
summary: Optimus Agent is a local, modular assistant intended to become broadly excellent across useful domains. It is not a coding-only agent, a project-builder product, or a single giant prompt. Individual tests and project journeys exercise...
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: current-state
owns:
  - docs/current/status.md
watches:
  - Cargo.toml
  - apps/**
  - crates/**
  - OPTIMUS_AGENTS.md
covers:
  - Cargo.toml
  - apps/**
  - crates/**
depends_on:
  - docs/architecture/system-overview.md
validated_by:
  - scripts/check-crate-layers.py
  - scripts/check-tool-coverage.py
  - scripts/check-observability-gate.py
---

# Optimus Agent today

Optimus Agent is a local, modular assistant intended to become broadly excellent
across useful domains. It is not a coding-only agent, a project-builder product,
or a single giant prompt. Individual tests and project journeys exercise
capabilities; they do not redefine the product.

## Confirmed current behaviour

- Native TUI, Electron/React desktop, legacy Wry shell, and CLI surfaces use a
  Rust host and provider-agnostic kernel.
- Codex OAuth, OpenAI-compatible providers, deterministic offline models, Auto
  routing, durable sessions, streamed lifecycle events, and cancellation seams
  exist.
- Versioned tools and packs cover bounded filesystem work, commands, web and
  browser operations, memory, skills, schedules, gateway work, and registered
  specialist/workflow verticals.
- Work Graph, SQLite stores, exact terminal outcomes, effect receipts,
  confinement, SmartDeny approvals, recovery, and causal observability exist.
- Installed-product instructions and repository-development instructions are
  separate authority planes.

The detailed component table and qualifications live in the
[system overview](../architecture/system-overview.md). Source and executable
tests outrank this summary whenever they disagree.

## Important incomplete capabilities

- General model-chosen specialist orchestration and open-ended parallel child
  execution are not complete.
- Approval UX still creates unnecessary friction for harmless confined work in
  observed TUI project journeys.
- Broad real-world integrations and end-to-end domain coverage remain uneven.
- Longitudinal conversation continuity and evaluation calibration require more
  live evidence.
- Local memory, session state, project knowledge, retrieval indexes, skills and
  Engineering Memory remain distinct systems rather than one universal memory.

## Development status

Repository changes use assigned `Development/worktrees/*` checkouts and reach
main only through `just land`. GitHub issues, pull requests, `gh`, and manual Git
history commands are not the current repository-development workflow. These
development rules do not become Optimus product behaviour.
