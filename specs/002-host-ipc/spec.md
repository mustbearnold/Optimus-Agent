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
  - scripts/check-desktop-ipc-matrix.py
  - scripts/test_desktop_ipc_matrix.py
  - apps/optimus-desktop/e2e/**
  - apps/optimus-ui/src/ipc/**/*.test.ts
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
  renderer surface and MUST be listed in `HOST_NON_INVOKE_CHANNELS`.
- R4. Every registry method MUST be either renderer-callable or a documented
  non-invoke channel (no silent host methods).
- R5. `host_invoke` MUST forward any registry method and fail in the host on
  unknown or main-only methods.
- R6. Chat streaming MUST use `chat_start`/`chat_cancel` Tauri commands with
  exactly one terminal event per stream (done, error, cancelled).
- R7. `pick_folder` MUST stage a single-use project-root grant token; the
  native path exchange MUST go through `project_root_stage_native` with a
  process secret never sent to the renderer.
- R8. `OPTIMUS_HTTP_TOKEN` (>= 32 chars) MUST stay renderer-inaccessible;
  HTTP mode is loopback and development-only.

## Acceptance criteria
- [ ] A1. Given the current tree, when `scripts/check-desktop-ipc-matrix.py` and its unit tests run, then they exit 0 with `DESKTOP_IPC_MATRIX_OK`.
- [ ] A2. Given the critical path list, when the renderer surface and host registry are compared, then no critical method (approvals, scopes, sessions, fs, settings, `term_run`, `jobs_list`) is missing.
- [ ] A3. Given the contract docs, when they are compared with the matrix gate output, then they match (no phantom methods or channels).

## Out of scope

- Kernel semantics (spec 003) and effect execution (spec 004).

## Open questions

- None.

## Links

Code: crates/optimus-host, apps/optimus-desktop/src/bridge.rs ·
Tests: check-desktop-ipc-matrix.py · ADRs: 0038, 0045 · Ontology:
optimus-host, optimus-ui
