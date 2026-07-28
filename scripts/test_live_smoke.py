#!/usr/bin/env python3
"""Offline self-test for live_smoke.py's pure assertion core.

The live legs spend real tokens and cannot run in CI; what CI *can* hold is
the assertion logic those legs rely on, so a green live run can be trusted.
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from live_smoke import nonce, prompt_for, transcript_ok  # noqa: E402


def main() -> int:
    failures: list[str] = []

    token = nonce()
    if not token.startswith("LIVE-") or len(token) != 11:
        failures.append(f"nonce shape drifted: {token}")
    if token == nonce():
        failures.append("nonce is not fresh per call")
    if token not in prompt_for(token):
        failures.append("prompt does not carry the nonce")

    ok, _ = transcript_ok(f"model gpt-5.6\n{token}\n[provider=codex]", token)
    if not ok:
        failures.append("a genuine echo must pass")

    ok, why = transcript_ok("model gpt-5.6\nsomething else\n", token)
    if ok or "nonce" not in why:
        failures.append("a reply without the nonce must fail naming the nonce")

    ok, why = transcript_ok(f"{token}\ncodex is not connected — sign in", token)
    if ok or "not connected" not in why:
        failures.append("a visible provider error must fail even when the nonce appears")

    for failure in failures:
        print(f"ERROR: {failure}", file=sys.stderr)
    if not failures:
        print("LIVE_SMOKE_SELFTEST_OK")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
