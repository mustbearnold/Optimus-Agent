#!/usr/bin/env python3
"""Regression tests for the repo token-budget ratchet gate."""

from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent.parent / "gates" / "check-repo-token-budget.py"
SPEC = importlib.util.spec_from_file_location("repo_token_budget", SCRIPT)
assert SPEC and SPEC.loader
token_budget = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(token_budget)


def write_surface(root: Path, rel: str, size: int) -> None:
    path = root / rel
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("x" * size, encoding="utf-8")


def write_baseline(root: Path, budgets: dict[str, int]) -> None:
    path = root / "docs" / "architecture" / "token-budget-baseline.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(
            {
                "comment": "test baseline",
                "measure": "bytes",
                "surfaces": {key: {"budget_bytes": b} for key, b in budgets.items()},
            },
            indent=2,
        ),
        encoding="utf-8",
    )


class RepoTokenBudgetTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        # A tree that fits every budget: AGENTS 5,000 <= 6,500; CONTEXT 1,000
        # <= 2,000; architecture 40,000 <= 45,000; in-repo skills 8,000 <=
        # 12,000. No profile skill under HOME -> optional surface skipped.
        write_surface(self.root, "AGENTS.md", 5_000)
        write_surface(self.root, "CONTEXT.md", 1_000)
        write_surface(self.root, "docs/architecture.md", 40_000)
        write_surface(self.root, "skills/optimus-native-ui-testing/SKILL.md", 6_000)
        write_surface(self.root, "skills/update-engineering-memory/SKILL.md", 2_000)
        write_baseline(
            self.root,
            {
                "AGENTS.md": 6_500,
                "CONTEXT.md": 2_000,
                "docs/architecture.md": 45_000,
                "skills/ (in-repo)": 12_000,
                "skill:optimus-agent-development/SKILL.md": 84_904,
            },
        )

    def tearDown(self) -> None:
        self.temp.cleanup()

    def test_accepts_tree_within_budget(self) -> None:
        self.assertEqual(token_budget.findings(self.root, Path(self.temp.name)), [])

    def test_rejects_surface_over_budget(self) -> None:
        write_surface(self.root, "AGENTS.md", 7_000)
        problems = token_budget.findings(self.root, Path(self.temp.name))
        self.assertTrue(any("AGENTS.md" in item and "7000 B" in item for item in problems))

    def test_rejects_missing_required_surface(self) -> None:
        (self.root / "CONTEXT.md").unlink()
        problems = token_budget.findings(self.root, Path(self.temp.name))
        self.assertTrue(any("CONTEXT.md" in item and "missing" in item for item in problems))

    def test_missing_budget_for_known_surface(self) -> None:
        write_baseline(self.root, {"AGENTS.md": 6_500})
        problems = token_budget.findings(self.root, Path(self.temp.name))
        self.assertTrue(any("no budget" in item for item in problems))

    def test_optional_profile_skill_skipped_when_absent(self) -> None:
        # HOME points at the empty temp dir -> no profile skill -> no finding.
        self.assertEqual(token_budget.findings(self.root, Path(self.temp.name)), [])

    def test_optional_profile_skill_enforced_when_present(self) -> None:
        skill = (
            Path(self.temp.name)
            / ".hermes"
            / "profiles"
            / "p1"
            / "skills"
            / "software-development"
            / "optimus-agent-development"
            / "SKILL.md"
        )
        skill.parent.mkdir(parents=True)
        skill.write_text("x" * 90_000, encoding="utf-8")
        problems = token_budget.findings(self.root, Path(self.temp.name))
        self.assertTrue(
            any("skill:optimus-agent-development" in item and "90000 B" in item for item in problems)
        )
        # Shrink under the budget -> clean.
        skill.write_text("x" * 80_000, encoding="utf-8")
        self.assertEqual(token_budget.findings(self.root, Path(self.temp.name)), [])

    def test_update_carries_optional_budget_forward_when_absent(self) -> None:
        token_budget.write_baseline(self.root, token_budget.measure(self.root, Path(self.temp.name)))
        payload = json.loads(
            (self.root / "docs" / "architecture" / "token-budget-baseline.json").read_text(
                encoding="utf-8"
            )
        )
        # Optional surface absent at update time keeps its previous budget.
        self.assertIn("skill:optimus-agent-development/SKILL.md", payload["surfaces"])


if __name__ == "__main__":
    unittest.main()
