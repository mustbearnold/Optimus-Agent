#!/usr/bin/env python3
"""Regression tests for atomic managed remote-branch retirement."""

from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path

from test_managed_delivery import Fixture


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "managed_branch_retirement", ROOT / "scripts" / "managed_branch_retirement.py"
)
assert SPEC and SPEC.loader
RETIRE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = RETIRE
SPEC.loader.exec_module(RETIRE)


class ManagedBranchRetirementTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.fixture = Fixture(Path(self.temporary.name))
        fixture = self.fixture
        fixture.repo.git(["update-ref", "refs/heads/contained-fixture", fixture.initial])
        fixture.repo.git(
            ["push", "origin", "refs/heads/contained-fixture:refs/heads/contained-fixture"]
        )
        original = (fixture.worktree / "file.txt").read_text(encoding="utf-8")
        (fixture.worktree / "file.txt").write_text("unique branch\n", encoding="utf-8")
        tree = fixture.repo.snapshot_tree()
        (fixture.worktree / "file.txt").write_text(original, encoding="utf-8")
        fixture.unique = RETIRE.delivery.commit_tree(
            fixture.repo,
            tree,
            fixture.initial,
            "fixture unique branch\n",
            "2026-07-31T00:00:00+00:00",
        )
        fixture.repo.git(["update-ref", "refs/heads/unique-fixture", fixture.unique])
        fixture.repo.git(
            ["push", "origin", "refs/heads/unique-fixture:refs/heads/unique-fixture"]
        )

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def test_plan_refuses_unresolved_nonancestor(self) -> None:
        with self.assertRaisesRegex(RETIRE.delivery.Refusal, "neither contained"):
            RETIRE.plan(self.fixture.repo, {})

    def test_plan_classifies_contained_and_explicitly_superseded(self) -> None:
        payload, digest = RETIRE.plan(
            self.fixture.repo,
            {"unique-fixture": "superseded-by:test-fixture"},
        )
        dispositions = {
            item["branch"]: item["disposition"] for item in payload["retirements"]
        }
        self.assertEqual("contained-in-main", dispositions["contained-fixture"])
        self.assertEqual("verified-superseded", dispositions["unique-fixture"])
        self.assertRegex(digest, r"^[0-9a-f]{64}$")

    def test_execute_refuses_changed_plan_without_deleting_anything(self) -> None:
        superseded = {"unique-fixture": "superseded-by:test-fixture"}
        _payload, digest = RETIRE.plan(self.fixture.repo, superseded)
        self.fixture.repo.git(["update-ref", "refs/heads/late-fixture", self.fixture.initial])
        self.fixture.repo.git(
            ["push", "origin", "refs/heads/late-fixture:refs/heads/late-fixture"]
        )

        with self.assertRaisesRegex(RETIRE.delivery.Refusal, "plan changed"):
            RETIRE.execute(self.fixture.repo, digest, superseded)

        names = {branch.name for branch in RETIRE.remote_heads(self.fixture.repo)}
        self.assertIn("contained-fixture", names)
        self.assertIn("unique-fixture", names)
        self.assertIn("late-fixture", names)

    def test_execute_atomically_leaves_only_main_and_is_idempotent(self) -> None:
        superseded = {"unique-fixture": "superseded-by:test-fixture"}
        payload, digest = RETIRE.plan(self.fixture.repo, superseded)

        receipt = RETIRE.execute(self.fixture.repo, digest, superseded)

        self.assertEqual("retired", receipt["state"])
        self.assertEqual(len(payload["retirements"]), 2)
        self.assertEqual(["main"], [b.name for b in RETIRE.remote_heads(self.fixture.repo)])
        repeated = RETIRE.execute(self.fixture.repo, digest, superseded)
        self.assertEqual(digest, repeated["plan_sha256"])


if __name__ == "__main__":
    unittest.main(verbosity=2)
