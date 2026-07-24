#!/usr/bin/env python3
"""Phase-5 observability merge gate.

Fail-closed local gate for kernel / runtime / packs / eval changes:

  1. Offline integrity integration suite (required cases live in optimus-eval)
  2. Causal reconstruction + security-denial unit tests (optimus-kernel)

Usage:
  python3 scripts/check-observability-gate.py
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def run(label: str, command: list[str]) -> None:
    print(f"OBS_GATE: {label}", flush=True)
    print(" ", " ".join(command), flush=True)
    completed = subprocess.run(command, cwd=ROOT, check=False)
    if completed.returncode != 0:
        raise SystemExit(f"OBS_GATE_FAILED: {label}")


def main() -> int:
    run(
        "integrity integration",
        [
            "cargo",
            "test",
            "-p",
            "optimus-eval",
            "--test",
            "integrity_integration",
            "--",
            "--test-threads=1",
        ],
    )
    run(
        "causal reconstruction + security denial codes",
        [
            "cargo",
            "test",
            "-p",
            "optimus-kernel",
            "--test",
            "causal_trace",
            "--",
            "--test-threads=1",
        ],
    )
    print("OBS_GATE_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
