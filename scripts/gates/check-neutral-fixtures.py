#!/usr/bin/env python3
"""Keep machine-specific paths out of app source and fixtures (#103).

The React fixtures once carried the developer's real workspace root as
literal data. Nothing compared it to the actual filesystem — fixtureTransport
is the offline stand-in for the desktop IPC bridge, and the value only meets
itself — but the string went stale the moment the tree moved (#102), and a
path move ended up editing application source. Fixtures use the neutral
synthetic root `/projects/optimus-agent` instead; this gate keeps every
machine-specific spelling from coming back, under any prefix a real checkout
has lived at.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

# Directories whose sources ship to another machine and must not know this one.
SCANNED = ["apps"]

SUFFIXES = {".ts", ".tsx", ".cjs", ".mjs", ".js", ".rs", ".json", ".html", ".css"}
SKIPPED_DIRS = {"node_modules", "dist", "target", "playwright-report", "test-results"}

# Any spelling that identifies a real checkout of this repo on a real machine.
FORBIDDEN = re.compile(
    r"/home/[A-Za-z0-9_.-]+/Projects|/mnt/Projects|mustbearnold"
)


def scan_tree(root: Path) -> list[str]:
    """Return `file:line: snippet` findings for machine-specific paths.

    `root` is the repository tree to scan; extracted so the gate is testable
    against a synthetic tree instead of the live checkout.
    """
    errors: list[str] = []
    for base in SCANNED:
        for path in (root / base).rglob("*"):
            if not path.is_file() or path.suffix not in SUFFIXES:
                continue
            if SKIPPED_DIRS & set(path.parts):
                continue
            try:
                text = path.read_text(encoding="utf-8")
            except (OSError, UnicodeDecodeError):
                continue
            for number, line in enumerate(text.splitlines(), start=1):
                if FORBIDDEN.search(line):
                    rel = path.relative_to(root)
                    errors.append(f"{rel}:{number}: {line.strip()[:100]}")
    return errors


def main() -> int:
    errors = scan_tree(ROOT)

    if errors:
        print(
            "ERROR: machine-specific path in app source — use the neutral "
            "fixture root /projects/optimus-agent (#103):",
            file=sys.stderr,
        )
        for error in errors:
            print(f"  {error}", file=sys.stderr)
        return 1

    print("neutral fixtures check OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
