---
knowledge_type: decision
status: current
covers:
  - apps/optimus-desktop/src/main.rs
  - apps/optimus-desktop/src/server.rs
  - apps/optimus-desktop/src/ipc/**
  - apps/optimus-electron/**
  - apps/optimus-ui/**
depends_on:
  - docs/decisions/0014-native-webview-ipc-mode.md
  - docs/decisions/0015-preview-browser-cdp.md
validated_by:
  - apps/optimus-desktop/src/main.rs
  - apps/optimus-electron/package.json
last_verified_commit: null
---

# ADR-0028: Electron + React shell over Rust host

- **Status:** Accepted (migration in progress)
- **Date:** 2026-07-23

## Context

The product desktop shell is tao+wry with a single vanilla HTML document. T3
Code–class UX (React SPA, Electron chrome, multi-panel surface) needs a modern
frontend toolchain. Rewriting the durable agent core into Node/Effect would
discard Optimus’s Rust runtime advantages.

## Decision

1. **Frontend method** converges on the T3 Code shape:
   - Electron desktop host
   - React + Vite SPA
   - Local backend process for IPC
2. **Backend authority remains Rust.** The host reuses `handle_ipc`, chat
   workers, and the frozen method registry. Durable effects, SmartDeny, sessions,
   memory, and the turn loop stay in `optimus-kernel` / `optimus-runtime`.
3. **Transport** for the new shell is loopback HTTP + SSE (the existing
   Playwright host path), hardened with bearer token and CSRF. Electron preload
   may later add a native channel that still speaks the same method contract.
4. **ADR-0014** remains valid for the **legacy Wry** custom-protocol path. The
   Electron shell does not use Wry custom protocols; it loads the host origin.
5. **Strangler migration:**
   - Electron can load the assembled HTML via the Rust host.
   - React SPA replaces `app.js` behind the same IPC contract.
   - Install cutover makes Electron the default; Wry becomes optional/legacy.
6. **IPC method names and shapes are frozen** across the migration.

## Alternatives considered

- Keep Wry and only add React. Rejected as the long-term target (not T3 method).
- Move kernel to Node/Effect. Rejected: durability stays in Rust.
- Big-bang React before host/Electron. Rejected: prove transport first.

## Consequences

- Packages: `apps/optimus-electron`, `apps/optimus-ui`.
- Host-only mode is a first-class entry for Electron, not only a test seam.
- Install scripts and native-UI skill update at cutover with evidence.
