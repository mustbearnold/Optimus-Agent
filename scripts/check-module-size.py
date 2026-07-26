#!/usr/bin/env python3
"""Enforce the module-size law from the architecture blueprint.

`docs/architecture/optimus-exceeds-hermes.md` section 2.1 diagnoses Hermes'
accretion god-modules (`gateway/run.py` ~24k LOC, `cli.py` ~16k) and states the
rule Optimus exists to hold:

    no module > ~800 LOC without a forced split

The rule was never mechanised, and 14 files now exceed it. A hard limit today
would fail the tree and get switched off, so this gate is a **ratchet**:

  * a file not in the baseline must be <= LIMIT
  * a baselined file may shrink, never grow
  * shrinking below LIMIT retires it from the baseline permanently

That stops the bleeding immediately and applies steady downward pressure
without demanding a large refactor up front.

Size is measured in **production** lines: everything before the first
`#[cfg(test)]`. Counting a file's own inline tests against it would penalise
good coverage, which is the opposite of what the rule is for.

Exit 0 when clean; exit 1 with findings.

  python3 scripts/check-module-size.py            # gate
  python3 scripts/check-module-size.py --report   # ranked sizes, no gating
  python3 scripts/check-module-size.py --update   # rewrite baseline after a shrink
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASELINE = ROOT / "docs" / "architecture" / "module-size-baseline.json"
LIMIT = 800
SCAN_ROOTS = ("crates", "apps")


def production_lines(path: Path) -> int:
    """Lines before the first inline `#[cfg(test)]` module, else all lines."""
    lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
    for index, line in enumerate(lines):
        if line.strip().startswith("#[cfg(test)]"):
            return index
    return len(lines)


def source_files() -> list[Path]:
    found: list[Path] = []
    for base in SCAN_ROOTS:
        for path in (ROOT / base).rglob("*.rs"):
            parts = path.relative_to(ROOT).parts
            # Only crate/app `src` trees. Integration tests under `tests/` are
            # excluded: long fixture-heavy test files are not god-modules.
            if "target" in parts or "src" not in parts:
                continue
            found.append(path)
    return sorted(found)


def measure() -> dict[str, int]:
    return {
        str(path.relative_to(ROOT)): production_lines(path) for path in source_files()
    }


def load_baseline() -> dict[str, int]:
    if not BASELINE.exists():
        return {}
    return json.loads(BASELINE.read_text(encoding="utf-8"))["files"]


def write_baseline(sizes: dict[str, int]) -> None:
    over = {name: size for name, size in sorted(sizes.items()) if size > LIMIT}
    BASELINE.write_text(
        json.dumps(
            {
                "comment": (
                    "Grandfathered modules over the 800-line law in "
                    "docs/architecture/optimus-exceeds-hermes.md section 2.1. "
                    "Ratchet: these may shrink, never grow. A file that drops "
                    "to 800 or below is removed and can never regress. Do not "
                    "add entries by hand — new files must be <= 800."
                ),
                "limit": LIMIT,
                "measure": "production lines, excluding the inline #[cfg(test)] module",
                "files": over,
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", action="store_true", help="print sizes, no gating")
    parser.add_argument("--update", action="store_true", help="rewrite the baseline")
    args = parser.parse_args()

    sizes = measure()

    if args.report:
        for name, size in sorted(sizes.items(), key=lambda kv: -kv[1])[:25]:
            flag = "  OVER" if size > LIMIT else ""
            print(f"{size:6d}  {name}{flag}")
        over = sum(1 for size in sizes.values() if size > LIMIT)
        print(f"\n{len(sizes)} files, {over} over {LIMIT}")
        return 0

    if args.update:
        write_baseline(sizes)
        kept = sum(1 for size in sizes.values() if size > LIMIT)
        print(f"baseline updated: {kept} grandfathered files")
        return 0

    baseline = load_baseline()
    errors: list[str] = []
    shrunk: list[str] = []

    for name, size in sorted(sizes.items()):
        allowed = baseline.get(name)
        if allowed is None:
            if size > LIMIT:
                errors.append(
                    f"{name}: {size} lines exceeds the {LIMIT}-line limit. "
                    f"Split it — do not add it to the baseline."
                )
        elif size > allowed:
            errors.append(
                f"{name}: grew {allowed} -> {size} lines. Baselined modules may "
                f"only shrink."
            )
        elif size <= LIMIT:
            shrunk.append(f"{name}: now {size} lines — retire it from the baseline")
        elif size < allowed:
            shrunk.append(f"{name}: shrank {allowed} -> {size} — ratchet the baseline")

    for name in sorted(set(baseline) - set(sizes)):
        shrunk.append(f"{name}: gone — remove it from the baseline")

    if errors:
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        return 1

    if shrunk:
        for note in shrunk:
            print(f"NOTE: {note}")
        print("\nRun `python3 scripts/check-module-size.py --update` to ratchet.")

    over = sum(1 for size in sizes.values() if size > LIMIT)
    print(
        f"module-size ok files={len(sizes)} over_limit={over} "
        f"baselined={len(baseline)} limit={LIMIT}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
