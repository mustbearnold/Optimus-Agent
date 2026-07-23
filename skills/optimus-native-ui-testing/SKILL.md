---
name: optimus-native-ui-testing
description: Use when changing, repairing, installing, or accepting Optimus Desktop on Linux or Windows. Exercise the installed native Wry shell through accessibility-first controls, verify persistence and session isolation, and keep browser HTTP tests supplemental.
version: 0.2.1
author: Hermes
license: MIT
platforms: [linux, windows]
metadata:
  hermes:
    tags: [Optimus, Native-UI, CUA, WebKitGTK, WebView2, XDG]
    requires_tools: [computer_use, execute_code, terminal, read_file, write_file, patch, project_list, project_switch]
---

# Optimus Native UI Testing

## Overview

Exercise the exact installed Optimus Desktop candidate through its native Wry window. On Ubuntu this means Tao plus WebKitGTK; on Windows it means Tao plus WebView2. Accessibility or an attached native web inspector is the primary functional evidence. Selective screenshots prove paint and layout. Playwright against `--development-http` remains a deterministic supplement and never substitutes for installed-app proof.

A passing process or window is not enough. Acceptance requires a rendered shell, a prompt submitted through the composer, the expected response read back from the native surface, durable SQLite evidence, and a new-session/reopen isolation check.

## When to Use

Use this skill when:

- changing the native shell, composer, sessions, tasks, approvals, timers, sidebars, window controls, or WebView bridge;
- porting or repairing Linux/Windows behavior;
- changing installation, launch, XDG, desktop-entry, or local-app packaging;
- reproducing a user-visible installed-app failure;
- accepting a release candidate after installation.

Do not use it for isolated Rust logic with no user-visible effect, documentation-only work, or browser sites unrelated to Optimus. Do not replace native proof with a direct Kernel/provider call, a database write, or development-HTTP Playwright.

## Platform Contract

| Concern | Ubuntu/Linux | Windows |
|---|---|---|
| Native engine | WebKitGTK 4.1 | WebView2 |
| Normal candidate | `~/.local/share/optimus-agent/bin/optimus-desktop` | `%LOCALAPPDATA%\Programs\OptimusAgent\optimus-desktop.exe` |
| Canonical installer | `bash scripts/rebuild-install-relaunch.sh` | `powershell -File scripts/rebuild-install-relaunch.ps1` |
| Custom-protocol URL | `optimus://localhost/` | `http://optimus.localhost/` |
| User data default | `${XDG_DATA_HOME:-~/.local/share}/optimus` | platform-local Optimus data directory |
| Native inspector | WebKitGTK inspector when explicitly enabled | WebView2 CDP when explicitly enabled |

Wry translates a custom scheme to the `.localhost` HTTP form only on WebView2 and Android. Navigating WebKitGTK to `http://optimus.localhost/` produces a real network error, typically `Could not connect to optimus.localhost: Connection refused`. Browser HTTP tests cannot catch this failure.

## Prerequisites

- `project_list` reports `Optimus Agent` at the intended repository root. Switch projects and re-read `AGENTS.md` if it does not.
- A bounded scenario with exact provider, model, thinking, access, prompt text, and expected observable response.
- A clean ignored evidence directory under `local/tmp/cua-evidence/`.
- No credentials, auth files, tokens, or secret-bearing raw logs in evidence.
- Explicit user authorization before any live or paid provider call. Prefer `Offline / offline-echo` for shell acceptance.
- Linux build packages documented in the repository, including GTK 3 and WebKitGTK 4.1 development libraries.
- The active Rust toolchain includes `rustfmt` (`rustup component add rustfmt`). Fresh release targets compile `headless_chrome`, whose CDP generator invokes `rustfmt`; the canonical installer resolves the real `rustup which rustfmt` path automatically.
- The exact candidate installed or staged. For isolated Linux acceptance, set `XDG_DATA_HOME` and `XDG_BIN_HOME` to an evidence subdirectory before running the installer.

## Evidence Ladder

Use the least invasive rung that produces verifiable evidence:

1. **AX snapshot:** discover the window, controls, labels, and terminal state.
2. **AX action:** click a fresh element handle; recapture after any `unverifiable` result.
3. **Foreground escalation:** use only when the driver reports `background_unavailable`; bring the candidate forward first.
4. **Native inspector:** attach only to the installed WebView process through an approved test seam.
5. **Selective screenshot:** use for blank/error surfaces, clipping, overlap, focus, HiDPI mapping, and final visual acceptance.
6. **Linux virtual keyboard fallback:** use only when WebKitGTK exposes the textarea without EditableText and rejects synthetic X11 key events. Keep the daemon temporary, keyboard-only, private, and tracked.

Never report delivered input as verified input. Require the exact composer value visually or through native DOM/AX before submitting.

## Ubuntu Procedure

1. **Freeze the scenario.** Record the exact binary, isolated or normal Optimus home, provider/model/thinking/access, prompt, and expected response. Completion: every rerun uses unchanged values.

2. **Establish candidate identity.** Check project identity, `git status --short`, target directory, installed executable path, and `/proc/<pid>/exe`. Preserve unrelated changes. Completion: the running PID resolves to the intended installed candidate.

3. **Build and install.** Run focused tests before the canonical installer. For isolated release acceptance:

   ```bash
   export XDG_DATA_HOME="$PWD/local/tmp/cua-evidence/<case>/xdg-data"
   export XDG_BIN_HOME="$PWD/local/tmp/cua-evidence/<case>/bin"
   export CARGO_TARGET_DIR="$PWD/local/tmp/cargo-target-ubuntu-audit"
   bash scripts/rebuild-install-relaunch.sh
   ```

   Completion: installer status validates binaries, desktop entry, icon, metadata, and the launched PID.

4. **Prove normal launch first.** Let the installer launch with the normal desktop backend. Check the process and sanitized launcher log for `ready`. Do not call a healthy Wayland process broken merely because an X11-only driver cannot enumerate it.

5. **Use XWayland only for automation when required.** If the current driver cannot bind the native Wayland surface, stop only the staged candidate and relaunch the exact installed binary with `GDK_BACKEND=x11`. This is an acceptance adapter, not a product default. Completion: window discovery returns title `Optimus Agent`, the expected PID, visible bounds, and an X11 window ID.

6. **Capture before action.** Take a bounded AX snapshot and one initial screenshot. Reject blank pages, network errors, or partial chrome before interacting. On Linux, require the full shell and no `optimus.localhost` connection error.

7. **Confirm in-app configuration.** Read provider, model, thinking, and access controls from the native tree. Change them through fresh native handles. For deterministic acceptance use `Offline` and require `MODEL offline-echo` after recapture.

8. **Drive the composer.** Click or type through the native accessibility tool first. When WebKitGTK reports `background_unavailable`, foreground escalation is allowed. If AX exposes the composer as `entry` but omits EditableText and all synthetic X11 key events are ignored, use the bounded fallback below. Completion: the exact prompt is visibly present once, with no missing or duplicated characters.

9. **Submit through the UI.** Use the fresh Send handle or Return while the composer is focused. Never invoke chat IPC directly as acceptance proof. Poll bounded fresh AX snapshots until a terminal result or timeout.

10. **Bind durable evidence.** Query `sessions.db`, `execution.db`, and `optimus.db` read-only with SQLite URI `mode=ro`. Match the exact session, prompt, response, provider/model metadata, timings, tool calls, and approvals. Do not print credentials.

11. **Verify isolation and reload.** Create a new session through the native control. Require an empty transcript, `Tasks 0`, and `turn —`. Reopen the tested session through the sidebar and require its title and exact response to return while transient tasks/timers remain cleared.

12. **Record and clean up.** Save a concise Markdown ledger plus only necessary screenshots. Stop temporary virtual-input and driver daemons, terminate only staged candidates, and uninstall staged XDG artifacts. Leave a normal user install running only when the task requests it.

## Linux Keyboard Fallback

Use this only after native evidence proves pointer focus works but CUA/XTEST text does not. `ydotool` must be installed with user authority. Do not add it as an Optimus runtime dependency.

Start a tracked temporary daemon with mouse injection disabled and a private socket:

```bash
sudo ydotoold \
  --socket-path=/tmp/optimus-native-ydotool.sock \
  --socket-perm=0600 \
  --socket-own="$(id -u):$(id -g)" \
  --mouse-off
```

Then:

1. Bring the staged Optimus X11 window forward.
2. Click the textarea through AX or a verified physical coordinate.
3. Type only the frozen prompt:

   ```bash
   YDOTOOL_SOCKET=/tmp/optimus-native-ydotool.sock \
     ydotool type --key-delay=8 --key-hold=8 'EXACT_PROMPT'
   ```

4. Immediately recapture and verify the full value.
5. Stop the tracked daemon and remove its socket after the scenario.

Do not leave a root virtual-input daemon running. Do not enable mouse support. Do not use this fallback when normal AX/native DOM typing works.

## Windows Procedure Differences

- Use the canonical PowerShell installer and verify the installed `.exe`, not a stale Cargo target binary.
- Use CUA AX against `Optimus Agent`; attach WebView2 CDP only through an approved installed-process seam.
- Expect `http://optimus.localhost/` for Wry custom-protocol navigation.
- Use WebView2 DOM value assertions when AX does not expose a textarea value.
- Preserve the same prompt, persistence, new-session, and reopen requirements as Linux.

## RED to GREEN Discipline

When native acceptance finds a failure:

1. Save the minimal RED evidence.
2. Identify the native boundary browser tests bypass.
3. Add the smallest deterministic regression possible.
4. Make the smallest coherent source change.
5. Run focused tests and native acceptance again with the unchanged scenario.
6. Update this skill immediately when a reusable platform pitfall was missing.

A deterministic GREEN does not close the case until the installed native candidate is green.

## Common Pitfalls

1. **Process equals UI.** A healthy PID can render a network error. Capture the actual native document.
2. **Wayland invisibility equals crash.** X11 enumeration may not see a healthy Wayland window. Check process/log health before using XWayland for automation.
3. **WebView2 URL on Linux.** `http://optimus.localhost/` is not the WebKitGTK custom-scheme URL.
4. **Stale element handles.** Every snapshot replaces the AX cache. Recapture before each action.
5. **Unverified input.** Character counts and key events do not prove a WebKit textarea changed.
6. **HiDPI confusion.** AX logical frames, X11 physical bounds, and screenshot pixels may use different scales. Derive coordinates from current reported dimensions; do not guess.
7. **Development DOM as native proof.** Playwright HTTP bypasses Wry protocol navigation and native shell behavior.
8. **Live-provider overreach.** Offline shell acceptance needs no credentials or paid call.
9. **Stale transient state after reopen.** Titles and messages persist; tasks and active turn timers should not leak.
10. **Untracked virtual input.** Temporary `ydotoold` must be keyboard-only, private, tracked, and stopped.
11. **Fresh release target cannot find rustfmt.** If `auto_generate_cdp` reports `rustfmt not found` while `rustfmt --version` works, pin the toolchain binary for that invocation: `RUSTFMT="$(rustup which rustfmt)" bash scripts/rebuild-install-relaunch.sh --release`. Keep the installer fallback that resolves this path automatically.

## Verification Checklist

- [ ] Active project and candidate executable identity confirmed
- [ ] Installed candidate launched from the intended path
- [ ] Normal Wayland/Windows launch health checked before automation adaptations
- [ ] Native shell fully rendered with no blank/network-error surface
- [ ] Provider/model/thinking/access read from native controls
- [ ] Exact prompt verified in the native composer before submit
- [ ] Exact response read from the native surface
- [ ] SQLite evidence matched read-only
- [ ] New session is empty with no stale tasks/timer
- [ ] Tested session reopens with title and response intact
- [ ] Final visual checkpoint has no clipping or overlap
- [ ] Deterministic Rust and Playwright gates pass separately
- [ ] Temporary daemons, staged app processes, and staged install artifacts cleaned up
- [ ] Every unresolved native gap stated explicitly
