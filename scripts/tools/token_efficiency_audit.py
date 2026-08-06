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


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--extract", metavar="FILE")
    parser.add_argument("--diff", nargs=2, metavar=("BEFORE", "AFTER"))
    args = parser.parse_args()

    if args.extract:
        text = Path(args.extract).read_text(encoding="utf-8")
        print(json.dumps(extract(text), indent=2))
        return 0

    if args.diff:
        before = extract(Path(args.diff[0]).read_text(encoding="utf-8"))
        after = extract(Path(args.diff[1]).read_text(encoding="utf-8"))
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
