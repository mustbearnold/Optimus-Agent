---
doc_id: spec-012-windows-tauri-packaging
doc_type: reference
plane: work
status: current
authority: canonical
summary: Windows packaging for the Tauri shell — the PowerShell installer stages the Tauri binary and the Wry/WebView2 desktop backend is retired.
reviewed_on: 2026-08-05
review_by: 2026-10-31
knowledge_type: specification
covers:
  - scripts/rebuild-install-relaunch.ps1
  - scripts/rebuild-install-relaunch.sh
  - scripts/gates/check-product-complete-install.py
depends_on:
  - specs/001-desktop-shell/spec.md
  - docs/decisions/0043-no-auto-updater-channel.md
validated_by:
  - scripts/tests/test_rebuild_install_safety.py
---

# Windows Tauri packaging

Status: draft
Owner: optimus-agent-development (prompt-only owner)

## Purpose

Linux is Tauri-exclusive since the 2026-08-05 cutover, but the Windows
installer (`rebuild-install-relaunch.ps1`) still stages the legacy Wry
desktop binary (`optimus-desktop` shell role) alongside the Tauri shell.
This spec retires that fallback on Windows: the PowerShell installer stages
the Tauri binary + React assets + CLI, matching the Linux evidence bar as
closely as the platform allows.

## Requirements

- R1. The PowerShell installer MUST stage the Tauri shell binary
  (`optimus-tauri`) and the CLI, and MUST NOT stage the legacy Wry desktop
  backend, when the `desktop-wry-fallback` ontology row's `removal_when`
  date (2026-10-31) passes or the owner approves earlier.
- R2. The installer MUST keep the current safety contracts: refuse a foreign
  non-empty install root, existing non-Optimus CLI links, and symlinked
  desktop-entry/icon destinations (mirrors R4 of spec-001).
- R3. WebView2 runtime handling MUST be documented in
  `docs/runbooks/install-relaunch.md` (Tauri on Windows depends on the
  WebView2 runtime; the *backend being retired* is the Wry shell, not the
  OS webview runtime) — [inferred: WebView2 is Tauri's Windows webview
  dependency; the exact bootstrapper choice is an open question].
- R4. The Windows evidence bar MUST be recorded: launch-acceptance on
  Windows = installer contract tests (`test_rebuild_install_safety.py`,
  cross-platform, already green) + a smoke launch of the staged binary.
  The Linux xdotool window check has no Windows equivalent in the gate
  suite — [inferred: no Windows CI host exists on this machine; the
  cross-platform safety tests are the executable floor].
- R5. The `desktop-wry-fallback` ontology row MUST be removed (not merely
  marked) once no installer path stages the Wry binary, per the
  component-database lifecycle law.

## Acceptance criteria

- [ ] A1. Given the ontology row `desktop-wry-fallback` at `removal_when`,
      when the deadline passes, then the row is removed from
      `docs/repository-components.json` and the ontology benchmark stays
      11/11.
- [ ] A2. Given a Windows host with the PowerShell installer, when
      `rebuild-install-relaunch.ps1` runs, then it stages only the Tauri
      binary + CLI + desktop entry, and `check-product-complete-install.py`
      reports `desktop_shell react-tauri` with no Wry shell staged.
- [ ] A3. Given the safety suite, when `test_rebuild_install_safety.py`
      runs, then all Windows-installer contract cases pass (foreign-root
      refusal, symlink refusal, owned reparse safety, portability).
- [ ] A4. Given the runbook, when `docs/runbooks/install-relaunch.md` is
      read, then it documents the Windows WebView2 runtime requirement and
      the exact smoke-launch command for the staged Tauri binary.

## Out of scope

- macOS packaging (no macOS target in the ontology).
- WebView2 runtime distribution/bootstrapping policy (open question below).
- Changing the Linux gate topology (xdotool checks stay Linux-only).

## Open questions

- WebView2 runtime: rely on the OS-installed Evergreen runtime, or bundle a
  bootstrapper? (Affects R3's documentation and the installer's download
  surface; the no-auto-updater ADR-0043 constrains self-updating, not
  runtime bootstrapping.)
- Should the Linux launch-acceptance gate gain a `--windows-smoke` sibling
  that only asserts binary metadata + help text, to keep a Windows-shaped
  gate in the suite? [inferred as desirable; owner decision]

## Links

- `specs/001-desktop-shell/spec.md` — the Tauri shell capability (this spec
  is its Windows packaging slice).
- `docs/decisions/0043-no-auto-updater-channel.md` — updater constraint.
- `docs/runbooks/install-relaunch.md` — installer profiles + verification.
- `docs/repository-components.json` — `desktop-wry-fallback` row (removal
  tracked here).
