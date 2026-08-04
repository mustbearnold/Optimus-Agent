#!/usr/bin/env python3
"""Offline self-test for check-tool-coverage.py's parse and evaluate core.

The gate's whole value is refusing drift between three sources it parses with
regexes; a silent regex miss would wave every future tool through. So the
parsers and the verdict are held here against known-good and known-broken
miniature sources.

The scaffold ceiling is held to a fourth property the other checks cannot see:
it is one-way. A count pin reads the same whether the registry shrank or grew,
so the direction has to be asserted on its own.
"""

from __future__ import annotations

import importlib.util
import sys
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location(
    "check_tool_coverage", SCRIPTS / "gates" / "check-tool-coverage.py"
)
gate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gate)

INVOCATION = """
    pub(crate) fn id(self) -> Option<&'static str> {
        match self {
            Self::ReadFile => Some("read_file"),
            Self::Terminal => Some("terminal"),
            Self::Unavailable => None,
        }
    }
"""

PACKS = """
    unavailable("clarify", "Ask the user", ToolPolicy::UserInteraction),
    unavailable(
        "home_device_status",
        "Read home device status",
        ToolPolicy::NetworkRead,
    ),
"""

LEDGER = """
const DISPATCHABLE_EXERCISED: [&str; 2] = [
    "read_file",
    "terminal",
];
const UNAVAILABLE_REFUSED: [&str; 2] = [
    "clarify",
    "home_device_status",
];
"""

# The real ceiling names the real registry; this miniature world gets its own so
# the fixtures stay small and the ratchet is still exercised on its own terms.
CEILING = frozenset({"clarify", "home_device_status"})


def main() -> int:
    failures: list[str] = []

    dispatchable = gate.parse_dispatchable(INVOCATION)
    if dispatchable != {"read_file", "terminal"}:
        failures.append(f"dispatchable parse drifted: {dispatchable}")

    unavailable = gate.parse_unavailable(PACKS)
    if unavailable != {"clarify", "home_device_status"}:
        failures.append(
            f"unavailable parse must catch multi-line entries: {unavailable}"
        )

    exercised = gate.parse_ledger(LEDGER, "DISPATCHABLE_EXERCISED")
    refused = gate.parse_ledger(LEDGER, "UNAVAILABLE_REFUSED")
    if exercised != {"read_file", "terminal"} or refused != {
        "clarify",
        "home_device_status",
    }:
        failures.append(f"ledger parse drifted: {exercised} / {refused}")

    # A matched world has only the dispatchable-pin complaint (the miniature
    # registry is smaller than the real pin) and nothing about names.
    verdict = gate.evaluate(dispatchable, unavailable, exercised, refused, CEILING)
    if any("has no entry" in line or "stale" in line for line in verdict):
        failures.append(f"a matched ledger must raise no name failures: {verdict}")

    # A dispatchable tool missing from the ledger must be named.
    verdict = gate.evaluate(dispatchable | {"new_tool"}, unavailable, exercised, refused, CEILING)
    if not any("new_tool" in line and "DISPATCHABLE_EXERCISED" in line for line in verdict):
        failures.append("an untested new tool must be named in the failure")

    # A ledger entry the registry no longer dispatches must be named stale.
    verdict = gate.evaluate(dispatchable, unavailable, exercised | {"ghost"}, refused, CEILING)
    if not any("ghost" in line for line in verdict):
        failures.append("a ghost ledger entry must be named")

    # A scaffold without a refusal check must be named.
    verdict = gate.evaluate(dispatchable, unavailable | {"new_scaffold"}, exercised, refused, CEILING)
    if not any("new_scaffold" in line and "UNAVAILABLE_REFUSED" in line for line in verdict):
        failures.append("an unchecked new scaffold must be named")

    # A *correctly ledgered* new scaffold must still be refused. This is what the
    # count pin waved through: `len(unavailable) != PINNED_UNAVAILABLE` cannot
    # tell four-because-one-shipped from four-because-one-shipped-and-another-
    # was-added, so a new refusing row was legal as long as the same commit
    # bumped an integer. ADR-0068 §1 says a catalog row exists only when its tool
    # dispatches, and the ratchet is where that becomes enforceable.
    verdict = gate.evaluate(
        dispatchable,
        unavailable | {"new_scaffold"},
        exercised,
        refused | {"new_scaffold"},
        CEILING,
    )
    if not any("new_scaffold" in line and "only shrink" in line for line in verdict):
        failures.append("a scaffold above the ceiling must be refused, not merely counted")

    # Shipping a scaffold for real is the ratchet counting down: every other
    # source agrees, and the one remaining act is deleting its ceiling line.
    verdict = gate.evaluate(
        dispatchable | {"clarify"},
        unavailable - {"clarify"},
        exercised | {"clarify"},
        refused - {"clarify"},
        CEILING,
    )
    if not any("clarify" in line and "stale pin" in line for line in verdict):
        failures.append("a shipped scaffold must be reported as a stale ceiling entry")
    if any("only shrink" in line for line in verdict):
        failures.append("shrinking the scaffold set must never be reported as growth")

    # An empty parse is a parser failure, never a green.
    verdict = gate.evaluate(set(), set(), set(), set(), frozenset())
    if not any("parser" in line for line in verdict):
        failures.append("empty parses must fail loudly, not pass quietly")

    for failure in failures:
        print(f"ERROR: {failure}", file=sys.stderr)
    if not failures:
        print("TOOL_COVERAGE_GATE_SELFTEST_OK")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
