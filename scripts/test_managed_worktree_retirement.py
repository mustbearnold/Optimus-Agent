#!/usr/bin/env python3
"""Disposable-Git regression for recoverable managed worktree retirement."""

from __future__ import annotations

import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

import managed_worktree_retirement as retirement


def command(cwd: Path, *args: str, input_text: str | None = None) -> str:
    env = dict(os.environ)
    env.update({
        "GIT_AUTHOR_NAME": "Fixture", "GIT_AUTHOR_EMAIL": "fixture@example.invalid",
        "GIT_COMMITTER_NAME": "Fixture", "GIT_COMMITTER_EMAIL": "fixture@example.invalid",
    })
    result = subprocess.run(
        args, cwd=cwd, env=env, input=input_text, text=True,
        capture_output=True, check=False,
    )
    if result.returncode != 0:
        raise AssertionError(f"{' '.join(args)} failed\n{result.stdout}\n{result.stderr}")
    return result.stdout.strip()


class WorktreeRetirementTests(unittest.TestCase):
    def test_dirty_tree_is_preserved_before_stale_checkout_is_removed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "Optimus Agent"
            common = root / "Development" / "git"
            current = root / "Development" / "worktrees" / "current"
            stale = root / "Development" / "worktrees" / "stale"
            common.parent.mkdir(parents=True)
            command(root.parent, "git", "init", "--bare", str(common))
            blob = command(root.parent, "git", f"--git-dir={common}", "hash-object", "-w", "--stdin", input_text="base\n")
            tree = command(root.parent, "git", f"--git-dir={common}", "mktree", input_text=f"100644 blob {blob}\tfile.txt\n")
            commit = command(root.parent, "git", f"--git-dir={common}", "commit-tree", tree, input_text="base\n")
            command(root.parent, "git", f"--git-dir={common}", "update-ref", "refs/heads/main", commit)
            current.parent.mkdir(parents=True)
            command(root.parent, "git", f"--git-dir={common}", "worktree", "add", "-b", "task/current", str(current), commit)
            command(root.parent, "git", f"--git-dir={common}", "worktree", "add", "-b", "task/stale", str(stale), commit)
            repository = root / "Repository"
            command(root.parent, "git", f"--git-dir={common}", "worktree", "add", "--detach", str(repository), commit)
            (stale / "file.txt").write_text("unlanded\n", encoding="utf-8")
            orphan = root / "Development" / "worktrees" / "orphan-copy"
            shutil.copytree(stale, orphan)

            planned = retirement.plan(current)
            self.assertEqual([str(stale)], [item["path"] for item in planned["retire"]])
            self.assertEqual([str(orphan)], [item["path"] for item in planned["orphan"]])
            self.assertTrue(planned["retire"][0]["dirty"])

            receipt = retirement.execute(current, planned["sha256"])
            self.assertFalse(stale.exists())
            self.assertFalse(orphan.exists())
            self.assertTrue(current.exists())
            self.assertTrue(repository.exists())
            recovery = next(item for item in receipt["recoveries"] if item["path"] == str(stale))
            recovered = command(root.parent, "git", f"--git-dir={common}", "show", f"{recovery['commit']}:file.txt")
            self.assertEqual(recovered, "unlanded")


if __name__ == "__main__":
    unittest.main(verbosity=2)
