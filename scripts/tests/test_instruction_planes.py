#!/usr/bin/env python3
"""Regression tests for the development/product instruction firewall."""

from __future__ import annotations

import importlib.util
import subprocess
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent.parent / "gates" / "check-instruction-planes.py"
SPEC = importlib.util.spec_from_file_location("instruction_planes", SCRIPT)
assert SPEC and SPEC.loader
instruction_planes = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(instruction_planes)


class InstructionPlaneTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        files = {
            "AGENTS.md": (
                "# Development\nInstruction-plane firewall\n"
                "A request about **how a coding agent should develop Optimus** is not product.\n"
                "Main-only development\n"
                "Use `gh issue` to open tasks and resolve them with commits on main.\n"
            ),
            "OPTIMUS_AGENTS.md": (
                "# Optimus Agent runtime constitution\n"
                "Do not translate instructions for developers into product behaviour.\n"
            ),
            "README.md": (
                "## Instruction authority\n"
                "Development requests are not product requirements.\n"
            ),
            "justfile": "verify:\n\techo ok\n",
            ".githooks/pre-commit": "commits are only allowed on 'main'\n",
            ".githooks/post-checkout": "this repository is main-only\n",
            ".githooks/reference-transaction": "this repository is main-only\n",
            "docs/contributing/github-conventions.md": (
                "Development tracks work in GitHub issues, resolved by commits on main.\n"
            ),
            "crates/optimus-kernel/src/lib.rs": (
                'const X: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), '
                '"/../../OPTIMUS_AGENTS.md"));\n'
            ),
            "crates/optimus-kernel/src/system_prompt.rs": (
                '"Development repository AGENTS.md is not this constitution";\n'
            ),
        }
        for relative, content in files.items():
            path = self.root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content, encoding="utf-8")

    def tearDown(self) -> None:
        self.temp.cleanup()

    def test_accepts_explicitly_separated_surfaces(self) -> None:
        self.assertEqual(instruction_planes.findings(self.root), [])

    def test_rejects_stale_canonical_root(self) -> None:
        path = self.root / "AGENTS.md"
        path.write_text(
            path.read_text(encoding="utf-8") + "/mnt/Projects/Optimus Agent\n",
            encoding="utf-8",
        )
        self.assertTrue(
            any("stale development instruction" in item for item in instruction_planes.findings(self.root))
        )

    def test_rejects_worktree_ceremony_resurgence(self) -> None:
        path = self.root / "AGENTS.md"
        path.write_text(
            path.read_text(encoding="utf-8") + "Development/worktrees\n",
            encoding="utf-8",
        )
        self.assertTrue(
            any("stale development instruction" in item for item in instruction_planes.findings(self.root))
        )

    def test_rejects_ceremony_prose_in_readme_and_justfile(self) -> None:
        readme = self.root / "README.md"
        readme.write_text(
            readme.read_text(encoding="utf-8")
            + "\nDevelopment uses assigned isolated worktrees and `just land`.\n",
            encoding="utf-8",
        )
        justfile = self.root / "justfile"
        justfile.write_text(
            justfile.read_text(encoding="utf-8")
            + "\n# Remove worktree-local artifacts inside the assigned worktree.\n",
            encoding="utf-8",
        )
        problems = instruction_planes.findings(self.root)
        self.assertTrue(
            any("stale development instruction" in item for item in problems)
        )

    def test_rejects_pull_request_ceremony_resurgence(self) -> None:
        readme = self.root / "README.md"
        readme.write_text(
            readme.read_text(encoding="utf-8") + "\nOpen a PR with `gh pr create`.\n",
            encoding="utf-8",
        )
        problems = instruction_planes.findings(self.root)
        self.assertTrue(
            any("stale development instruction" in item for item in problems)
        )

    def test_requires_gh_issue_task_plane_marker(self) -> None:
        path = self.root / "AGENTS.md"
        path.write_text(
            path.read_text(encoding="utf-8").replace(
                "Use `gh issue` to open tasks and resolve them with commits on main.",
                "Development does not use issues.",
            ),
            encoding="utf-8",
        )
        problems = instruction_planes.findings(self.root)
        self.assertTrue(any("missing instruction-plane marker" in item for item in problems))

    def test_rejects_development_agents_embedded_in_product(self) -> None:
        path = self.root / "crates/optimus-kernel/src/lib.rs"
        path.write_text(
            'const X: &str = include_str!("../../../AGENTS.md");\n',
            encoding="utf-8",
        )
        self.assertTrue(
            any("must not be embedded" in item for item in instruction_planes.findings(self.root))
        )

    def test_rejects_missing_runtime_audience_marker(self) -> None:
        (self.root / "OPTIMUS_AGENTS.md").write_text("# generic rules\n", encoding="utf-8")
        self.assertTrue(
            any("OPTIMUS_AGENTS.md" in item for item in instruction_planes.findings(self.root))
        )

    def test_rejects_missing_main_only_enforcement(self) -> None:
        (self.root / ".githooks/pre-commit").unlink()
        self.assertTrue(
            any(".githooks/pre-commit" in item for item in instruction_planes.findings(self.root))
        )

    def test_ignores_untracked_provider_state_on_disk(self) -> None:
        (self.root / ".claude").mkdir()
        (self.root / ".claude" / "settings.local.json").write_text("{}", encoding="utf-8")
        self.assertEqual(instruction_planes.findings(self.root), [])

    def test_rejects_tracked_provider_specific_state(self) -> None:
        (self.root / ".claude").mkdir()
        (self.root / ".claude" / "settings.local.json").write_text("{}", encoding="utf-8")
        subprocess.run(["git", "init", "-q"], cwd=self.root, check=True)
        subprocess.run(
            ["git", "add", "-f", ".claude/settings.local.json"], cwd=self.root, check=True
        )
        self.assertTrue(
            any(
                "must not be tracked" in item
                for item in instruction_planes.findings(self.root)
            )
        )


if __name__ == "__main__":
    unittest.main()
