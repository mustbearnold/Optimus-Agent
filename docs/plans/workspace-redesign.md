# Optimus workspace redesign

Status: implementation in progress
Owner: `apps/optimus-ui`
Completion branch: implementation

## Experience contract

### Product and outcome

- **Primary user:** a developer supervising local and delegated agent work.
- **Primary job:** move a scoped task from intent to a verified result without losing control of effects, state, or evidence.
- **Durable outcome:** the session, resulting files/artifacts, approvals, and receipts remain inspectable after the run.
- **Definition of done:** the useful result is visible; current run state is truthful; stop, approval, and recovery controls remain reachable; evidence is available without reading raw activity.
- **Partial value:** completed messages, partial artifacts, and tool receipts remain inspectable after cancellation or failure when the runtime provides them.

### Surface architecture

- **Primary surface:** a session workbench.
- **Dominant object:** the selected session and its useful output.
- **Supporting navigation:** a compact project and session rail.
- **Evidence surface:** browser, files, and artifacts open as a peer pane on wide screens and as an exclusive tab on narrow screens.
- **Activity surface:** tools and approvals remain summarized in the transcript; technical detail belongs in disclosures and the execution dock.
- **Persistent controls:** composer, run control, and pending approval actions.

### Outcome, control, activity, evidence

1. The session result and current conversation remain visually dominant.
2. Send, stop, approve, deny, and surface-switching controls remain stable.
3. Run and tool activity is summarized in context.
4. Browser/files/artifacts and receipts remain directly inspectable.

### Autonomy and consequence

The redesign does not change runtime permissions or approval semantics. Existing exact-action approval, denial, stop, and project authorization behavior remains authoritative. Visual state must not imply that a proposed or dispatched effect completed.

### Responsive contract

- **Wide:** project rail + session workbench + optional evidence pane.
- **Medium:** compact project rail + session workbench + optional evidence pane when width permits.
- **Narrow:** one primary surface at a time, selected by a keyboard-accessible tab list; composer and status stay available on the work surface.
- **Reflow:** no phantom pane may reserve space when evidence is closed; no consequential control may be hidden solely to preserve desktop composition.

### Visual direction

- **Adjectives:** focused, quiet, capable.
- **Composition:** dense developer workbench with one raised work surface inside subdued application chrome.
- **Type:** compact humanist/system sans; monospace only for code and identifiers.
- **Color:** black and very dark gray translucent surfaces, neutral controls, and distinct semantic warning/success/danger colors. Translucency never uses backdrop blur.
- **Shape:** zero-radius cards, controls, overlays, and panels throughout the application.
- **Motion:** short causal transitions. The selected Full access label and icon may use a gentle reduced-motion-safe fire-color cycle; no other ambient animation, glow, blur, or agent theatre.

### Accessibility and reliability

- Preserve named landmarks, native buttons, the conversation log, live status cadence, and keyboard-operable resizers.
- Preserve visible focus and explicit labels for icon-only controls.
- Verify 390px narrow, 768px medium, and wide desktop compositions in both themes.
- Browser-contract evidence does not prove installed native paint, native focus transfer, or child-WebView stacking.

## Implementation slices

### Slice 1 — shell foundation (implemented)

- Replace the phantom closed-workspace column with a single flexible work surface.
- Establish application chrome, rail, session header, transcript, composer, evidence pane, dark theme, and narrow-surface tokens.
- Preserve all IPC, reducer, approval, and conversation behavior.

### Slice 2 — navigation and session identity

- Make project, worktree/root, session status, and pending attention scannable in the rail.
- Consolidate duplicated top-level destinations and clarify the difference between resources, capabilities, and artifacts.
- Add deterministic long-title, many-session, active-run, and archived fixtures.

### Slice 3 — run projection and activity

- Recompose active, awaiting approval, cancelling, failed, disconnected, partial, and completed states around outcome and next action.
- Bound auxiliary timelines and keep raw event detail behind the inspector.
- Add focused reducer/projection and accessibility coverage for every consequential state.

### Slice 4 — evidence workspace

- Unify browser, files, artifacts, terminal, and receipts under a consistent pane contract.
- Preserve per-session pane state, focus handoff, compact-surface selection, and resize behavior.
- Verify compiled-shell and installed-native behavior separately from the source browser harness.

### Slice 5 — secondary routes and release evidence

- Apply the system to Mail, Capabilities, Resources, Artifacts, Settings, dialogs, empty states, and recovery states.
- Remove superseded legacy visual rules after parity is proven.
- Run focused component, responsive, accessibility, native-shell, and Engineering Memory gates before release claims.

## Current evidence limits

- **Observed:** source browser harness at wide and 390px widths; light and dark themes; work-only and split browser layouts; semantic snapshot; no new console errors.
- **Code-backed:** runtime and interaction behavior is unchanged by Slice 1; the new layer is CSS plus its import and motion-contract coverage.
- **Not tested:** installed native paint, platform accessibility tree, screen reader output, 400% zoom, reconnect, cancellation, approval tamper/expiry, and long-history performance.
