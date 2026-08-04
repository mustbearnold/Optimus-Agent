---
doc_id: spec-001-desktop-shell
doc_type: reference
plane: work
status: current
authority: canonical
summary: The default desktop surface: Tauri v2 shell over the Rust host, React workbench renderer, and the Linux installer. Wry/Tao remains the legacy rollback shell.
reviewed_on: 2026-08-05
review_by: 2026-10-31
knowledge_type: specification
covers:
  - apps/optimus-tauri/src/**
  - apps/optimus-ui/src/**
  - apps/optimus-desktop/src/main.rs
  - scripts/rebuild-install-relaunch.sh
validated_by:
  - scripts/gates/check-tauri-launch.py
  - scripts/tests/test_tui_feature_matrix.py
  - apps/optimus-desktop/e2e/**
  - scripts/tests/test_rebuild_install_safety.py
---

# 001 — Desktop shell

Status: active
Owner: development agents (main-only)

## Purpose

Deliver the installed desktop experience: a Tauri v2 window hosting the React
workbench over the Rust host, packaged by the Linux installer, with the Wry/Tao
shell retained only as a legacy rollback surface.

## Requirements

- R1. The installed desktop entry MUST launch the Tauri binary
  (`optimus-agent-tauri`), not any other shell. [inferred]
- R2. The Tauri shell MUST run supervised under `--home`, `--supervised-ready`,
  and `--session` with a readiness marker and an instance log line
  `[optimus-tauri] ready ui=react`.
- R3. The shell MUST embed the built React bundle (`frontendDist`) at compile
  time; the UI build MUST precede the shell build in the gate tier.
- R4. The renderer MUST NOT receive `OPTIMUS_HTTP_TOKEN`; all durable effects
  flow through the host bridge (see spec 002).
- R5. Window chrome (minimize/maximize/close) and native folder selection
  MUST be Tauri commands, not host registry methods.
- R6. The installer MUST stage Tauri + host + CLI, register the desktop entry
  with `X-Optimus-UI=react-tauri`, and MUST NOT stage Electron or reference it.
- R7. `OPTIMUS_DESKTOP_SHELL=wry` MUST remain a documented legacy rollback;
  Electron rollback is retired.
- R8. The React workbench MUST auto-detect the transport: Tauri bridge when
  `window.__TAURI_INTERNALS__` is present, HTTP host mode for tests. [inferred]

## Acceptance criteria
- [ ] A1. Given a clean checkout on main with the Tauri shell built, when `scripts/gates/check-tauri-launch.py` runs, then it exits 0 and prints `TAURI_LAUNCH_OK` with a windowed surface.
- [ ] A2. Given the full gate spine, when `bash scripts/verify.sh all` runs, then the `tauri launch acceptance` tier passes and no electron tier is spawned.
- [ ] A3. Given an installed product, when `scripts/gates/check-product-complete-install.py` runs, then it reports `desktop_shell react-tauri` with no ElectronRollback action.
- [ ] A4. Given the desktop e2e suite, when Playwright drives the React workbench over the host, then all specs pass including a chat round-trip.

## Out of scope

- Windows packaging (Wry/WebView2 path remains; see the wry-fallback ontology
  row, review_by 2026-10-31).
- Electron in any form.

## Open questions

- Windows Tauri packaging (installer port) — tracked in BACKLOG.

## Links

Code: apps/optimus-tauri, apps/optimus-ui, apps/optimus-desktop ·
Tests: e2e + check-tauri-launch.py · ADRs: 0028, 0029, 0051 · Ontology:
optimus-tauri (primary), desktop-wry-fallback (rollback)
