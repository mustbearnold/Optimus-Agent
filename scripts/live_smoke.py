#!/usr/bin/env python3
"""Live-model smoke: prove the product against a real provider, end to end.

The deterministic tiers in verify.sh run every gate on the offline provider,
which is right for CI and wrong as the *only* evidence: auth, streaming wire
shape, provider quirks, and model behaviour are exactly the parts offline
cannot touch. This tier drives the real binary against real Codex (the
product's default live provider) twice:

  leg 1 — host path: one-shot `optimus chat --provider codex` with a nonce
          prompt; the reply must echo the nonce, so a cached or scripted
          transcript cannot pass.
  leg 2 — terminal face: the actual TUI in a tmux pty, same nonce scheme,
          asserting the answer lands in the transcript and no error row is
          painted. The user's remembered provider preference is snapshotted
          and restored, so a smoke run never rewrites a human's choice.

Design rules:
  * Missing credentials, missing tmux, or a failed turn are FAILURES, never
    skips — a live tier that can quietly skip is the self-serving green the
    north-star criteria ban (C6 language).
  * Not part of `verify.sh all`: it spends real tokens and needs a real
    credential, which CI does not have. It is the release / TUI-change gate:
    run `just live` (or `scripts/verify.sh live`) before claiming a live
    surface works.
  * One short prompt per leg at minimal thinking — bounded cost by design.

Exit 0 with LIVE_SMOKE_OK on success; exit 1 with the failing leg's evidence.
"""

from __future__ import annotations

import argparse
import secrets
import shutil
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TMUX_SESSION = "optimus-live-smoke"


def nonce() -> str:
    return f"LIVE-{secrets.token_hex(3).upper()}"


def prompt_for(token: str) -> str:
    return f"Reply with exactly this token and nothing else: {token}"


def transcript_ok(text: str, token: str) -> tuple[bool, str]:
    """Pure assertion core, unit-tested offline in test_live_smoke.py."""
    if token not in text:
        return False, f"reply does not echo the nonce {token}"
    lowered = text.lower()
    # The TUI paints failures as an error row; the one-shot prints to stderr.
    for marker in ("sign-in failed", "not connected", "could not reach"):
        if marker in lowered:
            return False, f"provider error visible in output: {marker!r}"
    return True, ""


def run_one_shot(binary: Path, home: Path) -> str:
    token = nonce()
    command = [
        str(binary),
        "--home",
        str(home),
        "chat",
        "--provider",
        "codex",
        "--thinking",
        "minimal",
        prompt_for(token),
    ]
    result = subprocess.run(
        command,
        capture_output=True,
        text=True,
        timeout=180,
        check=False,
    )
    output = result.stdout + result.stderr
    if result.returncode != 0:
        raise SystemExit(f"FAIL leg1 (host one-shot): exit {result.returncode}\n{output[-2000:]}")
    ok, why = transcript_ok(result.stdout, token)
    if not ok:
        raise SystemExit(f"FAIL leg1 (host one-shot): {why}\n{output[-2000:]}")
    model = next(
        (line.split(" ", 1)[1] for line in result.stdout.splitlines() if line.startswith("model ")),
        "unknown",
    )
    print(f"leg1 host one-shot: codex answered on {model}")
    return model


def tmux(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(["tmux", *args], capture_output=True, text=True, check=False)


def capture() -> str:
    return tmux("capture-pane", "-t", TMUX_SESSION, "-p").stdout


def run_tui(binary: Path, home: Path) -> None:
    if shutil.which("tmux") is None:
        raise SystemExit("FAIL leg2 (TUI): tmux is required for the pty drive and is not installed")
    prefs = home / "tui-preferences.json"
    saved = prefs.read_bytes() if prefs.exists() else None
    tmux("kill-session", "-t", TMUX_SESSION)
    token = nonce()
    try:
        started = tmux(
            "new-session",
            "-d",
            "-s",
            TMUX_SESSION,
            "-x",
            "110",
            "-y",
            "32",
            f"'{binary}' --home '{home}'",
        )
        if started.returncode != 0:
            raise SystemExit(f"FAIL leg2 (TUI): tmux could not start: {started.stderr}")
        time.sleep(2)
        if "ready" not in capture():
            raise SystemExit(f"FAIL leg2 (TUI): no ready status line\n{capture()[-2000:]}")
        tmux("send-keys", "-t", TMUX_SESSION, "/provider codex", "Enter")
        time.sleep(1)
        tmux("send-keys", "-t", TMUX_SESSION, prompt_for(token), "Enter")
        deadline = time.time() + 120
        text = ""
        while time.time() < deadline:
            text = capture()
            if token in text and "ready" in text:
                break
            time.sleep(2)
        ok, why = transcript_ok(text, token)
        if not ok or "ready" not in text:
            raise SystemExit(
                f"FAIL leg2 (TUI): {why or 'turn never settled back to ready'}\n{text[-2000:]}"
            )
        print("leg2 TUI pty: codex answer painted in the transcript")
    finally:
        tmux("kill-session", "-t", TMUX_SESSION)
        # A human's remembered provider choice must survive a smoke run.
        if saved is not None:
            prefs.write_bytes(saved)
        elif prefs.exists():
            prefs.unlink()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--home",
        type=Path,
        default=Path.home() / ".local" / "share" / "optimus",
        help="Optimus home holding real credentials (default: the installed product home)",
    )
    parser.add_argument(
        "--binary",
        type=Path,
        default=ROOT / "target" / "debug" / "optimus",
        help="optimus binary to smoke (default: target/debug/optimus)",
    )
    args = parser.parse_args()

    if not args.binary.exists():
        raise SystemExit(f"FAIL: binary {args.binary} not built — cargo build -p optimus-cli")
    if not (args.home / "auth.json").exists():
        raise SystemExit(
            f"FAIL: {args.home} holds no auth.json — connect Codex first (optimus auth); "
            "a live tier with no credential is a failure, not a skip"
        )

    model = run_one_shot(args.binary, args.home)
    run_tui(args.binary, args.home)
    print(f"LIVE_SMOKE_OK model={model}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
