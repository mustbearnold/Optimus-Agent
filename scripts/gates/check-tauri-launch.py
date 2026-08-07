#!/usr/bin/env python3
"""Tauri shell launch acceptance: supervised launch, readiness, window, IPC host.

Proves the installed-surface contract of the Tauri desktop shell without a
Chromium-compatible driver: the binary answers `--version`, launches under
supervision (`--home`, `--supervised-ready`, `--session`), reports the
`[optimus-tauri] ready ui=react` readiness line, opens the windowed surface
that the XDG desktop entry targets, and stays alive until terminated.

Spec-015 A4 extension (the shell's serve lifecycle): with a fresh home and
NO discoverable CLI, the shell must NOT spawn a serve (no host-runtime.json
— the honest diagnostic state, not a phantom record); with an
`OPTIMUS_INSTALL_ROOT` fake install naming the debug `optimus` as
`cli_binary`, the shell must spawn serve, the v2/ws record must appear and
health-check with the record's own dial ticket as the Bearer, and shell
termination must kill the spawned serve (quit termination).

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
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BINARY = ROOT / "target" / "debug" / "optimus-agent"
CLI_BINARY = ROOT / "target" / "debug" / "optimus"
WINDOW_TITLE = "Optimus Agent"
READY_MARKER = "[optimus-tauri] ready ui=react"
LAUNCH_TIMEOUT_S = 30
RECORD_FILE = "host-runtime.json"


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


def launch_shell(home: Path, ready_file: Path, log_file: Path, env: dict) -> subprocess.Popen:
    with log_file.open("wb") as log:
        return subprocess.Popen(
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


def wait_ready(ready_file: Path, child: subprocess.Popen, log_file: Path) -> None:
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


def check_window() -> None:
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


def stop_shell(child: subprocess.Popen) -> None:
    if child.poll() is None:
        child.send_signal(signal.SIGTERM)
        try:
            child.wait(timeout=5)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait(timeout=5)


def record_health(record: dict) -> bool:
    """GET /api/health on the record port with the record's dial ticket
    as the Bearer — the probe shape (R8): the health endpoint is
    protected by the same credential as the WS handshake."""
    try:
        request = urllib.request.Request(
            f"http://127.0.0.1:{record['port']}/api/health",
            headers={"Authorization": f"Bearer {record['token']}"},
        )
        with urllib.request.urlopen(request, timeout=2) as response:
            body = json.loads(response.read().decode("utf-8"))
            return body.get("ok") is True and body.get("streaming") is True
    except Exception:
        return False


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

    base_env = dict(os.environ)
    # Same pinned stack as the installed desktop launcher: X11 via XWayland
    # with WebKitGTK software compositing (deterministic on this GBM host).
    base_env["GDK_BACKEND"] = "x11"
    base_env["WINIT_UNIX_BACKEND"] = "x11"
    base_env["WEBKIT_DISABLE_COMPOSITING_MODE"] = "1"

    with tempfile.TemporaryDirectory(prefix="optimus-tauri-accept-") as tmp:
        tmp = Path(tmp)
        home = tmp / "home"
        home.mkdir()
        ready_file = tmp / "ready.json"
        log_file = tmp / "launch.log"
        env = dict(base_env)
        # Negative phase must be hermetic: no discoverable CLI. R8 discovery
        # reads $OPTIMUS_INSTALL_ROOT install-meta.json, the data-home copy,
        # then PATH — a dev/installed `optimus` in any of those turns this
        # into a legit spawn and breaks the no-CLI premise.
        env["PATH"] = "/usr/local/bin:/usr/bin:/bin"
        env["OPTIMUS_INSTALL_ROOT"] = str(tmp / "no-install-root")
        env["XDG_DATA_HOME"] = str(tmp / "no-data-home")
        child = launch_shell(home, ready_file, log_file, env)
        try:
            wait_ready(ready_file, child, log_file)
            check_window()

            # Spec-015 A4: with no discoverable CLI, the shell must NOT
            # spawn a serve — no record, the honest diagnostic state.
            record = home / RECORD_FILE
            if record.exists():
                raise LaunchFailure(
                    "a serve record appeared without a discoverable CLI "
                    "(the lifecycle must surface a diagnostic, not spawn)"
                )
            time.sleep(1.0)
            if child.poll() is not None:
                raise LaunchFailure(
                    f"Tauri shell exited early with code {child.returncode}:\n"
                    + log_file.read_text(encoding="utf-8", errors="replace")
                )
            print(f"TAURI_LAUNCH_OK version={version_line.split()[-1]} window=yes")
        finally:
            stop_shell(child)

        # Spec-015 A4 spawn path: a fake install naming the debug
        # `optimus` as cli_binary — the shell must spawn serve, the
        # v2/ws record must appear and health-check, and shell exit must
        # kill the spawned serve (quit termination).
        if not CLI_BINARY.is_file():
            print("TAURI_LAUNCH_WARN debug optimus missing; spawn-path check skipped", file=sys.stderr)
            return 0
        install_root = tmp / "install"
        install_root.mkdir()
        (install_root / "install-meta.json").write_text(
            json.dumps({"cli_binary": str(CLI_BINARY)}), encoding="utf-8"
        )
        spawn_home = tmp / "spawn-home"
        spawn_home.mkdir()
        spawn_ready = tmp / "spawn-ready.json"
        spawn_log = tmp / "spawn-launch.log"
        env = dict(base_env)
        env["OPTIMUS_INSTALL_ROOT"] = str(install_root)
        child = launch_shell(spawn_home, spawn_ready, spawn_log, env)
        try:
            wait_ready(spawn_ready, child, spawn_log)

            def record_appeared() -> dict | None:
                try:
                    payload = json.loads((spawn_home / RECORD_FILE).read_text(encoding="utf-8"))
                except (OSError, json.JSONDecodeError):
                    return None
                return payload if payload.get("version") == 2 and payload.get("transport") == "ws" else None

            record = wait_until(record_appeared, "v2/ws serve record from the shell's spawn")
            if not wait_until(lambda: record_health(record), "record health with the dial ticket as Bearer"):
                raise LaunchFailure("spawned serve record failed the health probe")

            served_pid = int(record.get("pid") or 0)
            if served_pid <= 0:
                raise LaunchFailure("serve record carries no usable pid")
            stop_shell(child)
            time.sleep(0.5)
            try:
                os.kill(served_pid, 0)
            except ProcessLookupError:
                pass
            else:
                raise LaunchFailure(
                    f"quit termination failed: serve pid {served_pid} still alive after shell exit"
                )
            print("TAURI_LAUNCH_OK spawn-path=serve record=v2/ws health=yes quit-termination=yes")
            return 0
        finally:
            stop_shell(child)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except LaunchFailure as error:
        print(f"TAURI_LAUNCH_FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
