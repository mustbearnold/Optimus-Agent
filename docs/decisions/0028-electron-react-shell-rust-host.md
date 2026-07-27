---
knowledge_type: decision
status: current
covers:
  - apps/optimus-desktop/src/main.rs
  - apps/optimus-desktop/src/server.rs
  - crates/optimus-host/src/**
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

## Reasons

The split preserves Rust’s durable effect authority while allowing a modern
desktop presentation and native preview boundary. A strangler keeps the Wry
shell as rollback and lets transport, UI, and installation evidence mature
independently.

## Risks and unresolved boundaries

- **Planned behaviour:** installed packaging and desktop-entry cutover require
  explicit install/relaunch verification.
- **Unknown or unresolved behaviour:** active stream replay after renderer
  refresh is not implemented.
- **Unknown or unresolved behaviour:** the user preview and Rust Browser
  effector do not share browser state.

## Evaluation evidence

- The React production build loads from the custom `optimus-app://ui/`
  protocol without exposing the host bearer token.
- Browser-contract tests cover responsive UI states.
- Compiled Electron tests cover offline Rust chat/cancel, native preview paint,
  pointer input, geometry, annotations, and overlay lifecycle.

## Relevant code

- `apps/optimus-electron/main.cjs`
- `apps/optimus-electron/preload.cjs`
- `apps/optimus-ui/src/app/OptimusApp.tsx`
- `apps/optimus-desktop/src/server.rs`

## Relevant tests

- `apps/optimus-electron/test/browser-policy.test.cjs`
- `apps/optimus-electron/e2e/react-browser-contract.spec.cjs`
- `apps/optimus-electron/e2e/compiled-shell.spec.cjs`
- `apps/optimus-ui/src/ipc/electronTransport.test.ts`

## Conditions for reconsideration

Reconsider Electron if a Rust-native shell can provide equivalent modern
surface composition, isolated remote pixels, and the same verification depth.
Retire Wry rollback only after installed packaging, migration, accessibility,
and recovery evidence are green.

## 2026-07-24 implementation addendum

ADR-0029 completed the repository-level React/default-protocol cutover and
replaced direct renderer HTTP authority with a context-isolated bounded preload.
ADR-0030 now owns the Codex-measured shell, local multi-folder project catalog,
one-shot native annotations, and native-view suspension under renderer
overlays. Rust authority and the frozen desktop method registry remain
unchanged.
