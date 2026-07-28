#!/usr/bin/env python3
"""Offline self-test for check-tool-coverage.py's parse and evaluate core.

The gate's whole value is refusing drift between three sources it parses with
regexes; a silent regex miss would wave every future tool through. So the
parsers and the verdict are held here against known-good and known-broken
miniature sources.
"""

from __future__ import annotations

import importlib.util
import sys
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location(
    "check_tool_coverage", SCRIPTS / "check-tool-coverage.py"
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

    # A matched world has only the pin complaints (the miniature registry is
    # smaller than the real pins) and nothing about names.
    verdict = gate.evaluate(dispatchable, unavailable, exercised, refused)
    if any("has no entry" in line or "stale" in line for line in verdict):
        failures.append(f"a matched ledger must raise no name failures: {verdict}")

    # A dispatchable tool missing from the ledger must be named.
    verdict = gate.evaluate(dispatchable | {"new_tool"}, unavailable, exercised, refused)
    if not any("new_tool" in line and "DISPATCHABLE_EXERCISED" in line for line in verdict):
        failures.append("an untested new tool must be named in the failure")

    # A ledger entry the registry no longer dispatches must be named stale.
    verdict = gate.evaluate(dispatchable, unavailable, exercised | {"ghost"}, refused)
    if not any("ghost" in line for line in verdict):
        failures.append("a ghost ledger entry must be named")

    # A scaffold without a refusal check must be named.
    verdict = gate.evaluate(dispatchable, unavailable | {"new_scaffold"}, exercised, refused)
    if not any("new_scaffold" in line and "UNAVAILABLE_REFUSED" in line for line in verdict):
        failures.append("an unchecked new scaffold must be named")

    # An empty parse is a parser failure, never a green.
    verdict = gate.evaluate(set(), set(), set(), set())
    if not any("parser" in line for line in verdict):
        failures.append("empty parses must fail loudly, not pass quietly")

    for failure in failures:
        print(f"ERROR: {failure}", file=sys.stderr)
    if not failures:
        print("TOOL_COVERAGE_GATE_SELFTEST_OK")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
