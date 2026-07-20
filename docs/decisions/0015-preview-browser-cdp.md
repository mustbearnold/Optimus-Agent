# ADR-0015: Preview Browser via CDP (not iframe-only)

## Status
Accepted (design); implementation phased P11+

## Context
Codex-class preview requires real localhost pages, multi-tab, screenshots, and annotation→agent loops. Iframe-only preview cannot support CDP, reliable localhost tooling, or shared agent control.

## Decision
1. **Primary engine:** CDP-controlled Chromium/Edge process managed by a new `optimus-browser` crate (future).
2. **UI chrome** stays in Optimus WebView2; browser content is mirrored/controlled via CDP.
3. **Agent `browser_*` tools** and the right-rail Preview Browser share one browser session.
4. **Degraded mode:** if CDP unavailable, show banner and limited preview; never fake full capability.

## Security
- localhost-first allowlist modes
- SSRF controls
- download quarantine under `{home}/downloads`
- audit log

## Consequences
- Extra process lifecycle complexity on Windows
- Superior parity with Codex annotate workflow
- Doctor flag `preview_browser` flips true only when CDP session can start

## See also
`.hermes/plans/2026-07-19_134540-sidebar-parity-codex-preview-browser-spec.md`
