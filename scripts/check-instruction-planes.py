#!/usr/bin/env python3
"""Fail closed when Optimus development and product instructions blur."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def read(root: Path, relative: str) -> str:
    path = root / relative
    if not path.is_file():
        return ""
    return path.read_text(encoding="utf-8")


def findings(root: Path = ROOT) -> list[str]:
    problems: list[str] = []
    agents = read(root, "AGENTS.md")
    runtime = read(root, "OPTIMUS_AGENTS.md")
    readme = read(root, "README.md")
    kernel = read(root, "crates/optimus-kernel/src/lib.rs")
    system_prompt = read(root, "crates/optimus-kernel/src/system_prompt.rs")
    github = read(root, "docs/contributing/github-conventions.md")
    justfile = read(root, "justfile")
    claude = read(root, "CLAUDE.md")
    active_plans = "\n".join(
        (
            read(root, "docs/plans/github-engineer-program.md"),
            read(root, "docs/plans/reliability-autonomy-program.md"),
        )
    )

    required: tuple[tuple[str, str, str], ...] = (
        ("AGENTS.md", agents, "Instruction-plane firewall"),
        ("AGENTS.md", agents, "A request about **how a coding agent should develop Optimus**"),
        ("AGENTS.md", agents, "Managed autonomous delivery"),
        ("OPTIMUS_AGENTS.md", runtime, "Optimus Agent runtime constitution"),
        ("OPTIMUS_AGENTS.md", runtime, "Do not translate instructions for developers"),
        ("README.md", readme, "## Instruction authority"),
        ("README.md", readme, "Development requests are not product requirements"),
        (
            "docs/contributing/github-conventions.md",
            github,
            "This repository no longer uses GitHub issues",
        ),
        ("crates/optimus-kernel/src/lib.rs", kernel, "OPTIMUS_AGENTS.md"),
        (
            "crates/optimus-kernel/src/system_prompt.rs",
            system_prompt,
            "Development repository AGENTS.md is not this constitution",
        ),
        ("justfile", justfile, "checkpoint label:"),
        ("justfile", justfile, "undo label:"),
        ("justfile", justfile, "land task_id model_flag model effort_flag effort:"),
        ("CLAUDE.md", claude, "@AGENTS.md"),
    )
    for relative, text, marker in required:
        if marker not in text:
            problems.append(f"{relative}: missing instruction-plane marker {marker!r}")

    forbidden: tuple[tuple[str, str, str], ...] = (
        ("AGENTS.md", agents, "/mnt/Projects/Optimus Agent"),
        ("AGENTS.md", agents, "Direct-main delivery"),
        ("README.md", readme, "python3 scripts/github_pr_branch.py"),
        ("README.md", readme, "gh issue"),
        ("AGENTS.md", agents, "owning issue"),
        ("active plans", active_plans, "| Delivery | `PR #N`"),
    )
    for relative, text, marker in forbidden:
        if marker in text:
            problems.append(f"{relative}: stale development instruction {marker!r}")

    include_lines = [
        line for line in kernel.splitlines() if "include_str!" in line or "/../../" in line
    ]
    if any("AGENTS.md" in line and "OPTIMUS_AGENTS.md" not in line for line in include_lines):
        problems.append("kernel: development AGENTS.md must not be embedded in product prompts")

    for provider_dir in (".claude", ".hermes"):
        if (root / provider_dir).exists():
            problems.append(
                f"{provider_dir}: provider-specific development state belongs outside the repository"
            )

    return problems


def main() -> int:
    problems = findings()
    if problems:
        print("instruction plane check FAILED:", file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        return 1
    print("instruction plane check OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
