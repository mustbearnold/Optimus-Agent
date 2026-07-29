#!/usr/bin/env python3
"""Fail closed when the sole Codex delivery contract drifts (#120)."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

REQUIRED_MARKERS: dict[str, tuple[str, ...]] = {
    "AGENTS.md": (
        "## Repository map (mandatory orientation)",
        "## Sole Codex delivery workflow (mandatory)",
        "Focused issue → Codex plan → isolated worktree → tested change → draft PR",
        "`caveman-optimus`",
        "`@codex review`",
        "`gh pr merge --auto --merge`",
        "## Codex review rules (mandatory)",
    ),
    "docs/contributing/github-conventions.md": (
        "## Codex issue-to-auto-merge loop",
        "### Repository enforcement (confirmed 2026-07-29)",
        "local/worktrees/<slug>",
        "`@codex review`",
        "`gh pr merge --auto --merge`",
        "`just verify (Linux)`",
        "GitHub deletes the remote head automatically",
    ),
    "docs/agents/issue-tracker.md": (
        "## Required issue contract",
        "One Codex task owns that issue",
        "`caveman-optimus`",
        "`gh pr merge --auto --merge`",
        "Monitor to a terminal outcome",
    ),
    ".github/pull_request_template.md": (
        "## Focused issue",
        "## Review and merge automation",
        "`@codex review`",
        "`just verify (Linux)`",
        "`gh pr merge --auto --merge`",
    ),
    ".github/ISSUE_TEMPLATE/config.yml": ("blank_issues_enabled: false",),
    "scripts/verify.sh": (
        'spawn "codex-workflow"',
        'spawn "test_codex_workflow"',
        'spawn "test_pre_push_hook"',
    ),
}

ISSUE_FORMS = (
    ".github/ISSUE_TEMPLATE/architecture.yml",
    ".github/ISSUE_TEMPLATE/bug.yml",
    ".github/ISSUE_TEMPLATE/feature.yml",
    ".github/ISSUE_TEMPLATE/task.yml",
)
ISSUE_FIELDS = ("goal", "context", "constraints", "done_when")

FORBIDDEN_MARKERS: dict[str, tuple[str, ...]] = {
    "AGENTS.md": (
        "One session → one PR → merged that session",
        "human-controlled merge",
    ),
    "docs/contributing/github-conventions.md": (
        "git checkout main && git pull",
        "human-controlled merge",
    ),
    ".github/ISSUE_TEMPLATE/config.yml": ("blank_issues_enabled: true",),
}


def validate(root: Path) -> list[str]:
    errors: list[str] = []

    for relative, markers in REQUIRED_MARKERS.items():
        path = root / relative
        if not path.is_file():
            errors.append(f"missing required workflow file: {relative}")
            continue
        text = path.read_text(encoding="utf-8")
        for marker in markers:
            if marker not in text:
                errors.append(f"{relative}: missing workflow marker {marker!r}")

    for relative in ISSUE_FORMS:
        path = root / relative
        if not path.is_file():
            errors.append(f"missing required issue form: {relative}")
            continue
        text = path.read_text(encoding="utf-8")
        for field in ISSUE_FIELDS:
            if not re.search(rf"^\s+id:\s+{re.escape(field)}\s*$", text, re.MULTILINE):
                errors.append(f"{relative}: missing required field id {field!r}")

    for relative, markers in FORBIDDEN_MARKERS.items():
        path = root / relative
        if not path.is_file():
            continue
        text = path.read_text(encoding="utf-8")
        for marker in markers:
            if marker in text:
                errors.append(f"{relative}: forbidden legacy workflow marker {marker!r}")

    return errors


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT)
    args = parser.parse_args(argv)

    errors = validate(args.root.resolve())
    if errors:
        print("CODEX_WORKFLOW_FAIL", file=sys.stderr)
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        return 1

    print("CODEX_WORKFLOW_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
