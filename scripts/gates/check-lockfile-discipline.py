#!/usr/bin/env python3
"""Pin the flat monorepo package managers: Cargo for Rust, Bun for JS/TS.

A flat monorepo must not silently grow a second JS package manager: a
package-lock.json, yarn.lock, or pnpm-lock.yaml committed by any tool or agent
would fork the dependency graph and drift from the frozen bun.lock. This gate
fails closed when a foreign lockfile is tracked anywhere, when either
ecosystem lockfile is missing from the root, or when the root package.json
stops declaring Bun as the package manager.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
FOREIGN_LOCKFILES = {
    "package-lock.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "npm-shrinkwrap.json",
}
REQUIRED_ROOT_LOCKFILES = {"Cargo.lock", "bun.lock"}


def tracked_files(root: Path = ROOT) -> list[str]:
    result = subprocess.run(
        ["git", "-C", str(root), "ls-files"],
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        raise SystemExit(result.stderr.strip() or "git ls-files failed")
    return [line for line in result.stdout.splitlines() if line.strip()]


def findings(root: Path = ROOT) -> list[str]:
    problems: list[str] = []
    # Query the index once: `git ls-files` is the whole cost of this gate, and
    # each call spawns a subprocess. Scanning foreign lockfiles and checking
    # required roots against one shared set is identical in behaviour and
    # avoids calling git three times per check.
    tracked = tracked_files(root)
    tracked_names = {Path(path).name for path in tracked}
    for path in tracked:
        if Path(path).name in FOREIGN_LOCKFILES:
            problems.append(f"{path}: foreign lockfile tracked; use Bun (bun.lock)")
    for name in sorted(REQUIRED_ROOT_LOCKFILES):
        if name not in tracked_names:
            problems.append(f"{name}: required root lockfile is not tracked")
    manifest = root / "package.json"
    try:
        payload = json.loads(manifest.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        problems.append(f"package.json: cannot read: {error}")
    else:
        manager = str(payload.get("packageManager", ""))
        if not manager.startswith("bun@"):
            problems.append(
                f"package.json: packageManager must declare bun@..., got {manager!r}"
            )
    return problems


def main() -> int:
    problems = findings()
    if problems:
        print("lockfile discipline check FAILED:", file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        return 1
    print("lockfile discipline check OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
