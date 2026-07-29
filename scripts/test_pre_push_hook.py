#!/usr/bin/env python3
"""Regression test: pre-push gates cannot inherit hook-local Git state (#120)."""

from __future__ import annotations

import os
import shutil
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
HOOK = ROOT / ".githooks/pre-push"


def clean_git_env() -> dict[str, str]:
    env = os.environ.copy()
    result = subprocess.run(
        ["git", "rev-parse", "--local-env-vars"],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    for name in result.stdout.splitlines():
        env.pop(name, None)
    return env


def git(cwd: Path, *args: str, env: dict[str, str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", *args],
        cwd=cwd,
        env=env,
        check=True,
        capture_output=True,
        text=True,
    )


class PrePushHookIsolationTest(unittest.TestCase):
    def test_linked_worktree_gate_cannot_reinitialize_real_git_dir(self) -> None:
        env = clean_git_env()
        with tempfile.TemporaryDirectory() as raw:
            base = Path(raw)
            repo = base / "repo"
            remote = base / "origin.git"
            worktree = base / "worktree"
            repo.mkdir()

            git(base, "init", "--quiet", "--bare", str(remote), env=env)
            git(repo, "init", "--quiet", env=env)
            git(repo, "config", "user.email", "runs@example.invalid", env=env)
            git(repo, "config", "user.name", "Optimus Test", env=env)

            hooks = repo / ".githooks"
            scripts = repo / "scripts"
            hooks.mkdir()
            scripts.mkdir()
            shutil.copy2(HOOK, hooks / "pre-push")
            (hooks / "pre-push").chmod(
                (hooks / "pre-push").stat().st_mode | stat.S_IXUSR
            )

            # This gate models the dangerous fixture command. If GIT_DIR leaks
            # from the hook, it changes the real worktree repository to bare.
            verify = scripts / "verify.sh"
            verify.write_text(
                "#!/usr/bin/env bash\n"
                "set -euo pipefail\n"
                "fixture=$(mktemp -d)\n"
                "git -C \"$fixture\" init --quiet --bare origin.git\n",
                encoding="utf-8",
            )
            verify.chmod(verify.stat().st_mode | stat.S_IXUSR)
            (scripts / "verify_skip_report.py").write_text("", encoding="utf-8")
            (repo / "tracked.txt").write_text("seed\n", encoding="utf-8")

            git(repo, "add", ".", env=env)
            git(repo, "commit", "--quiet", "-m", "seed", env=env)
            git(repo, "config", "core.hooksPath", ".githooks", env=env)
            git(repo, "remote", "add", "origin", str(remote), env=env)
            git(repo, "worktree", "add", "--quiet", "-b", "wip/test", str(worktree), env=env)

            git(worktree, "push", "-u", "origin", "HEAD:wip/test", env=env)

            self.assertEqual(
                git(worktree, "config", "--get", "core.bare", env=env).stdout.strip(),
                "false",
            )
            self.assertEqual(
                git(worktree, "rev-parse", "--abbrev-ref", "HEAD", env=env).stdout.strip(),
                "wip/test",
            )
            self.assertEqual(
                git(worktree, "rev-parse", "--show-toplevel", env=env).stdout.strip(),
                str(worktree),
            )


if __name__ == "__main__":
    unittest.main()
