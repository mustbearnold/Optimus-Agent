#!/usr/bin/env python3
"""Regression tests for assigned-worktree creation and provisioning."""

from __future__ import annotations

import importlib.util
import os
import stat
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "managed_worktree_provision", ROOT / "scripts" / "managed_worktree_provision.py"
)
assert SPEC and SPEC.loader
WP = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = WP
SPEC.loader.exec_module(WP)


def command(cwd: Path, *args: str) -> subprocess.CompletedProcess[str]:
    completed = subprocess.run(list(args), cwd=cwd, capture_output=True, text=True, check=False)
    if completed.returncode != 0:
        raise AssertionError(
            f"{' '.join(args)} failed ({completed.returncode})\n"
            f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}"
        )
    return completed


class Fixture:
    """A managed Repository/Development layout with one assigned worktree.

    The bare store carries `core.bare = true` plus `extensions.worktreeConfig`
    exactly like the real workspace, so the config.worktree gap reproduces.
    """

    def __init__(self, base: Path) -> None:
        self.base = base
        seed = base / "seed"
        seed.mkdir()
        command(seed, "git", "init", "-b", "main")
        command(seed, "git", "config", "user.name", "Fixture")
        command(seed, "git", "config", "user.email", "fixture@example.invalid")
        (seed / "file.txt").write_text("base\n", encoding="utf-8")
        command(seed, "git", "add", "-A")
        command(seed, "git", "commit", "-m", "seed")

        development = base / "Development"
        development.mkdir()
        self.common = development / "git"
        command(base, "git", "clone", "--bare", str(seed), str(self.common))
        command(base, "git", f"--git-dir={self.common}", "config", "extensions.worktreeConfig", "true")

        self.worktrees = development / "worktrees"
        self.worktrees.mkdir()
        self.caller = self.worktrees / "caller"
        command(
            base,
            "git",
            f"--git-dir={self.common}",
            "worktree",
            "add",
            "-b",
            "claude/caller",
            str(self.caller),
            "main",
        )
        WP.write_worktree_config(self.caller)


class WorktreeProvisionTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.fixture = Fixture(Path(self.temporary.name))

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def test_new_worktree_answers_plain_git_immediately(self) -> None:
        ok = WP.new(self.fixture.caller, "fresh-task", include_system=False)
        self.assertTrue(ok)
        checkout = self.fixture.worktrees / "fresh-task"
        status = subprocess.run(
            ["git", "status", "--short", "--branch"],
            cwd=checkout,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(status.returncode, 0, status.stderr)
        self.assertIn("claude/fresh-task", status.stdout)
        config = WP.metadata_dir(checkout) / "config.worktree"
        self.assertIn("bare = false", config.read_text(encoding="utf-8"))

    def test_new_refuses_collisions_and_bad_names(self) -> None:
        with self.assertRaisesRegex(WP.Refusal, "already exists"):
            WP.new(self.fixture.caller, "caller", include_system=False)
        for bad in ("Has Spaces", "UPPER", "-leading", "a"):
            with self.assertRaisesRegex(WP.Refusal, "lowercase slug"):
                WP.new(self.fixture.caller, bad, include_system=False)

    def test_ready_repairs_a_missing_worktree_config(self) -> None:
        config = WP.metadata_dir(self.fixture.caller) / "config.worktree"
        config.unlink()
        broken = subprocess.run(
            ["git", "status", "--short"],
            cwd=self.fixture.caller,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertNotEqual(broken.returncode, 0, "the gap must reproduce before the repair")

        self.assertTrue(WP.ready(self.fixture.caller, include_system=False))
        repaired = subprocess.run(
            ["git", "status", "--short"],
            cwd=self.fixture.caller,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(repaired.returncode, 0, repaired.stderr)

    def test_ready_installs_missing_node_modules_via_npm_ci(self) -> None:
        app = self.fixture.caller / "apps" / "optimus-ui"
        app.mkdir(parents=True)
        (app / "package.json").write_text("{}\n", encoding="utf-8")

        shim_dir = Path(self.temporary.name) / "bin"
        shim_dir.mkdir()
        log = Path(self.temporary.name) / "npm-calls.log"
        shim = shim_dir / "npm"
        shim.write_text(
            "#!/usr/bin/env bash\n"
            f"echo \"$PWD $*\" >> '{log}'\n"
            "mkdir -p node_modules\n",
            encoding="utf-8",
        )
        shim.chmod(shim.stat().st_mode | stat.S_IEXEC)

        previous = os.environ["PATH"]
        os.environ["PATH"] = f"{shim_dir}:{previous}"
        try:
            self.assertTrue(WP.ready(self.fixture.caller, include_system=False))
        finally:
            os.environ["PATH"] = previous

        self.assertIn("ci --no-audit --no-fund", log.read_text(encoding="utf-8"))
        self.assertTrue((app / "node_modules").is_dir())
        # A second pass finds everything present and runs npm not at all.
        log.unlink()
        self.assertTrue(WP.ready(self.fixture.caller, include_system=False))
        self.assertFalse(log.exists())


if __name__ == "__main__":
    unittest.main(verbosity=1)
