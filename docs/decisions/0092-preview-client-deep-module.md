---
doc_id: decisions-0092-preview-client-deep-module
doc_type: decision
plane: decision
status: current
authority: record
summary: The embedded-child-webview preview (preview_embed.rs mechanism plus injected annotation JS) is replaced by one deep renderer module — `createPreviewClient(transport)` — whose small interface (attach/navigate/reload/back/forward/setAnnotationMode/state/subscribe/destroy) hides the whole CDP preview pipeline: latest-wins frame coalescing, native decode+paint, input capture with coordinate scaling, degraded mode, and annotation-point capture. Mechanism stays shell-side; annotation and bounds policy move to optimus-host (ADR-0051 step 4). The wire gains an additive preview_* family and a latest-wins preview_frame event (spec-015 amendment), which is the seam: React renderer, TUI, and fixture are adapters over the same contract. Gate 1 of ADR-0051 step 2 is closed by measured evidence (shell_paint_spike): shell decode+paint p95 10-15ms worst-case loaded, inside the 16.7ms budget; the click bar is re-baselined into Chromium's flip-frame arrival and the shell budget.
reviewed_on: 2026-08-11
review_by: 2026-11-11
knowledge_type: decision
covers:
  - apps/optimus-ui/src/components/workspace/BrowserSurface.tsx
  - apps/optimus-desktop/src/preview_embed.rs
  - crates/optimus-browser/src/lib.rs
depends_on:
  - docs/decisions/0015-preview-browser-cdp.md
  - docs/decisions/0040-shared-browser-contract.md
  - docs/decisions/0045-agent-host-and-surface-transports.md
  - docs/decisions/0051-electron-now-tauri-when-the-preview-leaves-the-shell.md
  - docs/decisions/0083-one-wire-protocol-for-all-surfaces.md
  - docs/decisions/0090-renderer-client-deep-module.md
validated_by:
  - scripts/gates/check-surface-contract.py
  - crates/optimus-browser/examples/shell_paint_spike.rs
---

# ADR-0092: Preview client as a deep module over the frozen wire surface

## Status

Current.

## Context

ADR-0051 step 2 returns the preview to the ADR-0015 design: the out-of-process
`optimus-browser` Chromium renders preview content; the shell displays
`Page.startScreencast` frames and forwards input via CDP. The measured gate
(commit 78b0838, `crates/optimus-browser/examples/shell_paint_spike.rs`)
closed the feasibility half: shell decode+paint end-to-end p95 is 10.2-14.9ms
worst-case (full-frame noise JPEG, loaded machine), inside the 16.7ms 60Hz
budget, with the pure-Rust `jpeg-decoder`; native decoders are 2-4x faster.
What the spike also measured is that a synchronous CDP ack on the paint path
stalls ~235ms under load — the shell must ack on the wire thread, never after
paint — and that the click bar as written (p95 <= 100ms, first-frame
semantics) does not survive content verification: the flip frame arrives
12-32ms after dispatch, but Chromium's screencast delivery stalls ~200ms p95
even idle, so the click bar must split into Chromium's flip-frame arrival and
the shell's decode+paint budget (amendment in ADR-0051).

Today the preview is the opposite shape from the target: `PreviewEmbed`
(`apps/optimus-desktop/src/preview_embed.rs`, 679 lines) is a Wry child
WebView positioned over the renderer's `browser-hole` by IPC bounds calls,
with annotation capture injected as page JS (`ANNOTATE_INJECT_JS`) delivered
through a cancelled-navigation callback (`optimus.invalid/__annot?`) because
secondary WebKit webviews sometimes fail `window.ipc`. This is the Electron
`WebContentsView` mechanism re-paid in Wry: platform geometry hacks (GTK
restack, `lower_main_webview`), a second webview engine in the shell, and the
preview-geometry flake cluster (#85, #100, #109) all concentrate here. The
renderer's `client.browser` domain (ADR-0090) is a shallow facade over the
wire: bounds/visible/navigate/annotate each round-trip to the shell, and the
shell answers with mechanism, not policy.

The wire is frozen (spec-015, ADR-0083/0084): one protocol for every surface,
JSON-RPC 2.0 over stdio and loopback WebSocket carriers. The renderer is a
pure protocol client; the TUI is a second surface (spec-015 Phase-B); the
packaged desktop app spawns or attaches the backend it talks to. Any preview
re-architecture must respect that contract and its security model (tickets,
process secrets, bounded inputs).

## Decision

Add one deep renderer module, `apps/optimus-ui/src/preview/**`, whose single
consumer-facing object is `createPreviewClient(transport)`:

```ts
interface PreviewClient {
  attach(canvas: HTMLCanvasElement): Promise<void>;
  navigate(url: string): Promise<PreviewState>;
  reload(): Promise<PreviewState>;
  back(): Promise<PreviewState>;
  forward(): Promise<PreviewState>;
  setAnnotationMode(on: boolean): Promise<void>;
  state(): PreviewState;
  subscribe(listener: (state: PreviewState) => void): () => void;
  destroy(): void;
}
```

`PreviewState` carries `url`, `title`, `canGoBack`, `canGoForward`,
`loading`, `mode: 'live' | 'degraded' | 'fixture'`, and `error`. That is the
entire interface a caller must learn. Everything else — the frame pipeline,
the input pipeline, connection lifecycle, and degraded fallback — lives
behind it.

**The implementation is deep, and deliberately so.** Behind that interface:

- **Latest-wins frame pipeline.** The host produces CDP screencast frames
  and broadcasts them as `preview_frame` events; the client keeps only the
  newest (a preview is lossy by construction — stale frames are dropped, not
  queued). Decode uses `createImageBitmap` on the JPEG blob (native, the
  measured 2-10ms class), paint is `drawImage` to the attached canvas, and
  the paint schedule rides the existing `frameCoordinator` so frames and
  input coalesce on the same rAF clock. The renderer never acks frames: the
  host owns the CDP ack on its wire-side thread — the shape the spike proved
  mandatory (sync ack after paint stalls ~235ms under load).
- **Input forwarding with scaling.** The client captures pointer, wheel,
  key, and IME events on the canvas, coalesces them per rAF, scales CSS
  pixels to the screencast viewport (measured 1280x657 device px, not the
  requested window), and forwards typed `preview_input` events to the host,
  which dispatches `Input.dispatchMouseEvent` / `dispatchKeyEvent` /
  `imeSetComposition` / `insertText`. Pointer-capture semantics for drag,
  wheel deltas for momentum, and IME composition are the browser's own once
  the events arrive; the client's job is fidelity of forwarding (coalescing
  must never merge drags mid-button or drop IME state).
- **Annotation is a point, not a script.** The client captures the click
  point on the canvas; the host resolves the element at that point through
  CDP and applies host-side policy (gallery-only routing, ADR-0040, the
  "112-line rule" retirement from ADR-0051 step 4). The injected
  `ANNOTATE_INJECT_JS` + `optimus.invalid/__annot?` fallback and the
  `annotation_from_nav_url` parser are retired with the embedded webview.
- **Degraded mode is the existing fixture.** If the preview session cannot
  open (CDP unavailable), `mode` is `'degraded'` and the client renders the
  existing `FixturePage` — the honest "never fake capability" path of
  ADR-0015 section 4 — with a banner. The fixture is an adapter, not a lie.
- **Lifecycle is one session per surface.** `attach` opens
  `preview_open` (host-side `PreviewSession` in the UserPreview trust
  domain, ADR-0040 — never the agent effector's session, whose SSRF guard
  deliberately forbids localhost); `destroy` closes it. Reopening is a
  fresh session.

**Host side: policy and plumbing, not mechanism.** `optimus-host` gains a
`preview.rs` module owning: session lifecycle (`preview_open`/`preview_close`
via `optimus-browser::PreviewSession`, a `CdpBrowserSession` peer whose
network authority allows localhost and dev pages per ADR-0040/0045),
navigation policy (the same `validate_network_url` discipline in the preview
authority), bounds policy (validate and clamp bounds requests), annotation
policy (element-at-point plus destination rules), frame broadcast
(latest-wins, dropped when no surface is attached), and input validation
(bounded integers, coordinate clamping, rate limits — the discipline the old
shell kept in `main.cjs`). Policy functions are pure and reachable by fast
Rust tests.

**The wire is the seam, and it is real.** The spec-015 amendment is additive
only: `preview_open`, `preview_close`, `preview_navigate`, `preview_reload`,
`preview_back`, `preview_forward`, `preview_set_annotation`, `preview_bounds`,
`preview_input` methods plus one event, `preview_frame` (latest-wins, no
ack), with the frozen-wire pins updated (`contracts.schema.test`,
`check-surface-contract.py`). The seam has more than one adapter: the React
renderer (live now), the TUI (spec-015 Phase-B), and the fixture (degraded).
Per seam discipline, nothing preview-shaped is added to `PreviewEmbed`: the
child-webview path is kept only as the named rollback until parity, then
deleted.

**What this ADR does not do:** it does not open the CDP session from the
renderer (the renderer holds no browser secrets — ADR-0084), it does not
stream raw CDP to the wire (the host is the only CDP client), and it does
not change the agent effector's browser session (ADR-0040 dual-domain
invariant stands: preview and effector session ids must differ).

## Alternatives considered

- **Paint in the shell native layer (wry/tauri window compositor).**
  Rejected: a native pixel surface over the webview is a new per-platform
  compositor path (GTK/Windows/macOS), the exact class of code that made
  `preview_embed.rs` flaky, and it forfeits the renderer's existing canvas
  and `frameCoordinator`. The renderer canvas is the pixel surface; the
  "shell" that keeps mechanism is the webview it lives in.
- **Renderer connects to CDP directly (localhost:9222).** Rejected: breaks
  the ticket/secret model (ADR-0084) and the SSRF boundary (ADR-0040); the
  renderer would hold browser credentials and bypass host policy entirely.
  The host is the only CDP client.
- **Extend the existing `client.browser` facade with frame events.**
  Rejected: that facade is the transport's shape (one method per wire call,
  call-site knowledge of envelopes), the shallow-module shape ADR-0090
  diagnosed; the preview pipeline is deep behavior (coalescing, scaling,
  decode/paint, degraded fallback) that belongs in one module with a small
  interface, not spread across `BrowserSurface` call sites.
- **Keep the child webview and add CDP only for annotation.** Rejected:
  keeps the second webview engine, the geometry hacks, and the injected-JS
  annotation path; step 2's purpose is retiring exactly that code, and the
  measured gate says the pixel path is viable.
- **Reliable (acked) frame stream.** Rejected: a preview is lossy by
  design; acked delivery adds backpressure and replay machinery for pixels
  that are stale the instant the next frame arrives. Latest-wins with a
  bounded host-side drop is the right contract.

## Reasons

- Depth buys leverage and locality exactly where the complexity is: one
  renderer module concentrates decode/paint/input/lifecycle so no caller
  re-learns CDP, and the geometry flake cluster (#85, #100, #109) is
  confined to code scheduled for deletion.
- The seam placement follows the ADR-0083 law: one wire protocol for every
  surface. The renderer client, TUI, and fixture cross the same seam, which
  is what makes the Tauri swap (ADR-0051 step 3) mechanical and the
  browser-testable HTTP mode (`:8787`) a parity proof for free.
- The spike's measurements are the design's spine: wire-side acks (235ms
  stall measured on the sync path), latest-wins (60Hz delivery with ~200ms
  p95 stalls means the client must never queue), and the 60Hz budget
  (10-15ms p95 worst case leaves the browser's native decoder headroom).
- Policy in the host, mechanism in the module is ADR-0045/0051 step 4
  applied consistently: the annotation rule (gallery-only) becomes a Rust
  test instead of a Playwright-only path, and bounds policy stops being the
  shell's judgment call.

## Consequences

- `preview_embed.rs` shrinks to nothing when parity lands; until then it is
  the named rollback path and receives no new features (the flake cluster is
  quarantined to it).
- The wire grows one event family; spec-015 pins, the schema test, and the
  surface-contract gate move with it. This is the documented cost of a
  frozen wire: changes are additive and pinned, never silent.
- The renderer carries a second deep module beside the ADR-0090 client;
  both consume the transport, neither knows the other's internals.
  `BrowserSurface.tsx` becomes a consumer of `PreviewClient` and loses its
  bounds-sync and annotation plumbing.
- The TUI gains a preview surface for free once it speaks the same wire
  (Phase-B), without a second browser integration.
- The injected annotation JS and its callback-URL fallback die with the
  embedded webview; annotation policy lives in `optimus-host` and is testable
  without any webview.

## Risks

- **Input fidelity through CDP** (wheel momentum, drag, IME) remains the
  unmeasured half of gate 1 — the spike measured pixels and clicks, not
  gestures. Mitigation: the client's forwarding is the only new surface;
  wheel/drag/IME probes are the next spike (prototype skill, same shape as
  `shell_paint_spike`), and the module's input path is behind the same
  interface its tests drive with a fake transport.
- **`createImageBitmap` decode in the renderer may differ from the spike's
  native decode** (browser engine, memory pressure). The spike used the
  pure-Rust `jpeg-decoder` as a *slower* proxy; if real renderer decode
  still fits the 16.7ms budget it is a stronger result, and the gate is
  re-measured in the first shell integration (ADR-0051 condition 1).
- **Wire throughput at worst-case frame sizes** (noise JPEG ~300KB at
  60fps = ~18MB/s over loopback). The host drops frames when no surface is
  attached and the client never queues; measured production cadence under
  load is ~8fps p50, and localhost IPC handles the idle worst case. If a
  future surface needs less, quality and cadence are host-side knobs.
- **The preview session is a second Chromium instance.** It reuses the
  `optimus-browser` launch path (same sandbox, same user-data discipline)
  but doubles browser processes while the embedded path also lives.
  Accepted for the parity window; the embedded path's retirement closes it.

## Evaluation evidence

- **Shell decode+paint gate, 2026-08-11** (`shell_paint_spike.rs`, commit
  78b0838, 3 runs, 16 cores, ~45% burned for loaded cells): worst-case
  decode+paint e2e p95 10.2-14.9ms (noise page, loaded) — PASS against the
  16.7ms 60Hz budget; simple-page e2e p50 ~1.9ms idle / ~2.5-3.2ms loaded;
  click flip-frame arrival 12-32ms after dispatch; Chromium delivery stalls
  ~200ms p95 even idle (attributed to Chromium, not the shell); loaded
  cadence p50 ~115ms is Chromium's capture rate under load. Full tables on
  issue #113.
- **Ack discipline, 2026-08-11**: synchronous `ack_screencast` on the
  consumer thread stalls ~235ms under load and throttles production; the
  wire-side ack thread restores 2ms-class e2e. The client design above
  encodes this as law (renderer never acks; host acks on its wire thread).
- The spike's own delivery numbers (2026-07-29, ADR-0051) remain the
  transport baseline: 60Hz cadence, 4ms staleness, click p95 29.6ms
  first-frame.

## Conditions for reconsideration

1. A fidelity probe (wheel momentum, drag, IME through `preview_input`)
   contradicts the forwarding design — the module's input path is amended
   with the measurement before any shell integration depends on it.
2. Real-renderer `createImageBitmap` decode misses the 16.7ms p95 budget in
   the first shell integration — the frame pipeline falls back to a
   native decode worker (host-side) or a quality/cadence reduction, and this
   ADR is amended.
3. The embedded webview gains first-class engine parity in Tauri v2
   (ADR-0051 condition 2) — the pixel path becomes optional, not default,
   but the module interface survives as the thin-shell contract either way.
4. The wire cannot carry `preview_frame` within the frozen-contract
   discipline — the preview is re-scoped to the desktop binary's own
   carrier and this ADR's seam moves with it.

## Relevant code

- `apps/optimus-ui/src/preview/**` (new module — this ADR's subject)
- `apps/optimus-ui/src/components/workspace/BrowserSurface.tsx` (becomes a consumer)
- `apps/optimus-ui/src/ipc/client/**` (ADR-0090 client, sibling consumer of the transport)
- `apps/optimus-desktop/src/preview_embed.rs` (retired mechanism, rollback path)
- `crates/optimus-host/src/preview.rs` (new policy/plumbing module)
- `crates/optimus-browser/src/lib.rs` (add `PreviewSession`, authority mirror of `BrowserNetworkAuthority::public_with_owned_localhost`)
- `crates/optimus-browser/examples/shell_paint_spike.rs` (gate evidence, kept in tree)
- `crates/optimus-browser/examples/screencast_spike.rs` (transport baseline)
- `crates/optimus-kernel/src/browser_coord.rs` (dual-domain invariant, unchanged)

## Relevant tests

- `apps/optimus-ui/src/preview/previewClient.test.ts` (fake transport: coalescing, latest-wins, scaling, degraded mode, lifecycle)
- `apps/optimus-ui/src/components/workspace/BrowserSurface.test.tsx` (consumer-level, updated)
- `crates/optimus-host/src/preview.rs` unit tests (annotation policy, bounds policy, input validation — fast Rust, no webview)
- `scripts/tests/test_surface_contract.py` + `check-surface-contract.py` (wire pins for the preview_* family)
- `crates/optimus-browser/examples/shell_paint_spike.rs` (re-runnable gate evidence)
