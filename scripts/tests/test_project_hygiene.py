#!/usr/bin/env python3
"""Regression tests for worktree-local project hygiene."""

from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

SCRIPT = Path(__file__).resolve().parent.parent / "tools" / "project_hygiene.py"
SPEC = importlib.util.spec_from_file_location("project_hygiene", SCRIPT)
assert SPEC and SPEC.loader
project_hygiene = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = project_hygiene
SPEC.loader.exec_module(project_hygiene)


class ProjectHygieneTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        base = Path(self.temp.name)
        self.repository = base / "repo"
        self.root = self.repository / "local/worktrees/task"
        self.git_dir = self.repository / ".git/worktrees/task"
        self.common = self.repository / ".git"
        self.root.mkdir(parents=True)
        self.git_dir.mkdir(parents=True)
        (self.root / ".git").write_text(f"gitdir: {self.git_dir}\n", encoding="utf-8")
        (self.git_dir / "commondir").write_text("../..\n", encoding="utf-8")
        (self.git_dir / "gitdir").write_text(f"{self.root / '.git'}\n", encoding="utf-8")
        self.worktree = project_hygiene.resolve_worktree(self.root)

    def tearDown(self) -> None:
        self.temp.cleanup()

    def write(self, relative: str, size: int = 8) -> Path:
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(b"x" * size)
        return path

    def git_result(self, tracked: str = "") -> mock.Mock:
        return mock.Mock(returncode=0, stdout=tracked, stderr="")

    @mock.patch.object(project_hygiene, "git")
    def test_report_is_read_only(self, git_mock: mock.Mock) -> None:
        git_mock.side_effect = [self.git_result(), self.git_result()]
        artifact = self.write("target/debug/object", 11)
        total, lines = project_hygiene.report(self.worktree)
        self.assertEqual(total, 11)
        self.assertTrue(artifact.exists())
        self.assertIn("OUTCOME completed", lines)

    @mock.patch.object(project_hygiene, "git")
    def test_clean_deletes_only_closed_manifest(self, git_mock: mock.Mock) -> None:
        git_mock.side_effect = [
            self.git_result(),
            self.git_result(),
            self.git_result(),
            self.git_result(),
        ]
        target = self.write("target/debug/object")
        modules = self.write("apps/optimus-ui/node_modules/pkg/index.js")
        source = self.write("apps/optimus-ui/src/app.ts")
        evidence = self.write("local/tmp/cua-evidence/ledger.json")
        _, lines = project_hygiene.clean(self.worktree)
        self.assertFalse(target.exists())
        self.assertFalse(modules.exists())
        self.assertTrue(source.exists())
        self.assertTrue(evidence.exists())
        self.assertEqual(lines[-1], "OUTCOME completed")

    def test_rejects_bare_root_invocation(self) -> None:
        with self.assertRaises(project_hygiene.HygieneError):
            project_hygiene.resolve_worktree(self.repository)

    def test_rejects_gitfile_without_matching_backpointer(self) -> None:
        (self.git_dir / "gitdir").write_text("/different/.git\n", encoding="utf-8")
        with self.assertRaises(project_hygiene.HygieneError):
            project_hygiene.resolve_worktree(self.root)

    @mock.patch.object(project_hygiene, "git")
    def test_rejects_symlinked_candidate(self, git_mock: mock.Mock) -> None:
        outside = Path(self.temp.name) / "outside"
        outside.mkdir()
        (self.root / "target").symlink_to(outside, target_is_directory=True)
        with self.assertRaises(project_hygiene.HygieneError):
            project_hygiene.current_candidates(self.worktree)
        git_mock.assert_not_called()

    @mock.patch.object(project_hygiene, "git")
    def test_refuses_candidate_that_is_not_ignored(self, git_mock: mock.Mock) -> None:
        self.write("target/debug/object")
        git_mock.return_value = mock.Mock(returncode=1, stdout="", stderr="")
        with self.assertRaises(project_hygiene.HygieneError):
            project_hygiene.current_candidates(self.worktree)

    @mock.patch.object(project_hygiene, "git")
    def test_refuses_candidate_containing_tracked_files(self, git_mock: mock.Mock) -> None:
        self.write("target/debug/object")
        git_mock.side_effect = [self.git_result(), self.git_result("target/kept\n")]
        with self.assertRaises(project_hygiene.HygieneError):
            project_hygiene.current_candidates(self.worktree)

    @mock.patch.object(project_hygiene, "git")
    def test_partial_failure_stops_and_reports(self, git_mock: mock.Mock) -> None:
        git_mock.side_effect = [self.git_result(), self.git_result()]
        self.write("target/debug/object")

        def fail(_: Path) -> None:
            raise OSError("disk refused")

        _, lines = project_hygiene.clean(self.worktree, remover=fail)
        self.assertEqual(lines[-1], "OUTCOME partial_failure")


if __name__ == "__main__":
    unittest.main()
