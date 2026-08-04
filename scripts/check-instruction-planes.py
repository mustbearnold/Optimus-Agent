#!/usr/bin/env python3
"""Fail closed when Optimus development and product instructions blur.

Current development law (owner directive, 2026-08-04): main-only development.
Zero linked worktrees, zero feature branches, enforced by `.githooks/`.
This gate pins that law so stale ceremony cannot silently return.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def read(root: Path, relative: str) -> str:
    path = root / relative
    if not path.is_file():
        return ""
    return path.read_text(encoding="utf-8")


def tracked_provider_files(root: Path) -> list[str]:
    """Provider state directories are fine on disk (git-excluded) but must
    never be tracked. Returns tracked paths under .claude/.hermes, if any."""
    try:
        result = subprocess.run(
            ["git", "-C", str(root), "ls-files", ".claude", ".hermes"],
            capture_output=True,
            text=True,
            timeout=30,
            check=False,
        )
    except (OSError, subprocess.SubprocessError):
        return []
    if result.returncode != 0:
        return []
    return [line for line in result.stdout.splitlines() if line.strip()]


def findings(root: Path = ROOT) -> list[str]:
    problems: list[str] = []
    agents = read(root, "AGENTS.md")
    runtime = read(root, "OPTIMUS_AGENTS.md")
    readme = read(root, "README.md")
    kernel = read(root, "crates/optimus-kernel/src/lib.rs")
    system_prompt = read(root, "crates/optimus-kernel/src/system_prompt.rs")
    justfile = read(root, "justfile")
    pre_commit = read(root, ".githooks/pre-commit")
    post_checkout = read(root, ".githooks/post-checkout")
    reference_transaction = read(root, ".githooks/reference-transaction")

    required: tuple[tuple[str, str, str], ...] = (
        ("AGENTS.md", agents, "Instruction-plane firewall"),
        ("AGENTS.md", agents, "A request about **how a coding agent should develop Optimus**"),
        ("AGENTS.md", agents, "Main-only development"),
        ("OPTIMUS_AGENTS.md", runtime, "Optimus Agent runtime constitution"),
        ("OPTIMUS_AGENTS.md", runtime, "Do not translate instructions for developers"),
        ("README.md", readme, "## Instruction authority"),
        ("README.md", readme, "Development requests are not product requirements"),
        ("crates/optimus-kernel/src/lib.rs", kernel, "OPTIMUS_AGENTS.md"),
        (
            "crates/optimus-kernel/src/system_prompt.rs",
            system_prompt,
            "Development repository AGENTS.md is not this constitution",
        ),
        (".githooks/pre-commit", pre_commit, "only allowed on 'main'"),
        (".githooks/post-checkout", post_checkout, "main-only"),
        (".githooks/reference-transaction", reference_transaction, "main-only"),
    )
    for relative, text, marker in required:
        if marker not in text:
            problems.append(f"{relative}: missing instruction-plane marker {marker!r}")

    forbidden: tuple[tuple[str, str, str], ...] = (
        ("AGENTS.md", agents, "/mnt/Projects/Optimus Agent"),
        # Anti-resurgence: the retired worktree/managed-delivery ceremony must
        # not creep back into live instructions.
        ("AGENTS.md", agents, "Development/worktrees"),
        ("justfile", justfile, "worktree-new"),
        ("justfile", justfile, "managed_delivery"),
        ("justfile", justfile, "assigned worktree"),
        ("justfile", justfile, "worktree-local"),
        ("README.md", readme, "just land"),
        ("README.md", readme, "just checkpoint"),
        ("README.md", readme, "assigned isolated worktrees"),
        ("README.md", readme, "python3 scripts/github_pr_branch.py"),
        ("README.md", readme, "gh issue"),
        ("AGENTS.md", agents, "owning issue"),
    )
    for relative, text, marker in forbidden:
        if marker in text:
            problems.append(f"{relative}: stale development instruction {marker!r}")

    include_lines = [
        line for line in kernel.splitlines() if "include_str!" in line or "/../../" in line
    ]
    if any("AGENTS.md" in line and "OPTIMUS_AGENTS.md" not in line for line in include_lines):
        problems.append("kernel: development AGENTS.md must not be embedded in product prompts")

    for tracked in tracked_provider_files(root):
        problems.append(
            f"{tracked}: provider-specific development state must not be tracked in the repository"
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
