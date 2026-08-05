---
version: alpha
name: Optimus Vantage
description: A compact artifact-centered agent workbench for supervising parallel software work with calm, high-refresh motion.
colors:
  primary: "#7C8CFF"
  canvas: "#080B10"
  rail: "#0B0F16"
  surface: "#0E131B"
  elevated: "#141B26"
  interactive: "#192231"
  interactiveHover: "#202C3D"
  border: "#263244"
  borderStrong: "#35445A"
  text: "#F2F5F8"
  textSecondary: "#A9B3C2"
  textMuted: "#7F8A9B"
  accent: "#7C8CFF"
  accentStrong: "#9AA6FF"
  accentWash: "#1B2240"
  cyan: "#63D4FF"
  success: "#52D6A4"
  warning: "#F3C778"
  danger: "#FF7B86"
  focus: "#A8B3FF"
  lightCanvas: "#F4F6F9"
  lightSurface: "#FFFFFF"
  lightText: "#17202D"
  lightTextSecondary: "#526074"
  lightBorder: "#CFD6E2"
typography:
  display:
    fontFamily: Inter, Geist, SF Pro Display, Segoe UI, sans-serif
    fontSize: 1.25rem
    fontWeight: 620
    lineHeight: 1.2
    letterSpacing: "-0.025em"
  title:
    fontFamily: Inter, Geist, SF Pro Text, Segoe UI, sans-serif
    fontSize: 0.875rem
    fontWeight: 600
    lineHeight: 1.3
    letterSpacing: "-0.01em"
  body:
    fontFamily: Inter, Geist, SF Pro Text, Segoe UI, sans-serif
    fontSize: 0.875rem
    fontWeight: 430
    lineHeight: 1.55
    letterSpacing: "-0.004em"
  bodyCompact:
    fontFamily: Inter, Geist, SF Pro Text, Segoe UI, sans-serif
    fontSize: 0.78125rem
    fontWeight: 450
    lineHeight: 1.35
    letterSpacing: "0em"
  label:
    fontFamily: Inter, Geist, SF Pro Text, Segoe UI, sans-serif
    fontSize: 0.6875rem
    fontWeight: 580
    lineHeight: 1.2
    letterSpacing: "0.035em"
  mono:
    fontFamily: Geist Mono, Berkeley Mono, SFMono-Regular, Cascadia Code, monospace
    fontSize: 0.75rem
    fontWeight: 430
    lineHeight: 1.5
    letterSpacing: "-0.01em"
rounded:
  xs: 4px
  sm: 6px
  md: 8px
  lg: 10px
  xl: 12px
  panel: 14px
spacing:
  hairline: 1px
  xxs: 2px
  xs: 4px
  sm: 6px
  md: 8px
  lg: 10px
  xl: 12px
  xxl: 16px
  section: 20px
  canvas: 24px
  major: 32px
components:
  button-icon:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.textSecondary}"
    rounded: "{rounded.md}"
    size: 30px
    padding: 6px
  button-icon-hover:
    backgroundColor: "{colors.interactiveHover}"
    textColor: "{colors.text}"
    rounded: "{rounded.md}"
    size: 30px
    padding: 6px
  button-compact:
    backgroundColor: "{colors.interactive}"
    textColor: "{colors.textSecondary}"
    typography: "{typography.bodyCompact}"
    rounded: "{rounded.md}"
    height: 30px
    padding: 8px
  button-primary:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.canvas}"
    typography: "{typography.bodyCompact}"
    rounded: "{rounded.md}"
    height: 32px
    padding: 10px
  button-danger:
    backgroundColor: "{colors.danger}"
    textColor: "{colors.canvas}"
    typography: "{typography.bodyCompact}"
    rounded: "{rounded.md}"
    height: 32px
    padding: 10px
  row-compact:
    backgroundColor: "{colors.rail}"
    textColor: "{colors.textSecondary}"
    typography: "{typography.bodyCompact}"
    rounded: "{rounded.sm}"
    height: 28px
    padding: 8px
  row-selected:
    backgroundColor: "{colors.accentWash}"
    textColor: "{colors.text}"
    typography: "{typography.bodyCompact}"
    rounded: "{rounded.sm}"
    height: 28px
    padding: 8px
  input-compact:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text}"
    typography: "{typography.bodyCompact}"
    rounded: "{rounded.md}"
    height: 30px
    padding: 8px
  composer:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text}"
    typography: "{typography.body}"
    rounded: "{rounded.xl}"
    padding: 12px
  terminal:
    backgroundColor: "{colors.canvas}"
    textColor: "{colors.textSecondary}"
    typography: "{typography.mono}"
    rounded: "{rounded.xs}"
    padding: 10px
---

## Overview

**Optimus Vantage** is the July 2027 design forecast for coding-agent applications: an **artifact-centered agent workbench** with compact supervision lanes. The product is neither a chat app with developer tools attached nor a traditional IDE with an AI sidebar. Its primary object is the current unit of work—plan, diff, file, preview, run, approval, or result—while conversation steers that object and evidence remains one gesture away.

### Implementation status — 2026-07-23

- **Confirmed current behaviour:** the React renderer implements the Vantage
  workbench and is the repository-level default Tauri surface. Rust still
  owns sessions, settings, jobs, approvals, tools, artifacts, files,
  cancellation, and terminal outcomes.
- **Confirmed current behaviour:** production React assets load through
  `optimus-app://ui/`; the renderer uses a context-isolated preload and does not
  receive the Rust bearer token.
- **Confirmed current behaviour:** React owns Browser chrome and geometry while
  the Tauri webview hosts the sandboxed preview surface. The user preview and Rust
  agent Browser remain separate capability paths.
- **Confirmed current behaviour:** the empty work surface uses compact,
  outcome-oriented starter rows rather than a generic card grid. Capabilities
  is a flat Rust-owned tool inventory with a separate unavailable boundary, and
  the 320 px composer keeps provider, model, effort, access, and fast mode
  visible without horizontal clipping.
- **Planned behaviour:** installed-app cutover and physical high-refresh
  certification are not part of the repository implementation proof.

This is a forecast, not a claim of settled consensus. It is based on product trajectories visible on **2026-07-23**:

- Cursor describes an interface “centered around agents rather than files,” then a higher level of abstraction with drill-down, multi-repo layout, and local/cloud-agent handoff.
- OpenAI's Codex app supports parallel agents through worktrees, in-thread diff review, comments, and editor handoff.
- GitHub's Copilot app exposes multiple isolated agent sessions simultaneously, each with its own branch, mode, model, and tools; GitHub also provides an agents panel for cross-repository monitoring.
- Google's Jules models work as sources, sessions, and activity, reinforcing asynchronous job-shaped agent work.
- Linear describes AI interaction as a purpose-built workbench and treats the agent session as a first-class interaction abstraction.
- Human-in-the-loop research such as Magentic-UI emphasizes observable plans, actions, and intervention rather than opaque automation.

### Forecast

By July 2027, the most copied coding-agent UI topology will likely be:

1. **Scope rail** on the left: projects, tasks, sessions, and agent occupancy.
2. **Work canvas** in the center: the selected artifact and its review state, with chat as command/history rather than the only surface.
3. **Evidence inspector** on the right: files, diff, preview/browser, provenance, and approvals.
4. **Execution dock** at the bottom: terminal, tests, logs, and long-running process output.
5. **Control strip** at the top: current scope, active-agent count, model/access state, and pane toggles.

The forecast is falsified if leading tools converge back to a single linear chat, eliminate parallel-session monitoring, or make produced artifacts less directly reviewable. The design should therefore preserve Optimus's existing panes and contracts while making the central work object, state, and provenance more legible.

### Product character

- **Dense, not cramped:** information occupies space only when it changes a decision.
- **Calm, not inert:** state transitions explain continuity without spectacle.
- **Technical, not terminal cosplay:** monospace is reserved for code, paths, IDs, timings, and output.
- **Agentic, not anthropomorphic:** show ownership, status, evidence, and scope; avoid decorative robot metaphors.
- **Trustworthy, not magical:** running, waiting, approval, failed, cancelled, and complete are visibly different.
- **Drill-down, not dashboard sprawl:** summary first; detail in the inspector or execution dock.

## Colors

Vantage uses a near-black blue canvas, opaque structural surfaces, restrained borders, and one cool periwinkle accent. Cyan is reserved for inspectable links and live navigation; semantic colors communicate execution state.

- **Canvas `#080B10`:** the continuous desktop field.
- **Rail `#0B0F16`:** scope and status lanes; barely separated from the canvas.
- **Surface `#0E131B`:** composer, controls, and embedded panels.
- **Elevated `#141B26`:** menus and temporary popovers only.
- **Interactive `#192231`:** selected/hoverable technical rows.
- **Accent `#7C8CFF`:** focus, active selection, send, and progress—not decoration.
- **Cyan `#63D4FF`:** links, Browser target, and inspected references.
- **Success/Warning/Danger:** completed, waiting/approval, and failed/destructive.

Do not use large gradients, aurora wallpaper, tinted glass across the whole shell, or saturated status confetti. A subtle one-pixel highlight may separate an elevated surface. Light mode maps the same luminance roles to `lightCanvas`, `lightSurface`, `lightText`, `lightTextSecondary`, and `lightBorder`; it is not a separate visual concept.

Minimum text/background contrast is WCAG 2.2 AA. Body text targets 7:1 where practical. Muted text is never the sole carrier of state. Focus uses both a visible ring and semantic selection.

## Typography

Inter/Geist-style grotesks carry interface and prose; a coding mono is reserved for technical evidence. The system uses optical density rather than tiny text.

- Main transcript: 14 px equivalent, 1.55 line-height, maximum readable measure 78 characters.
- Compact rows and controls: 12.5 px equivalent, 28–30 px tall.
- Section labels: 11 px equivalent, sentence case by default; uppercase only for terse machine states.
- Code/output: 12 px equivalent with tabular numerals.
- Headings: one restrained 20 px display level and compact 14 px titles. No oversized hero copy inside a desktop tool.
- Weight hierarchy: 430 body, 580 labels, 600–620 titles. Avoid fields of bold text.

Streamed text is never scaled, blurred, translated, or individually animated. Stable glyph metrics are a motion requirement.

## Layout

### Canonical desktop topology

```text
┌────────────────────────── 36 px control strip ─────────────────────────────┐
│ scope  session / task                         agents  inspect  term  window │
├─ 232 px scope rail ─┬──────────── flexible work canvas ───────┬─ workspace ┤
│ nav                 │ selected work object / transcript       │ files/diff │
│ projects + sessions │ compact evidence timeline               │ browser    │
│ agent occupancy     │ anchored command composer               │ provenance │
├─────────────────────┴──────────────────────────────────────────┴────────────┤
│ execution dock: terminal · tests · logs · processes                 0–42% │
├────────────────────────── 22 px truth strip ───────────────────────────────┤
└────────────────────────────────────────────────────────────────────────────┘
```

### Space ownership

- **Control strip:** 36 px high. Window controls retain platform-safe width. Product actions use 30 px icon buttons or icon + short label where ambiguity matters.
- **Scope rail:** 232 px default; 196–360 px resizable; collapses to a 52 px icon rail. Rows are 28 px with 4–6 px internal rhythm. Project/session hierarchy is denser than navigation.
- **Work canvas:** owns all unallocated width and retains a 520 px minimum in the wide three-surface mode. It never becomes a decorative empty void.
- **Workspace:** Browser, Files, and Artifacts are co-equal tabs. It defaults near 48% of usable width and has a 360 px minimum.
- **Execution dock:** 184 px default; 120 px minimum; at most 42% of available app height. It can be collapsed to its 30 px header without losing running-state visibility.
- **Truth strip:** 22 px. Show only facts useful across the whole app: connection, active agents, current model/access, branch/scope, and version warnings.

### Compactness rules

- Pointer targets are at least 24 × 24 CSS px; frequent desktop controls are normally 28–32 px. Icon geometry is 14–16 px inside the target.
- Primary actions may be 32 px tall; destructive confirmation actions may be 34 px. Generic 40–48 px controls are not used in the desktop shell.
- Horizontal padding: 6 px for icon controls, 8 px for compact rows, 10 px for labelled controls, 12 px for composer/panels.
- Adjacent related controls use 2–4 px gaps; unrelated groups use a one-pixel divider or 8–12 px gap.
- A panel gets one boundary. Do not nest cards merely to create depth.
- Prefer icon-only controls only when the symbol is conventional and a tooltip plus accessible label is present. Use icon + terse text for Files, Diff, Browser, Terminal, Approve, and Run when state or consequence may be ambiguous.

### Responsive behavior

- **≥1280 px:** full scope rail, work canvas, inspector; execution dock optional.
- **960–1279 px:** scope rail 208 px; workspace near 40% of usable width; hide low-priority control labels but retain tooltips.
- **720–959 px:** scope rail becomes a 52 px command rail; workspace overlays or replaces the canvas only when explicitly selected.
- **<720 px:** one primary surface at a time; titlebar exposes back/surface switcher. Do not squeeze three panes into unreadable columns.
- User-resized widths win over responsive defaults while they remain valid. Clamp persisted values when the window shrinks.

## Elevation & Depth

Depth communicates temporary ownership:

1. Canvas and structural rails: no shadow.
2. Selected rows and controls: background/border change only.
3. Composer and execution header: one hairline plus a short ambient shadow.
4. Menus, command palette, task popover: opaque elevated surface with a 12–18 px soft shadow.
5. Modal approval: scrim plus one elevated surface; only for genuinely blocking decisions.

Avoid permanent backdrop blur. It increases paint cost and reduces text contrast. Native Browser content must never be covered by fake CSS glass or rely on CSS z-index over a child window.

## Shapes

The shell is rectilinear with modest 6–12 px radii. Shape indicates interaction class:

- 4–6 px: code blocks, status marks, dense rows.
- 8 px: compact buttons, inputs, tabs, menus.
- 10–12 px: composer, popovers, task panels.
- 14 px: rare floating panel on a large canvas.
- Full pills: only count/status capsules with one short value; never paragraphs or full navigation rows.

No arbitrary mixed corner motifs. No giant rounded cards. Resize rails remain straight 7 px hit regions with a 1 px visible seam and a 2–3 px center affordance.

## Components

### Control strip

- Left rail toggle: 30 px icon target, conventional sidebar icon, tooltip, `aria-pressed`.
- Session title: breadcrumb-like text, truncates from the left only for long paths and from the right for titles.
- Tasks control: icon + count; status dot reflects running/waiting/failed.
- Inspector and Terminal: icon + short label at wide widths; icon only below 960 px.
- Theme and window controls remain separate from product state.

### Scope rail

- Navigation uses 30 px rows and 16 px icons.
- Project/session tree uses 28 px rows, one-line labels, 12 px indent steps, and visible active/running/dirty marks.
- Search is 30 px and supports command hint text without a decorative keycap at narrow widths.
- Pinned and workspace regions share one resizable splitter. Empty pinned state is one muted line, not a card.
- Collapsed 48 px mode retains New, Search, project switcher, Settings, and running-agent markers.

### Work canvas and transcript

- User prompts are compact right-aligned surfaces; assistant output sits directly on the canvas.
- Tool activity is a **foldable evidence timeline**, not a stack of chat bubbles. One line shows tool, state, elapsed time, and disclosure.
- Plans, diffs, code, and artifacts use dedicated visual treatments and can open in the inspector without changing the conversation's scroll position.
- Final metadata is summarized in one low-contrast line; expanded provenance belongs in the inspector.
- New-message entry uses 4 px of vertical translation and opacity once. Existing messages never replay entry animation.

### Composer

- Anchored to the canvas with a maximum width matching the transcript.
- Text area begins at 44 px and grows to 176 px.
- Provider/model/thinking/access form a single 26–28 px metadata row. Labels disappear before values; values truncate; controls never overlap.
- Send/Stop is a 32 px high-emphasis icon button. Stop uses danger color and a square icon; the target does not move between states.
- Attachments/browser annotations appear as removable context chips above the text line, not raw technical dumps.
- Running tasks never float over the composer. The task popover anchors to the control strip; active execution can also appear as a one-line activity rail above the composer.

### Evidence inspector

- Tabs: Files, Artifacts, Browser; future Diff/Changes fits the same contract.
- Tab strip is 34 px. Active state uses a quiet surface and one accent marker.
- File rows are 28 px. Preview is split from the tree by a draggable divider.
- Browser controls are 30 px; the omnibox is one continuous 30 px field.
- The persistent native Browser child stays physically outside resize hit regions and is hidden before an incompatible surface transition.

### Execution dock

- Header is 30 px and contains Terminal, Tests, Logs, process count, maximize, and close/collapse.
- Output remains mono, selectable, and stable. Running output appends without re-rendering prior lines.
- Input is 30 px with a persistent prompt affordance.
- Approval-required commands link to the approval surface; no fake success output.

### Empty, loading, and error states

- Empty work canvas shows a compact command headline, three context-aware starter rows, and recent/running work when available. It does not center two lines inside an otherwise empty 1,000 px field.
- Loading preserves geometry with skeleton lines or an inline progress mark; it does not blank the pane.
- Errors appear at the nearest responsible boundary with Retry/Inspect where meaningful. Authentication errors remain visible but do not monopolize the scope rail.

### Motion system: display-aware, not frame-rate theater

“240 fps” is an acceptance target on a capable 240 Hz display, not a guaranteed output rate. `requestAnimationFrame` follows the compositor/display cadence; GPU, WebKitGTK/Wry, operating system, and monitor remain external constraints. Every animation is time-based using the rAF timestamp so it has the same duration at 60, 120, 144, and 240 Hz.

At 240 Hz the theoretical frame interval is **4.17 ms**. During a UI transition, Optimus budgets:

- ≤1.0 ms application JavaScript per frame.
- ≤1 layout read and one batched write phase per frame.
- ≤2.0 ms combined style/layout/paint for shell motion on reference hardware.
- Remaining time for WebKit, native Browser placement, compositor, and scheduling variance.

No transition may allocate unbounded DOM, parse full transcript history, or issue duplicate native geometry.

| Object | Trigger | Duration | Curve | Animated properties | Contract |
|---|---:|---:|---|---|---|
| Icon/compact button | hover | 70 ms | linear-out | color, background, border | No movement; tooltip after 420 ms. |
| Button | press/release | 60/90 ms | `cubic-bezier(.2,0,0,1)` | transform to 0.985, color | Origin stays centered; disabled has no transform. |
| Nav/row selection | state change | 90 ms | ease-out | background, color, accent opacity | No sliding highlight that can lag pointer input. |
| Tooltip | reveal/hide | 110/70 ms | ease-out | opacity, translateY 2 px | Never blocks target or cursor. |
| Menu/popover | open/close | 120/90 ms | `cubic-bezier(.16,1,.3,1)` | opacity, translateY 4 px, scale .99 | Transform/opacity only; focus moves after mount. |
| Route surface | change | 110 ms | ease-out | opacity, translateX 4 px | Old surface exits before `hidden`; scroll positions persist. |
| Left rail | expanded ↔ icon rail | 150 ms | `cubic-bezier(.2,0,0,1)` | grid column width; inner label opacity | No spring/overshoot; labels fade before the width contracts. |
| Right inspector | open/close | 160/130 ms | `cubic-bezier(.2,0,0,1)` | grid column width, opacity, translateX 6 px | Browser placement uses the existing one-in-flight/latest-pending scheduler each changed frame. |
| Execution dock | open/close | 150/120 ms | `cubic-bezier(.2,0,0,1)` | flex-basis/height, opacity, translateY 4 px | One layout owner; no scroll jump; Browser bounds converge concurrently. |
| Pane tab | switch | 100 ms | ease-out | opacity, translateX 3 px | Native Browser reveal remains lifecycle-aware. |
| New message | first mount only | 120 ms | ease-out | opacity, translateY 4 px | Never replays on stream or history render. |
| Tool status | running → terminal | 140 ms | ease-out | color, dot opacity | No pulsing ring; running uses a low-amplitude luminance cycle. |
| Stream cursor | active | 900 ms | steps(1) | opacity | Cursor only; text is motionless. |
| Resize rail | pointer hover | 70 ms | linear-out | color, affordance opacity | Hit width does not change. |
| Sidebar/terminal resize | drag | 0 ms | direct | geometry | Same input-turn write, rAF convergence, no easing/inertia. |
| Theme | toggle | 120 ms | linear-out | color/background/border | No full-screen crossfade or blur. |

#### Resizing

- Pointer movement maps directly to the clamped requested geometry in the same input turn.
- There is no post-release momentum, spring, snap-back, or velocity continuation.
- During drag, disable CSS geometry transitions and text selection; change only the requested custom property.
- Native Browser bounds preserve one-pixel fidelity, suppress duplicates, and retain one in-flight plus latest pending backpressure.
- The rail brightens while active. A subtle edge shadow may indicate separation; `filter: blur()` is forbidden on text, Browser content, and active resize paths.
- On release, persist the final size and run one duplicate-suppressed convergence update.

#### Streamed text

- Network deltas accumulate in model state.
- A single rAF callback commits the latest accumulated text. Multiple deltas before a frame coalesce.
- The live assistant body is persistent. Do not replace the bubble, tool region, or transcript with `innerHTML` per delta.
- While streaming, render a stable plain-text tail or boundary-safe incremental markdown; perform one rich-markdown reconciliation when the turn settles.
- Keep the user at the bottom only if they were already within the follow threshold. If they scroll away, show a compact “jump to live” control; never fight their scroll.
- Measure scroll intent before writes and update scroll once after the frame commit.
- Glyphs never use per-token fades, typewriter delay, transforms, blur, or simulated motion blur. These effects create shimmer and reduce reading speed at high refresh rates.

#### Reduced motion and degraded hardware

`prefers-reduced-motion: reduce` changes durations to 1 ms, removes transforms, retains state-color changes, and keeps all geometry functional. When long animation frames are detected, stop decorative transitions before compromising input, stream, or Browser geometry. Motion is progressive enhancement; truth and control are not.

## Do's and Don'ts

### Do

- Make the selected work object and its review state obvious.
- Preserve compact icon/text controls with 24 px minimum pointer targets.
- Keep running, waiting, approval, failed, cancelled, and complete distinct in text and color.
- Batch DOM reads/writes and coalesce streamed deltas to the display cadence.
- Keep pane geometry direct under pointer control.
- Preserve focus, scroll, open panels, and user sizes across low-risk navigation.
- Use tooltips and accessible labels for icon-only actions.
- Keep Browser, terminal, diff, and file evidence close to the task that produced them.

### Don't

- Do not turn every region into a rounded card.
- Do not add giant headings, decorative empty space, glass wallpaper, or dashboard tiles.
- Do not hide consequential state behind hover alone.
- Do not animate layout while the user is dragging it.
- Do not use spring overshoot, inertial sidebars, parallax, bounce, or elastic panels.
- Do not blur or individually animate streamed glyphs.
- Do not re-render previous messages, terminal history, or tool rows on every delta.
- Do not claim literal 240 fps without native, compositor, and display evidence.
- Do not fake agent progress, approvals, tests, earnings, or completion.

### Sources informing the forecast

- Cursor, “Introducing Cursor 2.0 and Composer”: https://cursor.com/blog/2-0
- Cursor, “Meet the new Cursor”: https://cursor.com/blog/cursor-3
- OpenAI, “Introducing the Codex app”: https://openai.com/index/introducing-the-codex-app/
- GitHub Docs, “Working with agent sessions in the GitHub Copilot app”: https://docs.github.com/en/copilot/how-tos/github-copilot-app/agent-sessions
- GitHub Docs, “Managing agent sessions”: https://docs.github.com/en/copilot/how-tos/copilot-on-github/use-copilot-agents/manage-and-track-agents
- Google Developers Blog, “The Jules API is Here”: https://developers.googleblog.com/en/level-up-your-dev-game-the-jules-api-is-here/
- Linear, “Design for the AI age”: https://linear.app/now/design-for-the-ai-age
- Microsoft Research, “Magentic-UI: Towards Human-in-the-loop Agentic Systems”: https://www.microsoft.com/en-us/research/publication/magentic-ui-report/
- MDN, `requestAnimationFrame`: https://developer.mozilla.org/en-US/docs/Web/API/Window/requestAnimationFrame
- web.dev, “How to create high-performance CSS animations”: https://web.dev/articles/animations-guide
- W3C, WCAG 2.2 Understanding SC 2.5.8 Target Size (Minimum): https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum

### Mockup set

Mockups live under `docs/design/optimus-vantage-2027/`:

1. `01-primary-workbench.png` — parallel-agent task selected, evidence timeline, diff-oriented inspector, execution dock.
2. `02-browser-review.png` — Browser inspector, annotations, compact command composer, active task continuity.
3. `03-focus-and-empty.png` — collapsed scope rail, useful empty/focus state, command-first onboarding.

Compiled implementation captures:

4. `04-implemented-empty.png` — 1600 × 1000 empty/new-task state from the real HTTP desktop harness.
5. `05-implemented-workbench.png` — 1600 × 1000 transcript + inspector + execution dock after all transitions settle.
6. `06-implemented-focus-640.png` — 640 × 800 responsive focus state proving compact titlebar, rail, and composer fit.

Mockups are directional system views. Stable DOM IDs, existing IPC, native Browser lifecycle, private Browser context, and honest capability boundaries remain normative over any image detail.
