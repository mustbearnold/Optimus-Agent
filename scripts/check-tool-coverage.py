#!/usr/bin/env python3
"""Tool-coverage ratchet: no tool ships without an executed test.

The registry (crates/optimus-packs) classifies every tool as dispatchable or
a declared scaffold. The coverage suite (crates/optimus-kernel/tests/
tool_coverage.rs) holds a ledger of both: dispatchable tools it EXECUTES
through a real `Kernel::turn`, and scaffolds it holds to the typed-refusal
contract. This gate diffs the ledger against the registry source, so:

  * a new dispatchable tool that lands without joining the ledger (and so
    without a real dispatch test) turns the gates red;
  * a new scaffold that lands without a refusal check turns the gates red;
  * the pinned counts move only together with the ledger, in one commit —
    the counter cannot drift backwards silently.

The suite itself re-asserts the same closure at runtime against
`builtin_catalog()`; this script is the fast static face of the same law, so
drift is named in the gates tier (~1s) rather than minutes later in tests.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
INVOCATION_RS = ROOT / "crates" / "optimus-packs" / "src" / "invocation.rs"
PACKS_LIB_RS = ROOT / "crates" / "optimus-packs" / "src" / "lib.rs"
COVERAGE_RS = ROOT / "crates" / "optimus-kernel" / "tests" / "tool_coverage.rs"

# Move these only together with the coverage ledger, in the same commit.
PINNED_DISPATCHABLE = 17
PINNED_UNAVAILABLE = 14


def parse_dispatchable(invocation_source: str) -> set[str]:
    """Canonical ids from `fn id`: every `Some("name")` arm is dispatchable."""
    return set(re.findall(r'=>\s*Some\("([a-z_]+)"\)', invocation_source))


def parse_unavailable(packs_source: str) -> set[str]:
    """Every `unavailable("name", …)` entry in the builtin catalog."""
    return set(re.findall(r'unavailable\(\s*"([a-z_]+)"', packs_source))


def parse_ledger(coverage_source: str, const_name: str) -> set[str]:
    """String entries of one `const NAME: [&str; N] = [ … ];` ledger."""
    match = re.search(
        rf"const {const_name}[^=]*=\s*\[(.*?)\];",
        coverage_source,
        re.DOTALL,
    )
    if match is None:
        return set()
    return set(re.findall(r'"([a-z_]+)"', match.group(1)))


def evaluate(
    dispatchable: set[str],
    unavailable: set[str],
    exercised: set[str],
    refused: set[str],
) -> list[str]:
    failures: list[str] = []
    for name in sorted(dispatchable - exercised):
        failures.append(
            f"dispatchable tool {name!r} has no entry in DISPATCHABLE_EXERCISED — "
            "give it a real dispatch test in tests/tool_coverage.rs"
        )
    for name in sorted(exercised - dispatchable):
        failures.append(
            f"ledger names {name!r} but the registry does not dispatch it — "
            "remove it or implement the tool"
        )
    for name in sorted(unavailable - refused):
        failures.append(
            f"scaffold tool {name!r} has no entry in UNAVAILABLE_REFUSED — "
            "hold it to the typed-refusal contract in tests/tool_coverage.rs"
        )
    for name in sorted(refused - unavailable):
        failures.append(
            f"ledger refuses {name!r} but the registry does not declare it — "
            "remove the stale entry"
        )
    if len(dispatchable) != PINNED_DISPATCHABLE:
        failures.append(
            f"registry dispatches {len(dispatchable)} tools but the pin says "
            f"{PINNED_DISPATCHABLE} — move PINNED_DISPATCHABLE together with the "
            "ledger and its tests, in this commit"
        )
    if len(unavailable) != PINNED_UNAVAILABLE:
        failures.append(
            f"registry declares {len(unavailable)} scaffolds but the pin says "
            f"{PINNED_UNAVAILABLE} — move PINNED_UNAVAILABLE together with the "
            "ledger, in this commit"
        )
    if not dispatchable:
        failures.append("parsed zero dispatchable tools — the parser lost the source")
    if not unavailable:
        failures.append("parsed zero scaffolds — the parser lost the source")
    return failures


def main() -> int:
    dispatchable = parse_dispatchable(INVOCATION_RS.read_text())
    unavailable = parse_unavailable(PACKS_LIB_RS.read_text())
    coverage_source = COVERAGE_RS.read_text() if COVERAGE_RS.exists() else ""
    exercised = parse_ledger(coverage_source, "DISPATCHABLE_EXERCISED")
    refused = parse_ledger(coverage_source, "UNAVAILABLE_REFUSED")

    failures = evaluate(dispatchable, unavailable, exercised, refused)
    for failure in failures:
        print(f"ERROR: {failure}", file=sys.stderr)
    if failures:
        return 1
    print(
        f"tool-coverage: {len(dispatchable)} dispatchable exercised, "
        f"{len(unavailable)} scaffolds refused, "
        f"{len(dispatchable) + len(unavailable)} classified — OK"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
