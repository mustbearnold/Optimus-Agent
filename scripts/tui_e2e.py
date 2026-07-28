#!/usr/bin/env python3
"""Deterministic pty gate for the terminal face.

The desktop face has Playwright driving the real binary inside `verify.sh`;
until this script, the terminal face — surface #1 — had nothing equivalent in
the gate, and pty-only defects (paste handling, key routing, exit behaviour)
shipped invisibly. This drives the *real* `optimus` TUI in a tmux pty on the
offline provider: fresh temp home, no credentials, no network, no token spend
— the same determinism contract as the rest of `verify.sh`, so it runs in
`ui`/`all` beside Playwright. The live-model dimension is scripts/live_smoke.py.

Checks, in one session: launch reaches ready; a turn paints the user bubble,
the offline echo, and settles back to ready; /help paints the command list;
/new clears the transcript; Ctrl-C at idle exits the process; a relaunch on
the same home reaches ready again (no lock or state corruption).

Exit 0 with TUI_E2E_OK; exit 1 naming the failed check with the last frame.
"""

from __future__ import annotations

import argparse
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SESSION = "optimus-tui-e2e"


def tmux(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(["tmux", *args], capture_output=True, text=True, check=False)


def capture() -> str:
    return tmux("capture-pane", "-t", SESSION, "-p").stdout


def alive() -> bool:
    return tmux("has-session", "-t", SESSION).returncode == 0


def wait_for(predicate, timeout: float, what: str) -> str:
    deadline = time.time() + timeout
    text = ""
    while time.time() < deadline:
        text = capture()
        if predicate(text):
            return text
        time.sleep(0.4)
    raise SystemExit(f"FAIL: {what} (within {timeout}s)\n--- last frame ---\n{text}")


def launch(binary: Path, home: Path) -> None:
    started = tmux(
        "new-session", "-d", "-s", SESSION, "-x", "110", "-y", "32",
        f"'{binary}' --home '{home}'",
    )
    if started.returncode != 0:
        raise SystemExit(f"FAIL: tmux could not start the TUI: {started.stderr}")
    wait_for(lambda t: "ready" in t, 15, "launch never reached the ready status line")


def send(*keys: str) -> None:
    tmux("send-keys", "-t", SESSION, *keys)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target" / "debug" / "optimus")
    args = parser.parse_args()
    if not args.binary.exists():
        raise SystemExit(f"FAIL: {args.binary} not built — cargo build -p optimus-cli")

    tmux("kill-session", "-t", SESSION)
    with tempfile.TemporaryDirectory(prefix="optimus-tui-e2e-") as scratch:
        home = Path(scratch) / "home"
        home.mkdir()
        try:
            launch(args.binary, home)

            # A real offline turn, end to end through handle_ipc.
            send("e2e ping", "Enter")
            wait_for(
                lambda t: "offline echo: e2e ping" in t and "ready" in t,
                30,
                "the offline turn never painted its echo and settled",
            )

            # Slash commands answer locally.
            send("/help", "Enter")
            wait_for(lambda t: "/providers" in t, 10, "/help never painted the command list")

            # /new clears the transcript.
            send("/new", "Enter")
            wait_for(
                lambda t: "offline echo: e2e ping" not in t and "fresh session" in t,
                10,
                "/new did not clear the transcript",
            )

            # Ctrl-C at idle exits the process, restoring the terminal.
            send("C-c")
            deadline = time.time() + 10
            while time.time() < deadline and alive():
                time.sleep(0.3)
            if alive():
                raise SystemExit(f"FAIL: Ctrl-C at idle did not exit\n{capture()}")

            # Same home relaunches clean: no lock, no corrupted state.
            launch(args.binary, home)
        finally:
            tmux("kill-session", "-t", SESSION)

    print("TUI_E2E_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
