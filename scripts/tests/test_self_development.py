#!/usr/bin/env python3
"""Exercise the real Developer Full Access self-development lifecycle.

This is deliberately transport-level acceptance, not a unit-test double. It
starts the real host-only binary, enables a scoped grant, builds the current
repository, launches a separately authenticated child, proves a failed build
does not displace the healthy child, then restarts, emergency-stops, and
revokes the grant. Every action reports wall time in milliseconds.
"""

from __future__ import annotations

import argparse
import json
import os
import socket
import sqlite3
import subprocess
import tempfile
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Callable, TypeVar


ROOT = Path(__file__).resolve().parents[2]
T = TypeVar("T")


def free_port() -> int:
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def timed(label: str, action: Callable[[], T]) -> T:
    started = time.monotonic()
    value = action()
    elapsed_ms = int((time.monotonic() - started) * 1000)
    print(f"action={label} elapsed_ms={elapsed_ms} rc=0", flush=True)
    return value


class Host:
    def __init__(self, binary: Path, home: Path, port: int) -> None:
        self.binary = binary
        self.home = home
        self.port = port
        self.token = f"optimus-self-development-{os.getpid()}-0123456789abcdef"
        self.base = f"http://127.0.0.1:{port}"
        self.process = subprocess.Popen(
            [
                str(binary),
                "--host-only",
                "--host-port",
                str(port),
                "--home",
                str(home),
            ],
            cwd=ROOT,
            env={
                **os.environ,
                "OPTIMUS_HTTP_TOKEN": self.token,
                "OPTIMUS_SUPPRESS_TOKEN_LOG": "1",
            },
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            text=True,
        )

    def health(self) -> dict[str, object]:
        request = urllib.request.Request(
            f"{self.base}/api/health",
            headers={"Authorization": f"Bearer {self.token}"},
        )
        with urllib.request.urlopen(request, timeout=1) as response:
            return json.load(response)

    def wait_healthy(self) -> None:
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            if self.process.poll() is not None:
                raise RuntimeError(f"host exited before health check: {self.stderr_tail()}")
            try:
                if self.health().get("ok"):
                    return
            except (OSError, urllib.error.URLError):
                time.sleep(0.05)
        raise RuntimeError(f"host did not become healthy: {self.stderr_tail()}")

    def call(self, method: str, params: dict[str, object]) -> dict[str, object]:
        body = json.dumps({"id": 1, "method": method, "params": params}).encode()
        request = urllib.request.Request(
            f"{self.base}/api/ipc",
            data=body,
            method="POST",
            headers={
                "Authorization": f"Bearer {self.token}",
                "Origin": self.base,
                "X-Optimus-CSRF": "1",
                "Content-Type": "application/json",
            },
        )
        with urllib.request.urlopen(request, timeout=30) as response:
            envelope = json.load(response)
        if not envelope.get("ok"):
            raise RuntimeError(f"{method}: {envelope}")
        return envelope.get("result") or {}

    def call_error(self, method: str, params: dict[str, object]) -> dict[str, object]:
        body = json.dumps({"id": 1, "method": method, "params": params}).encode()
        request = urllib.request.Request(
            f"{self.base}/api/ipc",
            data=body,
            method="POST",
            headers={
                "Authorization": f"Bearer {self.token}",
                "Origin": self.base,
                "X-Optimus-CSRF": "1",
                "Content-Type": "application/json",
            },
        )
        with urllib.request.urlopen(request, timeout=30) as response:
            return json.load(response)

    def stderr_tail(self) -> str:
        if self.process.stderr is None:
            return ""
        return self.process.stderr.read()[-1200:]

    def close(self) -> None:
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=5)
        if self.process.returncode not in (0, -15):
            raise RuntimeError(
                f"parent host exited with {self.process.returncode}: {self.stderr_tail()}"
            )


def run(binary: Path, workspace: Path, surface: str) -> None:
    parent_port = free_port()
    child_port = free_port()
    with tempfile.TemporaryDirectory(prefix="optimus-self-development-") as home_raw:
        with tempfile.TemporaryDirectory(prefix="optimus-self-development-invalid-") as invalid_raw:
            home = Path(home_raw)
            invalid = Path(invalid_raw)
            host = Host(binary, home, parent_port)
            try:
                timed("self_development_parent_health", host.wait_healthy)
                grant = {
                    "scope": {
                        "kind": "selected_directories",
                        "roots": [str(workspace), str(invalid)],
                    },
                    "capabilities": {
                        "workspace_files": True,
                        "terminal_execution": True,
                        "process_management": True,
                        "package_installation": True,
                        "network_access": True,
                        "external_services": False,
                        "production_systems": False,
                        "secrets": False,
                    },
                    "pause_before_destructive": False,
                    "checkpoint_on_mutation": True,
                }
                timed(
                    "self_development_enable_access",
                    lambda: host.call(
                        "developer_access_enable",
                        {
                            "confirmation": "I understand Developer Full Access risks",
                            "grant": grant,
                        },
                    ),
                )
                session = timed(
                    "self_development_create_handoff_session",
                    lambda: host.call("new_session", {}),
                )
                session_id = session.get("id")
                if not isinstance(session_id, str) or not session_id:
                    raise RuntimeError(f"could not create a handoff session: {session}")
                built = timed(
                    "self_development_build_launch",
                    lambda: host.call(
                        "developer_supervisor_build_launch",
                        {
                            "workspace": str(workspace),
                            "port": child_port,
                            "surface": surface,
                            "session_id": session_id,
                        },
                    ),
                )
                if not built.get("healthy") or built.get("status") != "healthy":
                    raise RuntimeError(f"development child is not healthy: {built}")
                if built.get("surface") != surface:
                    raise RuntimeError(f"development child surface mismatch: {built}")
                if built.get("child_home") == str(home) or not built.get("child_home"):
                    raise RuntimeError(f"development child did not receive a separate home: {built}")
                if built.get("handoff_session_id") != session_id:
                    raise RuntimeError(f"development child did not advertise the handed-off session: {built}")
                child_home = Path(str(built["child_home"]))

                def verify_handoff_snapshot() -> None:
                    marker = json.loads((child_home / "handoff.json").read_text())
                    if marker.get("session_id") != session_id:
                        raise RuntimeError(f"handoff marker mismatch: {marker}")
                    with sqlite3.connect(child_home / "sessions.db") as connection:
                        row = connection.execute(
                            "SELECT title FROM sessions WHERE id = ?", (session_id,)
                        ).fetchone()
                        count = connection.execute("SELECT COUNT(*) FROM sessions").fetchone()[0]
                    if row is None:
                        raise RuntimeError("handed-off session is missing from child session store")
                    if count != 1:
                        raise RuntimeError(f"handoff leaked sibling sessions into child store: {count}")

                timed("self_development_verify_handoff_snapshot", verify_handoff_snapshot)
                build = built.get("build")
                if not isinstance(build, dict) or build.get("surface") != surface:
                    raise RuntimeError(f"development build metadata is incomplete: {built}")
                child_pid = built.get("pid")
                if not isinstance(child_pid, int) or child_pid <= 0:
                    raise RuntimeError(f"development child has no valid pid: {built}")

                logs = timed(
                    "self_development_read_logs",
                    lambda: host.call("developer_supervisor_log", {"lines": 120}),
                )
                if f"build workspace=" not in str(logs.get("build", "")):
                    raise RuntimeError("build log did not record the workspace build")
                if f"surface={surface}" not in str(logs.get("build", "")):
                    raise RuntimeError(f"build log did not record surface={surface}")
                if "developer_supervisor_build_launch" not in str(logs.get("actions", "")):
                    raise RuntimeError("action log did not record the build-and-launch action")
                if surface == "desktop" and "[optimus-tauri] ready ui=react" not in str(logs.get("lines", "")):
                    raise RuntimeError("windowed child did not report Tauri readiness in its instance log")

                failed = timed(
                    "self_development_failed_build_probe",
                    lambda: host.call_error(
                        "developer_supervisor_build_launch",
                        {"workspace": str(invalid), "port": child_port + 1, "surface": surface},
                    ),
                )
                if failed.get("ok") is not False:
                    raise RuntimeError(f"invalid build unexpectedly succeeded: {failed}")
                preserved = timed(
                    "self_development_failed_build_preserves_child",
                    lambda: host.call("developer_supervisor_status", {}),
                )
                if not preserved.get("healthy") or preserved.get("pid") != child_pid:
                    raise RuntimeError(f"failed build displaced healthy child: {preserved}")
                if host.process.poll() is not None:
                    raise RuntimeError("stable parent control channel exited")

                restarted = timed(
                    "self_development_restart_child",
                    lambda: host.call("developer_supervisor_restart", {}),
                )
                if not restarted.get("healthy"):
                    raise RuntimeError(f"restart did not produce a healthy child: {restarted}")
                stopped = timed(
                    "self_development_emergency_stop",
                    lambda: host.call("developer_emergency_stop", {}),
                )
                if stopped.get("healthy") or stopped.get("status") != "emergency_stopped":
                    raise RuntimeError(f"emergency stop did not settle: {stopped}")
                revoked = timed(
                    "self_development_revoke_access",
                    lambda: host.call("developer_access_revoke", {}),
                )
                if revoked.get("developer_access", {}).get("enabled"):
                    raise RuntimeError(f"revoke did not disable access: {revoked}")
                print("SELF_DEVELOPMENT_OK", flush=True)
            finally:
                host.close()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--binary",
        type=Path,
        default=ROOT / "target/debug/optimus-desktop",
        help="built host-only Optimus binary",
    )
    parser.add_argument(
        "--workspace",
        type=Path,
        default=ROOT,
        help="repository to build and supervise",
    )
    parser.add_argument(
        "--surface",
        choices=("host", "desktop"),
        default="host",
        help="development child surface to build and supervise",
    )
    args = parser.parse_args()
    binary = args.binary.resolve()
    workspace = args.workspace.resolve()
    if not binary.is_file():
        print(f"SELF_DEVELOPMENT_FAIL: missing binary {binary}")
        return 1
    if not workspace.is_dir():
        print(f"SELF_DEVELOPMENT_FAIL: missing workspace {workspace}")
        return 1
    try:
        run(binary, workspace, args.surface)
    except (OSError, RuntimeError, urllib.error.URLError) as error:
        print(f"SELF_DEVELOPMENT_FAIL: {error}")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
