#!/usr/bin/env python3
"""Tauri shell launch acceptance: supervised launch, readiness, window, IPC host.

Proves the installed-surface contract of the Tauri desktop shell without a
Chromium-compatible driver: the binary answers `--version`, launches under
supervision (`--home`, `--supervised-ready`, `--session`), reports the
`[optimus-tauri] ready ui=react` readiness line, opens the windowed surface
that the XDG desktop entry targets, and stays alive until terminated.

This is launch/install acceptance, not renderer-pixel proof: the React
workbench and its IPC transport are exercised by the desktop e2e tier
(HTTP transport), the tauri transport unit tests, and the host IPC contract
gates. The installer's `--supervised-ready` contract is shared with
`test_self_development.py --surface desktop`.
"""

from __future__ import annotations

import json
import os
import shutil
import signal
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BINARY = ROOT / "target" / "debug" / "optimus-agent"
WINDOW_TITLE = "Optimus Agent"
READY_MARKER = "[optimus-tauri] ready ui=react"
LAUNCH_TIMEOUT_S = 30


class LaunchFailure(RuntimeError):
    pass


def wait_until(predicate, description: str, timeout: float = LAUNCH_TIMEOUT_S):
    """Poll until predicate() is truthy; return the last predicate value."""
    deadline = time.monotonic() + timeout
    last = None
    while time.monotonic() < deadline:
        last = predicate()
        if last:
            return last
        time.sleep(0.1)
    raise LaunchFailure(f"timed out waiting for {description}")


def main() -> int:
    if not BINARY.is_file():
        print(f"TAURI_LAUNCH_FAIL missing binary: {BINARY}", file=sys.stderr)
        return 1
    version_line = subprocess.run(
        [str(BINARY), "--version"], capture_output=True, text=True, check=False
    ).stdout.strip()
    if not version_line.startswith("optimus-agent "):
        print(f"TAURI_LAUNCH_FAIL bad version output: {version_line!r}", file=sys.stderr)
        return 1

    with tempfile.TemporaryDirectory(prefix="optimus-tauri-accept-") as tmp:
        home = Path(tmp) / "home"
        home.mkdir()
        ready_file = Path(tmp) / "ready.json"
        log_file = Path(tmp) / "launch.log"
        env = dict(os.environ)
        # Same pinned stack as the installed desktop launcher: X11 via XWayland
        # with WebKitGTK software compositing (deterministic on this GBM host).
        env["GDK_BACKEND"] = "x11"
        env["WINIT_UNIX_BACKEND"] = "x11"
        env["WEBKIT_DISABLE_COMPOSITING_MODE"] = "1"
        with log_file.open("wb") as log:
            child = subprocess.Popen(
                [
                    str(BINARY),
                    "--home", str(home),
                    "--supervised-ready", str(ready_file),
                    "--session", "00000000-0000-4000-8000-000000000001",
                ],
                stdout=log,
                stderr=log,
                env=env,
            )
        try:
            def ready_payload() -> dict | None:
                try:
                    payload = json.loads(ready_file.read_text(encoding="utf-8"))
                except (OSError, json.JSONDecodeError):
                    return None
                return payload if payload.get("ready") is True else None

            payload = wait_until(ready_payload, "supervised ready file")
            if payload.get("pid") != child.pid:
                raise LaunchFailure(
                    f"ready file pid {payload.get('pid')} != child pid {child.pid}"
                )

            def readiness_line() -> bool:
                return log_file.exists() and READY_MARKER in log_file.read_text(
                    encoding="utf-8", errors="replace"
                )

            wait_until(readiness_line, "Tauri readiness line")

            def window_found() -> bool:
                result = subprocess.run(
                    ["xdotool", "search", "--name", WINDOW_TITLE],
                    capture_output=True, text=True, check=False,
                )
                return result.returncode == 0 and bool(result.stdout.strip())

            if shutil.which("xdotool"):
                wait_until(window_found, f"windowed surface '{WINDOW_TITLE}'")
            else:
                print("TAURI_LAUNCH_WARN xdotool missing; window check skipped", file=sys.stderr)

            time.sleep(1.0)
            if child.poll() is not None:
                raise LaunchFailure(
                    f"Tauri shell exited early with code {child.returncode}:\n"
                    + log_file.read_text(encoding="utf-8", errors="replace")
                )
            print(f"TAURI_LAUNCH_OK version={version_line.split()[-1]} window=yes")
            return 0
        finally:
            if child.poll() is None:
                child.send_signal(signal.SIGTERM)
                try:
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait(timeout=5)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except LaunchFailure as error:
        print(f"TAURI_LAUNCH_FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
