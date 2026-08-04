---
doc_id: decisions-0051-electron-now-tauri-when-the-preview-leaves-the-shell
doc_type: decision
plane: decision
status: current
authority: record
summary: - Date: 2026-07-29 - Program: program P30+ (TUI + core foundation)
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: decision
covers:
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
  - scripts/gates/check-desktop-ipc-matrix.py
  - scripts/gates/check-module-size.py
---

# ADR-0051: Tauri primary, Electron rollback during preview parity

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

**1. Tauri v2 is now the primary desktop shell.** The first operational slice
uses the existing Rust host directly through Tauri commands, keeps the React
workbench unchanged, and installs the Tauri binary as the default desktop
entry. Electron remains available as a named rollback shell until
browser-preview parity is complete.

The Tauri shell owns the window, invokes the transport-neutral
`optimus-host` registry, streams chat events through a typed Tauri `Channel`,
and keeps cancellation in Rust. It does not spawn Node or an intermediate HTTP
host for normal IPC.

**2. The preview returns to the ADR-0015 design: out of process, via CDP.**
The already-running `optimus-browser` Chromium renders preview content;
the shell displays screencast frames and forwards input
(`Page.startScreencast` + `Input.dispatchMouseEvent`/`dispatchKeyEvent`),
the way Chrome DevTools device mode works. The latency gate for this was
measured before this ADR was proposed and **passed with 3–6× margin** — see
Evaluation evidence and `crates/optimus-browser/examples/screencast_spike.rs`,
which stays in the tree so the numbers can be re-taken on other hardware.
The remaining gate for production is fidelity, not latency: scroll momentum,
drag, and IME through CDP input forwarding.

**3. Browser-preview parity is the remaining migration slice.** The primary
Tauri workbench currently leaves the Electron-only embedded preview surface on
the rollback path. The next slice moves preview rendering to the accepted
out-of-process CDP/screencast design, then adds the equivalent Tauri browser
surface and input forwarding before Electron is removed.

**4. Shells hold no product logic.** ADR-0045 moved the method registry out
of `apps/optimus-desktop` for this reason; the same boundary now covers the
preview layer. *Mechanism* stays in the shell (set bounds, ask a webview for
the clicked element, open a window). *Policy* moves to `optimus-host` (what
an annotation contains, where it may go, what a bounds request may be).
Annotation policy becomes host-side and testable without Electron.

**5. The SSE parser is recorded portability debt.** 94 lines every future
shell re-pays; a host-side client could own it once. Recorded, not paid now.

## Alternatives considered

- **Switch to Tauri v2 after preview parity.** Rejected as the sequencing
  choice after the user explicitly requested Tauri now. Electron remains the
  reversible fallback while the preview contract is completed.
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

- **Measured latency is delivery, not glass.** The spike measures frame
  arrival in the receiving process; a real shell adds JPEG decode and a
  compositor paint (~one frame). Even charging a full extra 17ms, click→pixel
  p95 lands near 47ms against the 100ms bar. Re-measure end-to-end in the
  first shell integration, and on a loaded machine — the spike ran idle.
- **Input fidelity gaps** (IME, scroll momentum, drag) may not survive CDP
  forwarding. Unmeasured; this is the remaining gate for step 2, checked
  during implementation, not assumed.
- **Two shell paths during preview parity.** The installer records Tauri as
  primary and keeps Electron explicitly selectable as rollback; this is
  removed only after the Tauri preview gate passes.
- **The policy/mechanism boundary will be argued at the margin.** Recorded
  here so the argument happens against a written line.

## Evaluation evidence

- **Screencast spike, 2026-07-29**, headless Chromium via the pinned
  `headless_chrome` 1.0.22, JPEG q80 at 1280×800, idle desktop
  (`cargo run -p optimus-browser --example screencast_spike --release`):

  | Metric | n | p50 | p95 | max | Bar |
  |---|---:|---:|---:|---:|---|
  | frame cadence, animating | 179 | 16.7ms | 17.3ms | 31.5ms | p50 ≤ 50ms |
  | capture→delivery staleness | 180 | 4.1ms | 4.3ms | 5.0ms | — |
  | click→pixel round trip | 20 | 24.4ms | 29.6ms | 29.7ms | p95 ≤ 100ms |

  Verdict: **PASS**. Cadence is the compositor's own 60Hz; input feel is
  under two frames end to end.
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

1. End-to-end shell integration or the fidelity checks (scroll, drag, IME)
   contradict the spike's latency verdict → step 3 is cancelled and this ADR
   is amended with the measurements.
2. Tauri v2 gains a `WebContentsView`-class in-process view with geometry
   parity — the spike becomes unnecessary and the swap can lead.
3. The embedded preview stops being a first-class product surface.
4. Electron ships a supported out-of-process compositing surface that makes
   the shell thin without CDP — re-weigh both paths on measurements.

## Relevant code

- `apps/optimus-electron/main.cjs`
- `apps/optimus-tauri/src/main.rs`
- `apps/optimus-tauri/tauri.conf.json`
- `crates/optimus-browser/src/lib.rs`
- `crates/optimus-browser/examples/screencast_spike.rs`
- `crates/optimus-kernel/src/browser_coord.rs`
- `crates/optimus-host/src/router.rs`

## Relevant tests

- `apps/optimus-electron/e2e/compiled-shell.spec.cjs`
- `apps/optimus-electron/e2e/react-browser-contract.spec.cjs`
- `apps/optimus-ui/src/components/workspace/BrowserSurface.test.tsx`
