#!/usr/bin/env python3
"""Fail-closed dependency layer rules for Optimus control-plane peels (P11).

Rules:
  - optimus-eval may depend on optimus-kernel; kernel must not depend on eval
  - optimus-ops must not depend on optimus-kernel
  - optimus-agent must not depend on optimus-workflow, optimus-kernel, or eval
  - optimus-workflow may depend on optimus-agent + optimus-artifacts
  - optimus-artifacts must not depend on kernel/agent/workflow/eval
  - optimus-browser must not depend on optimus-kernel
  - no peeled crate may depend on apps/*
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CRATES = ROOT / "crates"


def deps_of(crate: str) -> set[str]:
    cargo = CRATES / crate / "Cargo.toml"
    if not cargo.is_file():
        raise SystemExit(f"missing {cargo}")
    text = cargo.read_text(encoding="utf-8")
    found: set[str] = set()
    for match in re.finditer(r"^(optimus-[\w-]+)(?:\.workspace)?\s*=", text, re.M):
        found.add(match.group(1))
    for match in re.finditer(r'path\s*=\s*"\.\./\.\./crates/(optimus-[\w-]+)"', text):
        found.add(match.group(1))
    for match in re.finditer(r'path\s*=\s*"\.\./(optimus-[\w-]+)"', text):
        found.add(match.group(1))
    return found


def main() -> int:
    errors: list[str] = []

    def forbid(crate: str, banned: set[str]) -> None:
        have = deps_of(crate)
        bad = have & banned
        if bad:
            errors.append(f"{crate} must not depend on {sorted(bad)}; has {sorted(have)}")

    forbid("optimus-kernel", {"optimus-eval"})
    forbid("optimus-ops", {"optimus-kernel", "optimus-eval", "optimus-agent", "optimus-workflow"})
    forbid(
        "optimus-agent",
        {
            "optimus-kernel",
            "optimus-eval",
            "optimus-workflow",
            "optimus-ops",
            "optimus-artifacts",
        },
    )
    forbid(
        "optimus-artifacts",
        {
            "optimus-kernel",
            "optimus-eval",
            "optimus-agent",
            "optimus-workflow",
            "optimus-ops",
            "optimus-runtime",
            "optimus-graph",
        },
    )
    forbid(
        "optimus-workflow",
        {"optimus-kernel", "optimus-eval", "optimus-ops"},
    )
    forbid(
        "optimus-browser",
        {
            "optimus-kernel",
            "optimus-eval",
            "optimus-agent",
            "optimus-workflow",
            "optimus-artifacts",
            "optimus-ops",
        },
    )
    forbid("optimus-eval", {"optimus-ops"})  # eval may use kernel only among control peels

    # Required edges that define the peel graph
    agent_deps = deps_of("optimus-agent")
    if "optimus-runtime" not in agent_deps or "optimus-packs" not in agent_deps:
        errors.append("optimus-agent must depend on optimus-runtime and optimus-packs")
    workflow_deps = deps_of("optimus-workflow")
    for need in ("optimus-agent", "optimus-artifacts", "optimus-runtime"):
        if need not in workflow_deps:
            errors.append(f"optimus-workflow must depend on {need}")
    kernel_deps = deps_of("optimus-kernel")
    for need in (
        "optimus-agent",
        "optimus-workflow",
        "optimus-artifacts",
        "optimus-ops",
        "optimus-runtime",
    ):
        if need not in kernel_deps:
            errors.append(f"optimus-kernel must depend on {need}")

    if errors:
        print("CRATE_LAYER_FAIL")
        for err in errors:
            print(f"  - {err}")
        return 1
    print("CRATE_LAYER_OK")
    print(f"  optimus-agent -> {sorted(deps_of('optimus-agent'))}")
    print(f"  optimus-workflow -> {sorted(deps_of('optimus-workflow'))}")
    print(f"  optimus-artifacts -> {sorted(deps_of('optimus-artifacts'))}")
    print(f"  optimus-kernel -> {sorted(deps_of('optimus-kernel'))}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
