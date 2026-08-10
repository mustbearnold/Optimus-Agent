#!/usr/bin/env python3
"""Ratchet the impeccable design-quality detector over the React UI.

`impeccable` (pbakaus, Apache-2.0) is a deterministic 60-rule design-quality
detector with a CI mode. It targets exactly the generic-AI-UI tells the
quality mandate bans, which is why the UI surface adopted it (issue #86).

Semantics are a **ratchet**, the same contract as check-module-size.py:

  * every (file, antipattern) finding must be baselined
  * new findings fail the gate
  * a baseline entry may be retired, never silently re-added

The tool is pinned: a newer impeccable with a new rule must not fail the
tree without review. Bump PINNED deliberately, re-run, and reconcile the
baseline (that is the review).

One finding is accepted by decision and lives in the baseline with its
rationale: the preview fixture page's Arial face (`overused-font` on
`.fixture-page`) is a deliberate neutral test surface, not an app style.

Exit 0 when clean; exit 1 with findings.

  python3 scripts/gates/check-impeccable-ratchet.py            # gate
  python3 scripts/gates/check-impeccable-ratchet.py --update   # re-baseline after a shrink
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
UI = ROOT / "apps" / "optimus-ui"
BASELINE = ROOT / "docs" / "architecture" / "impeccable-baseline.json"
PINNED = "3.5.0"


def detect() -> list[dict]:
    proc = subprocess.run(
        ["npx", "-y", f"impeccable@{PINNED}", "detect", "--json", "."],
        cwd=UI,
        capture_output=True,
        text=True,
        timeout=180,
        check=False,
    )
    # impeccable exits 0 when clean and 2 when it detected findings (CI-mode
    # linter convention) — the JSON report is valid either way, and the
    # ratchet decides. Any other exit is a real failure.
    if proc.returncode not in (0, 2):
        sys.exit(f"impeccable detect failed: {proc.stderr.strip() or proc.stdout.strip()}")
    return json.loads(proc.stdout)


def current_set(findings: list[dict]) -> set[tuple[str, str]]:
    return {
        (Path(f["file"]).resolve().relative_to(ROOT).as_posix(), f["antipattern"])
        for f in findings
    }


def load_baseline() -> dict:
    if not BASELINE.exists():
        return {"version": PINNED, "findings": []}
    data = json.loads(BASELINE.read_text())
    return data


def ratchet(current: set[tuple[str, str]], baseline: dict) -> tuple[int, list[str]]:
    """The ratchet core: returns (exit_code, report lines). Split out of main()
    so the semantics are testable without argparse or npx."""
    lines: list[str] = []
    if baseline.get("version") != PINNED:
        lines.append(
            f"impeccable baseline pins v{baseline.get('version')} but the gate runs "
            f"v{PINNED}; reconcile the baseline (--update) before trusting the count."
        )
        return 1, lines

    known = {(f["file"], f["antipattern"]) for f in baseline.get("findings", [])}
    new = current - known
    if new:
        lines.append("new impeccable findings (ratchet):")
        for f, a in sorted(new):
            lines.append(f"  {f}: {a}")
        lines.append("baseline a finding only after fixing or deliberately accepting it (--update).")
        return 1, lines

    retired = known - current
    if retired:
        lines.append("impeccable findings retired (ratchet worked):")
        for f, a in sorted(retired):
            lines.append(f"  {f}: {a}")
        lines.append("re-baseline with --update to lock the shrink.")

    lines.append(f"impeccable@{PINNED}: {len(current)} finding(s), within baseline")
    return 0, lines


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--update", action="store_true", help="re-baseline the current findings")
    args = parser.parse_args()

    findings = detect()
    current = current_set(findings)

    if args.update:
        entries = [
            {"file": f, "antipattern": a}
            for f, a in sorted(current)
        ]
        BASELINE.write_text(
            json.dumps({"version": PINNED, "findings": entries}, indent=2) + "\n"
        )
        print(f"baseline updated: {len(entries)} finding(s) at impeccable@{PINNED}")
        return 0

    code, lines = ratchet(current, load_baseline())
    for line in lines:
        print(line)
    return code


if __name__ == "__main__":
    sys.exit(main())
