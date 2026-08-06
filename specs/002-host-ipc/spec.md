---
doc_id: spec-002-host-ipc
doc_type: reference
plane: work
status: current
authority: canonical
summary: The Rust host owns the frozen IPC registry, HTTP surface, and bridge security; the renderer surface is the typed DesktopMethod union over the Tauri host_invoke command.
reviewed_on: 2026-08-05
review_by: 2026-10-31
knowledge_type: specification
covers:
  - crates/optimus-host/src/**
  - apps/optimus-desktop/src/bridge.rs
  - apps/optimus-desktop/src/native_workers.rs
  - apps/optimus-ui/src/ipc/**
  - apps/optimus-tauri/src/main.rs
validated_by:
  - scripts/gates/check-surface-contract.py
  - scripts/tests/test_surface_contract.py
  - apps/optimus-desktop/e2e/**
---

# 002 — Host IPC

Status: active
Owner: development agents (main-only)

## Purpose

Define the frozen method surface every surface speaks: the Rust host registry
is authority, the renderer's typed `DesktopMethod` union is its declared
surface over the Tauri bridge, and non-invoke channels (chat stream, window,
folder picker) are explicit Tauri commands.

## Requirements

- R1. The host MUST own the registry (`METHOD_DOMAINS` in router.rs), session
  state, approvals, filesystem scopes, and every durable effect.
- R2. React `DesktopMethod` MUST be a subset of the host registry; the matrix
  gate MUST fail on any typed method missing from the registry.
- R3. Main-only methods (`project_root_stage_native`) MUST NOT appear in the
  renderer surface. Amended by spec-015 Phase A6 (the surface-protocol
  milestone): `project_root_stage_native` is a SHELL-GATED WIRE method —
  reachable on the wire ONLY from `client_kind:"shell"` connections
  presenting the staging process secret (spec-015 R2/R5/R7/R12), relayed
  by the shell over its own authenticated connection; it is NOT a
  `HOST_NON_INVOKE_CHANNELS` member (that const is deleted; the live gate
  carries it as its own shell-gated bucket).
- R4. Every registry method MUST be either wire-reachable, a documented
  non-wire channel, or explicitly superseded (no silent host methods).
  Amended by spec-015 Phase A6: the blocking chat family
  (`chat`/`chat_offline`/`chat_approval_resolve`) is SUPERSEDED by the
  streaming trio — not wire-reachable, not renderer-callable (spec-015 R2).
- R5. `host_invoke` MUST shrink to the shell-native allowlist (window
  chrome, folder picker — spec-001 R5) plus the staging relay, and MUST
  fail in the host on unknown or main-only methods. Amended by spec-015
  Phase A6: enforcement is the surface-contract gate's union rules + the
  renderer's move to the WebSocket carrier (spec-015 R11); the Tauri
  command itself remains a generic dispatcher.
- R6. Chat streaming MUST use the streaming trio (`chat_start`/`chat_cancel`/
  `chat_approval_resolve_start`) with exactly one terminal event per
  stream (done, error, cancelled). Amended by spec-015 Phase A6: the trio
  is promoted from Tauri commands to first-class wire methods over the
  serve protocol (spec-015 R2/R4); the blocking chat family is superseded.
- R7. `pick_folder` MUST stage a single-use project-root grant token; the
  native path exchange MUST go through `project_root_stage_native` with a
  process secret never sent to the renderer.
- R8. `OPTIMUS_HTTP_TOKEN` (>= 32 chars) MUST stay renderer-inaccessible;
  HTTP mode is loopback and development-only.

## Acceptance criteria
- [ ] A1. Given the current tree, when `scripts/gates/check-surface-contract.py` and its unit tests run, then they exit 0 with `SURFACE_CONTRACT_OK` (the surface-protocol gate, spec-015 A5).
- [ ] A2. Given the critical path list, when the renderer surface and host registry are compared, then no critical method (approvals, scopes, sessions, fs, settings, `term_run`, `jobs_list`) is missing.
- [ ] A3. Given the contract docs, when they are compared with the matrix gate output, then they match (no phantom methods or channels).

## Out of scope

- Kernel semantics (spec 003) and effect execution (spec 004).

## Open questions

- None.

## Links

Code: crates/optimus-host, apps/optimus-desktop/src/bridge.rs ·
Tests: check-surface-contract.py · ADRs: 0038, 0045 · Ontology:
optimus-host, optimus-ui
