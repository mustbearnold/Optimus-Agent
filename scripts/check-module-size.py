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

Size is measured in **production** lines. A file's own inline tests do not
count against it — counting them would penalise good coverage, which is the
opposite of what the rule is for. Two things follow, and both were learned the
hard way:

  * *Every* `#[cfg(test)]` item is skipped, wherever it sits, not just
    everything after the first one. Truncating at the first attribute measures
    "lines before the first test module", which is a different number that
    happens to agree most of the time. When it disagrees it lies: `lib.rs` once
    reported 912 while holding ~1200 lines of production code, because a test
    module two-thirds of the way down hid 280 lines of real code behind it. A
    gate on that number rewards moving a test module up rather than splitting
    the file.
  * Bare `mod x;` declarations do not count. They are a registry, not logic,
    and counting them taxes the exact behaviour the law is trying to cause:
    splitting a module adds one line to whichever file declares it, so a file
    sitting at its baseline could be pushed over the ratchet *by complying with
    the rule elsewhere*.

Braces are counted off source with comments and literal contents blanked out,
so a `{` inside a string or a doc comment cannot desynchronise the skip.

Exit 0 when clean; exit 1 with findings.

  python3 scripts/check-module-size.py            # gate
  python3 scripts/check-module-size.py --report   # ranked sizes, no gating
  python3 scripts/check-module-size.py --update   # rewrite baseline after a shrink
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASELINE = ROOT / "docs" / "architecture" / "module-size-baseline.json"
LIMIT = 800
SCAN_ROOTS = ("crates", "apps")

MEASURE = (
    "production lines: every #[cfg(test)] item and bare `mod x;` declaration "
    "excluded"
)

_IDENT = set("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_")

# `#[cfg(test)]`, and the `#[cfg(all(test, unix))]` family. Matched against a
# blanked line, so a `#[cfg(feature = "test")]` has already lost its `test` to
# the string blanking and cannot match here.
_CFG_TEST = re.compile(r"^#!?\[\s*cfg\s*\(.*\btest\b")
_MOD_DECL = re.compile(r"^(?:pub\s*(?:\([^)]*\)\s*)?)?mod\s+[A-Za-z_][A-Za-z0-9_]*\s*;$")


def _blank(text: str) -> str:
    """`text` with every character but the newlines replaced by a space."""
    return "".join("\n" if c == "\n" else " " for c in text)


def code_only(src: str) -> list[str]:
    """`src` as lines with comment and literal *contents* blanked out.

    Brace depth is what decides where a `#[cfg(test)]` item ends, and a brace
    inside a string or a comment is not a brace. Blanking rather than deleting
    keeps every line and every column where it was, so the result still lines
    up one-to-one with the file.
    """
    out: list[str] = []
    i, n = 0, len(src)
    block_depth = 0  # `/* */` nests in Rust.

    while i < n:
        c = src[i]

        if block_depth:
            if src.startswith("/*", i):
                block_depth += 1
                out.append("  ")
                i += 2
            elif src.startswith("*/", i):
                block_depth -= 1
                out.append("  ")
                i += 2
            else:
                out.append("\n" if c == "\n" else " ")
                i += 1
            continue

        if src.startswith("//", i):
            end = src.find("\n", i)
            end = n if end < 0 else end
            out.append(" " * (end - i))
            i = end
            continue

        if src.startswith("/*", i):
            block_depth = 1
            out.append("  ")
            i += 2
            continue

        # Raw strings, including the `b` byte-string prefix: r"…", r#"…"#, br#"…"#.
        # The preceding-character check keeps the `r` in `for` from starting one.
        if c in "rb" and (i == 0 or src[i - 1] not in _IDENT):
            j = i + 1 if c == "b" else i
            if j < n and src[j] == "r":
                j += 1
                hashes = 0
                while j < n and src[j] == "#":
                    hashes += 1
                    j += 1
                if j < n and src[j] == '"':
                    close = '"' + "#" * hashes
                    end = src.find(close, j + 1)
                    end = n if end < 0 else end + len(close)
                    out.append(_blank(src[i:end]))
                    i = end
                    continue

        if c == '"':
            j = i + 1
            while j < n:
                if src[j] == "\\":
                    j += 2
                    continue
                if src[j] == '"':
                    j += 1
                    break
                j += 1
            out.append(_blank(src[i:j]))
            i = j
            continue

        if c == "'":
            # A char literal closes; a lifetime (`&'a str`, `Foo<'_>`) does not.
            # Telling them apart matters: read as a literal, `'a` would swallow
            # the rest of the file looking for a quote that never comes.
            if i + 1 < n and src[i + 1] == "\\":
                j = i + 2
                while j < n and src[j] != "'":
                    j += 1
                out.append(_blank(src[i : min(j + 1, n)]))
                i = min(j + 1, n)
                continue
            if i + 2 < n and src[i + 2] == "'":
                out.append(_blank(src[i : i + 3]))
                i += 3
                continue
            out.append(" ")
            i += 1
            continue

        out.append(c)
        i += 1

    return "".join(out).splitlines()


def production_lines(path: Path) -> int:
    """Lines of production code: no `#[cfg(test)]` item, no `mod x;` line."""
    depth = 0
    counted = 0
    # Brace depth the skipped item started at, or None when not skipping.
    skip_from: int | None = None
    opened = False  # the skipped item has opened a block

    for line in code_only(path.read_text(encoding="utf-8", errors="replace")):
        stripped = line.strip()

        if skip_from is None and _CFG_TEST.match(stripped):
            skip_from, opened = depth, False

        if skip_from is None and not _MOD_DECL.match(stripped):
            counted += 1

        opens = line.count("{")
        depth += opens - line.count("}")

        if skip_from is not None:
            if depth > skip_from:
                opened = True
            elif opened or opens or stripped.endswith(";"):
                # The item's block closed — on an earlier line (`opened`) or on
                # this one (`opens`, for a `mod t {}` that never raised the
                # depth a later line could see) — or it never had a block and
                # this is the statement it applied to (`#[cfg(test)] use …;`).
                #
                # Without the `opens` case the skip never ends: depth never rose
                # above where it started, so every following line reads as still
                # inside the test item and the rest of the file goes uncounted.
                skip_from = None

    return counted


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
                "measure": MEASURE,
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
