#!/usr/bin/env python3
"""Observability merge gate (Phase 5 + P14 export).

Fail-closed local gate for kernel / runtime / packs / eval changes:

  1. Offline integrity integration suite (required cases live in optimus-eval)
  2. Causal reconstruction + security-denial + export unit tests (optimus-kernel)
  3. Export API surface present (CAUSAL_EXPORT_VERSION + write_causal_export)

Usage:
  python3 scripts/check-observability-gate.py
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def run(label: str, command: list[str]) -> None:
    print(f"OBS_GATE: {label}", flush=True)
    print(" ", " ".join(command), flush=True)
    completed = subprocess.run(command, cwd=ROOT, check=False)
    if completed.returncode != 0:
        raise SystemExit(f"OBS_GATE_FAILED: {label}")


def check_export_surface() -> None:
    causal = ROOT / "crates/optimus-kernel/src/causal.rs"
    cli = ROOT / "apps/optimus-cli/src/main.rs"
    text = causal.read_text(encoding="utf-8")
    if "CAUSAL_EXPORT_VERSION" not in text:
        raise SystemExit("OBS_GATE_FAILED: missing CAUSAL_EXPORT_VERSION")
    if "pub fn write_causal_export" not in text:
        raise SystemExit("OBS_GATE_FAILED: missing write_causal_export")
    if "optimus.causal.v1" not in text:
        raise SystemExit("OBS_GATE_FAILED: missing optimus.causal.v1 format id")
    if not re.search(r"pub fn export_causal_document", text):
        raise SystemExit("OBS_GATE_FAILED: missing export_causal_document")
    cli_text = cli.read_text(encoding="utf-8")
    if "Export {" not in cli_text:
        raise SystemExit("OBS_GATE_FAILED: CLI missing TraceCmd::Export variant")
    if "write_causal_export" not in cli_text:
        raise SystemExit("OBS_GATE_FAILED: CLI missing write_causal_export wiring")
    print("OBS_GATE: export surface OK", flush=True)


def main() -> int:
    check_export_surface()
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
        "causal reconstruction + export + security denial codes",
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
