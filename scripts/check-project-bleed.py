#!/usr/bin/env python3
"""Enforce the C1 bleed ratchet (docs/architecture/north-star-2026-07.md).

C1: five projects in one core, zero bleed; selection changes only via explicit
IPC call. The proof is crates/optimus-host/tests/project_bleed.rs, whose
BLEED_RATCHET constant fixes how many projects the test proves against each
other. This gate pins that constant and lets it move only toward 5: the pin
below must match the test exactly, so neither side can drift alone, and
lowering either is loud in review. Raising the ratchet is real engineering —
at pin time ratchet=2 fails because `sessions` has no project-scoped view.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TEST = ROOT / "crates" / "optimus-host" / "tests" / "project_bleed.rs"
TEST_NAME = "n_projects_in_one_core_zero_bleed"
TARGET = 5

# May only grow (C1 target: 5). Raise it in the same commit as the test's
# BLEED_RATCHET, with the engineering that makes the new N pass.
PINNED_RATCHET = 1


def main() -> int:
    errors: list[str] = []
    try:
        source = TEST.read_text(encoding="utf-8")
    except OSError as exc:
        print(f"ERROR: bleed test unreadable: {exc}", file=sys.stderr)
        return 1

    declarations = re.findall(r"(?m)^const BLEED_RATCHET: usize = (\d+);", source)
    if len(declarations) != 1:
        errors.append(
            "project_bleed.rs must declare exactly one "
            f"`const BLEED_RATCHET: usize = N;`, found {len(declarations)}"
        )
        ratchet = None
    else:
        ratchet = int(declarations[0])

    if f"fn {TEST_NAME}(" not in source:
        errors.append(f"bleed test function {TEST_NAME} is missing")
    if "#[ignore" in source:
        errors.append("bleed test may not be #[ignore]d — the ratchet must run")

    if ratchet is not None:
        if ratchet != PINNED_RATCHET:
            errors.append(
                f"ratchet drift: test declares {ratchet}, gate pins {PINNED_RATCHET} "
                "— move both in one commit, and only upward"
            )
        if ratchet > TARGET:
            errors.append(f"ratchet {ratchet} exceeds the C1 target {TARGET}")

    if errors:
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        return 1

    print(f"PROJECT_BLEED ratchet={PINNED_RATCHET}/{TARGET} projects (C1 target: {TARGET})")
    print("PROJECT_BLEED_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
