---
knowledge_type: decision
status: current
covers:
  - apps/optimus-electron/main.cjs
  - apps/optimus-ui/src/app/OptimusApp.tsx
  - apps/optimus-desktop/src/main.rs
  - crates/optimus-browser/src/lib.rs
depends_on:
  - docs/decisions/0015-preview-browser-cdp.md
  - docs/decisions/0028-electron-react-shell-rust-host.md
  - docs/decisions/0029-react-workbench-and-electron-preview-view.md
  - docs/decisions/0040-shared-browser-contract.md
  - docs/decisions/0045-agent-host-and-surface-transports.md
validated_by:
  - scripts/check-desktop-ipc-matrix.py
  - scripts/check-module-size.py
last_verified_commit: null
---

# ADR-0051: Electron now, Tauri when the preview leaves the shell

- **Status:** Proposed
- **Date:** 2026-07-29
- **Program:** program P30+ (TUI + core foundation)

## Context

ADR-0028 chose Electron + React over the tao/wry shell and is still marked
*"Accepted (migration in progress)"*. Both front ends are live and both are
gated; #106 tracks finishing that migration. The question this ADR answers is
the one ADR-0028 left open: **does the shell stay Electron, and what decides?**

Current practice as of this date is unambiguous. The consensus for a new
desktop app in 2026 is Tauri v2 unless a specific exception applies —
installer ~8MB against Electron's ~165MB, idle memory ~45MB against ~180MB,
cold start ~1.4s against ~3.2s, Tauri repository growth ~55% year over year
while Electron's has plateaued. The named exceptions: rendering-heavy UIs that
must look identical across the three OS webviews, release processes leaning on
Electron's signing/update story, and apps leaning hard on Node.

The first draft of this ADR claimed two exceptions applied. Checking the code
killed one of them:

**The agent's browser does not depend on Electron.**
`crates/optimus-browser/src/lib.rs` launches its own Chromium out-of-process
via `headless_chrome` (CDP, default port 9222). ADR-0040 makes the separation
law, not accident: `crates/optimus-kernel/src/browser_coord.rs` hard-errors if
the preview and agent-effector session ids ever coincide. Swapping the shell
changes nothing about agent automation — same Chromium, same CDP, same tests.

What *does* depend on Electron is exactly one subsystem: the preview pane.
Measured at `a905d88`, `apps/optimus-electron/main.cjs` is 931 lines:

| Lines | What | Belongs in a shell? |
|------:|------|---------------------|
| 143–246 | host process supervision, health wait | yes |
| 247–311 | `optimus-app://` protocol registration | yes |
| 312–352 | window creation | yes |
| **353–596** | **preview `WebContentsView`: bounds, state, annotation capture** | **no — 243 lines** |
| 597–624 | sender guards, bounded-integer validation | yes |
| 625–648 | host proxy | yes |
| 649–742 | hand-rolled SSE parser for chat streaming | mechanism, re-paid per shell — 94 lines |
| 743–931 | IPC wiring | yes |

Those 243 lines are in-process `WebContentsView` — an Electron-only API —
and they include `capturePreviewAnnotation` (112 lines), which encodes an
ADR-0040 security rule: annotations reach the gallery, never the composer.
They are also the least stable code in the repo: #85, #109, and the
`expect.poll` race fixed in #100 are all preview-geometry failures.

And they are a deviation, not a design. ADR-0015 §2, verbatim: *"UI chrome
stays in Optimus WebView2; browser content is mirrored/controlled via CDP."*
The accepted design renders preview content in the CDP-managed browser and
shows it in the shell as mirrored output — engine-agnostic by construction.
The in-process view is what Electron's convenience made easy, and it is the
only thing welding the product to Electron.

## Decision

**1. Electron remains the shell today.** For one reason, stated so it can
expire: the preview is in-process today, and porting 243 lines of
`WebContentsView` bounds arithmetic across WebKitGTK, WebView2, and WKWebView
— the flakiest subsystem, against three engines, while ADR-0028's migration
is still unfinished — is three half-migrations at once.

**2. The preview returns to the ADR-0015 design: out of process, via CDP.**
The already-running `optimus-browser` Chromium renders preview content;
the shell displays screencast frames and forwards input
(`Page.startScreencast` + `Input.dispatchMouseEvent`/`dispatchKeyEvent`),
the way Chrome DevTools device mode works. A spike gates this — measured
frame latency and input fidelity on local dev pages. If the spike fails the
feel test, this ADR's step 3 dies and Electron stays for that stated,
measured reason instead of a stale one.

**3. When the preview leaves the shell, the shell swap to Tauri v2 is
scheduled work, not a hypothetical.** At that point the shell is supervision,
window chrome, IPC wiring, and a pixel surface — all portable — and the 2026
default applies with no surviving exception. Ordered strictly behind #106;
two migrations do not run at once.

**4. Shells hold no product logic.** ADR-0045 moved the method registry out
of `apps/optimus-desktop` for this reason; the same boundary now covers the
preview layer. *Mechanism* stays in the shell (set bounds, ask a webview for
the clicked element, open a window). *Policy* moves to `optimus-host` (what
an annotation contains, where it may go, what a bounds request may be).
Annotation policy becomes host-side and testable without Electron.

**5. The SSE parser is recorded portability debt.** 94 lines every future
shell re-pays; a host-side client could own it once. Recorded, not paid now.

## Alternatives considered

- **Switch to Tauri v2 now.** Rejected for now — not on the agent-browser
  argument (checked, false), but because it ports the preview's in-process
  geometry to three webview engines before the preview is engine-agnostic,
  concurrently with #106. Sequenced instead of refused: see step 3.
- **Stay Electron permanently.** Rejected. The first draft's justification
  was half wrong, and the surviving half is temporary by design. A shell
  choice held for a reason that expired is how a 2027 codebase acquires a
  2019 dependency.
- **Iframe-only preview.** Rejected by ADR-0015 and unchanged: no CDP, no
  reliable localhost tooling, no annotate loop.
- **Move the preview layer wholesale into the host.** Rejected: bounds and
  frame display are engine APIs; only the policy above them moves.

## Reasons

- The shell choice is reversible; product logic trapped in a shell is not.
- ADR-0015 already made the preview engine-agnostic on paper. This ADR
  restores an accepted design rather than inventing one.
- The agent's browser was verified out-of-process before deciding — the
  decisive claim was checked against `optimus-browser`, not assumed.
- ADR-0045 set the policy/mechanism boundary for one subsystem; extending it
  is consistency, not novelty.

## Consequences

- `main.cjs` shrinks toward supervision, wiring, and a pixel surface.
- The preview-geometry flake cluster (#85, #100, #109) is confined to code
  scheduled for deletion, not code being invested in.
- Annotation policy becomes host-side — reachable by fast Rust tests instead
  of only the slowest Playwright gate.
- A Tauri swap becomes mechanical: the wins are ~157MB installer, ~135MB idle
  memory, ~1.8s cold start, and one webview engine's worth of CI.

## Risks

- **Screencast latency may feel worse than an in-process view.** This is the
  spike's whole job: measure frames-to-glass and input round-trip on local
  pages before committing. Fail → stop at step 2, keep Electron, record why.
- **Input fidelity gaps** (IME, scroll momentum, drag) may not survive CDP
  forwarding. Same gate.
- **Three migrations in flight** if sequencing is ignored. Steps are ordered:
  #106, then preview-out-of-process, then shell swap.
- **The policy/mechanism boundary will be argued at the margin.** Recorded
  here so the argument happens against a written line.

## Evaluation evidence

- `main.cjs` section measurement above, 2026-07-28, at `a905d88`.
- `optimus-browser` launch path read at the same commit: out-of-process
  `headless_chrome`, CDP port 9222, sandbox kept on.
- `browser_coord.rs` dual-domain invariant: preview/agent session ids must
  differ or construction fails.
- Electron/React surface: 117 vitest tests across 22 files plus 11 Playwright
  specs. Wry surface: 53 page-driven specs, 0 component tests (#106).
- 2026 framework numbers and exception list: current-practice search,
  2026-07-28, recorded on #106.

## Conditions for reconsideration

1. The screencast spike fails latency or fidelity on local pages → step 3 is
   cancelled and this ADR is amended with the measurements.
2. Tauri v2 gains a `WebContentsView`-class in-process view with geometry
   parity — the spike becomes unnecessary and the swap can lead.
3. The embedded preview stops being a first-class product surface.
4. Electron ships a supported out-of-process compositing surface that makes
   the shell thin without CDP — re-weigh both paths on measurements.

## Relevant code

- `apps/optimus-electron/main.cjs`
- `crates/optimus-browser/src/lib.rs`
- `crates/optimus-kernel/src/browser_coord.rs`
- `crates/optimus-host/src/router.rs`

## Relevant tests

- `apps/optimus-electron/e2e/compiled-shell.spec.cjs`
- `apps/optimus-electron/e2e/react-browser-contract.spec.cjs`
- `apps/optimus-ui/src/components/workspace/BrowserSurface.test.tsx`
