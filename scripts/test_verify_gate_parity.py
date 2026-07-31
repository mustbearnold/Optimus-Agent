#!/usr/bin/env python3
"""Prove optimized `all` cannot silently omit gates from the `gates` tier."""

from __future__ import annotations

import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
VERIFY = ROOT / "scripts" / "verify.sh"


def between(text: str, start: str, end: str) -> str:
    return text.split(start, 1)[1].split(end, 1)[0]


def spawned_names(text: str) -> set[str]:
    return set(re.findall(r'^\s*spawn\s+"([^"]+)"', text, re.MULTILINE))


def main() -> int:
    text = VERIFY.read_text(encoding="utf-8")
    gates_function = between(text, "tier_gates() {", "# --- tier: check")
    all_function = between(text, "tier_all() {", "# --- reporting")

    gates_static = spawned_names(between(gates_function, 'spawn_section "gates"', 'spawn_section "gate self-tests"'))
    all_static = spawned_names(between(all_function, 'spawn_section "gates"', 'spawn_section "gate self-tests"'))
    gates_tests = spawned_names(between(gates_function, 'spawn_section "gate self-tests"', "  reap"))
    all_tests = spawned_names(between(all_function, 'spawn_section "gate self-tests"', 'spawn_section "compile"'))

    problems = []
    if gates_static != all_static:
        problems.append(
            f"static gate mismatch: only_gates={sorted(gates_static - all_static)} "
            f"only_all={sorted(all_static - gates_static)}"
        )
    if gates_tests != all_tests:
        problems.append(
            f"self-test mismatch: only_gates={sorted(gates_tests - all_tests)} "
            f"only_all={sorted(all_tests - gates_tests)}"
        )
    if problems:
        print("VERIFY_GATE_PARITY_FAILED")
        print("\n".join(problems))
        return 1
    print(
        f"VERIFY_GATE_PARITY_OK static={len(gates_static)} self_tests={len(gates_tests)}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
