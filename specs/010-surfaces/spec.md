---
doc_id: spec-010-surfaces
doc_type: reference
plane: work
status: current
authority: canonical
summary: The terminal and CLI faces — the TUI (real binary, pty-driven gates) and the optimus-cli surface for jobs, approvals, skills, packs, chat, sessions, auth, cron, browser, gateway, evals, and campaigns.
reviewed_on: 2026-08-05
review_by: 2026-10-31
knowledge_type: specification
covers:
  - apps/optimus-tui/src/**
  - apps/optimus-cli/src/**
validated_by:
  - scripts/tools/tui_e2e.py
  - scripts/tools/tui_feature_matrix.py
  - scripts/tests/tui_layout_playwright.cjs
---

# 010 — Surfaces: TUI and CLI

Status: active
Owner: development agents (main-only)

## Purpose

The non-desktop faces of the product: the native TUI (tmux-driven real-binary
gates) and the CLI. Both speak the same host methods; the TUI has its own
branded identity and the CLI hosts the loopback webhook gateway.

## Requirements

- R1. The TUI gates MUST drive the real binary in tmux with the offline
  scripted provider (`OPTIMUS_OFFLINE_LATENCY_MS` pacing) and MUST assert on
  durable state, not transient phase labels.
- R2. Box-extraction predicates MUST anchor to box columns, never pane rows
  (a sidebar row sharing the composer's bottom border must not enter the
  composer text).
- R3. The CLI MUST expose jobs, approvals, skills, packs, chat, sessions,
  auth, cron, browser, gateway, evals, and campaigns; it also hosts a
  loopback webhook gateway.
- R4. Every long-running CLI/TUI operation MUST support cancellation with one
  terminal outcome.

## Acceptance criteria
- [ ] A1. Given a tmux session with the real TUI binary and the offline provider, when `tui_e2e.py`, `tui_feature_matrix.py` (16 cases, 160 checks), and `tui_layout_playwright.cjs` run, then all pass.
- [ ] A2. Given the CLI integration suite, when it runs, then all tests pass.

## Out of scope

- Desktop shell (spec 001).

## Open questions

- None.

## Links

Code: apps/optimus-tui, apps/optimus-cli · Tests: pty gates + cli tests ·
Ontology: optimus-tui, optimus-cli
