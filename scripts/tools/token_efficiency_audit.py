#!/usr/bin/env python3
"""Preservation audit for token-efficiency prose edits.

The "without harming quality or accuracy" contract (approved plan
Development/tmp/plan-token-efficiency-draft.md, section 4): a compression
edit may tighten prose, but every citation, marker, and normative sentence
that existed before must still exist after — relocated verbatim at worst,
never dropped or paraphrased.

  python3 scripts/tools/token_efficiency_audit.py --extract FILE
      Print the preservation set as JSON: backticked path references,
      markdown link targets, and normalized normative lines.

  python3 scripts/tools/token_efficiency_audit.py --diff BEFORE AFTER
      Assert AFTER's preservation set is a superset of BEFORE's; print any
      missing items; exit 1 when something was dropped.

Normative lines: lines whose text contains a normative verb (must, never,
mandatory, only, blocked, refused, required, always, forbidden, shall),
normalized by stripping and collapsing whitespace so untouched lines match.

Exit 0 when the after-set covers the before-set; exit 1 with the diff.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

NORMATIVE = re.compile(
    r"\b(must|never|mandatory|only|blocked|refused|required|always|forbidden|shall)\b",
    re.IGNORECASE,
)
BACKTICK_PATH = re.compile(r"`([^`\n]+)`")
MD_LINK = re.compile(r"\[[^\]]*\]\(([^)\s]+)\)")


def extract(text: str) -> dict:
    lines = text.splitlines()
    backtick_refs = sorted(
        {match.group(1) for line in lines for match in BACKTICK_PATH.finditer(line)}
    )
    md_links = sorted(
        {match.group(1) for line in lines for match in MD_LINK.finditer(line)}
    )
    normative = sorted(
        {
            " ".join(line.split())
            for line in lines
            if NORMATIVE.search(line) and line.strip()
        }
    )
    return {"backtick_refs": backtick_refs, "md_links": md_links, "normative": normative}


def diff_sets(before: dict, after: dict) -> list[str]:
    missing: list[str] = []
    for category in ("backtick_refs", "md_links", "normative"):
        for item in before[category]:
            if item not in after[category]:
                missing.append(f"{category}: {item!r}")
    return missing


def read_source(path: str) -> str:
    """Read a UTF-8 source file, converting a missing or unreadable file into
    a clear audit failure instead of a raw traceback.

    A failed diff should say *what* could not be read and exit 1, not dump a
    FileNotFoundError stack — the gate's failure must name the missing file so
    the author can fix it in one step.
    """
    try:
        return Path(path).read_text(encoding="utf-8")
    except OSError as error:
        raise SystemExit(f"AUDIT FAILED — cannot read {path}: {error}") from error


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--extract", metavar="FILE")
    parser.add_argument("--diff", nargs=2, metavar=("BEFORE", "AFTER"))
    args = parser.parse_args()

    if args.extract:
        print(json.dumps(extract(read_source(args.extract)), indent=2))
        return 0

    if args.diff:
        before = extract(read_source(args.diff[0]))
        after = extract(read_source(args.diff[1]))
        missing = diff_sets(before, after)
        if missing:
            print("AUDIT FAILED — dropped from the before-set:", file=sys.stderr)
            for item in missing:
                print(f"  - {item}", file=sys.stderr)
            return 1
        print(
            f"audit ok refs={len(after['backtick_refs'])} links={len(after['md_links'])} "
            f"normative={len(after['normative'])}"
        )
        return 0

    parser.print_help()
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
