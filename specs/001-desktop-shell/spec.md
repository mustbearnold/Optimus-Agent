---
doc_id: spec-001-desktop-shell
doc_type: reference
plane: work
status: current
authority: canonical
summary: The default desktop surface: Tauri v2 shell over the Rust host, React workbench renderer, and the Linux installer. The desktop product is exclusively Tauri.
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
workbench over the Rust host, packaged by the Linux installer. The desktop
product is exclusively Tauri — no Electron, no Wry rollback shell.

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
  MUST be Tauri commands, not host registry methods. Window chrome MUST be
  reachable from the renderer regardless of which surface carrier it is on
  (spec-015 A3): the WS transport has no window protocol, so chrome actions
  reach the shell bridge directly (`windowBridge.ts`) instead of riding the
  wire. [inferred]
- R6. The installer MUST stage Tauri + CLI, register the desktop entry
  with `X-Optimus-UI=react-tauri`, and MUST NOT stage Electron or reference
  it. The Wry rollback action (`LegacyWry`) and `OPTIMUS_DESKTOP_SHELL`
  dispatch are retired: the installed product is exclusively Tauri.
- R7. The desktop entry MUST NOT expose a rollback shell action: neither
  Electron nor Wry. `check-product-complete-install.py` forbids both
  `ElectronRollback` and `LegacyWry`.
- R8. The React workbench MUST auto-detect the transport: WebSocket when a
  broker ticket global is present (spec-015 A3/R7); otherwise the Tauri
  bridge when `window.__TAURI_INTERNALS__` is present; HTTP host mode for
  tests (dev-only). Amended by spec-015 Phase A6: in the packaged app a
  confirmed broker absence (Tauri bridge present, broker answered no
  ticket) selects NO transport and surfaces the terminal affordance —
  never a silent fixture; the packaged-vs-dev discriminator is
  `window.__TAURI_INTERNALS__` presence. [inferred]

## Acceptance criteria
- [x] A1. Given a clean checkout on main with the Tauri shell built, when `scripts/gates/check-tauri-launch.py` runs, then it exits 0 and prints `TAURI_LAUNCH_OK` with a windowed surface. (proven 2026-08-05: `TAURI_LAUNCH_OK version=0.1.0 window=yes`)
- [x] A2. Given the full gate spine, when `bash scripts/verify.sh all` runs, then the `tauri launch acceptance` tier passes and no electron tier is spawned. (proven 2026-08-05: verify 61/61, no electron anywhere)
- [x] A3. Given an installed product, when `scripts/gates/check-product-complete-install.py` runs, then it reports `desktop_shell react-tauri` with no ElectronRollback action. (proven 2026-08-05: `PRODUCT_COMPLETE_INSTALL_OK desktop_shell=react-tauri`)
- [x] A4. Given the desktop e2e suite, when Playwright drives the React workbench over the host, then all specs pass including a chat round-trip. (proven 2026-08-05: desktop e2e 62/62)
- [x] A5. Given the installed desktop app with the broker up (renderer on the WS carrier), when the window control buttons are clicked and the window edges/corners are dragged, then minimize/maximize/close reach the shell bridge and resize starts a native compositor drag. (proven 2026-08-07: `windowBridge` + `window_action`/`window_resize_start` unit tests 11/11; installed-app verification via the WebKit inspector: DOM clicks on the three controls produced Iconic state, maximized 3440x1400, restored 1280x840, and process exit; `window_resize_start` answered `ok:true` and the hotspot pointerdown dispatched; live pointer-drag motion was not exercised because this host has no working pointer injection on Wayland)

## Out of scope

- Electron in any form.
- Windows packaging details — covered by `specs/012-windows-tauri-packaging/spec.md`.

## Evidence ceiling (renderer verification)

There is no playwright-class driver for the WebKitGTK webview that Tauri
uses, so the renderer's native pixels cannot be scripted. The accepted
evidence bar is therefore: the `tauri launch acceptance` gate (real window
on a live display) + `tauriTransport` unit tests (the bridge contract) +
desktop e2e over the HTTP host (62/62, including a chat round-trip) + the
WebKit layout audit (`ui_layout_audit.cjs`). This ceiling is accepted and
recorded; a WebKit driver would raise it (see BACKLOG).

## Open questions

- Renderer-pixel proof under Tauri — see the evidence ceiling above; a
  WebKitGTK driver would raise it (tracked in BACKLOG).

## Links

Code: apps/optimus-tauri, apps/optimus-ui, apps/optimus-desktop ·
Tests: e2e + check-tauri-launch.py · ADRs: 0028, 0029, 0051 · Ontology:
optimus-tauri (primary, exclusive)
