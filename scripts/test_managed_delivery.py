#!/usr/bin/env python3
"""Regression tests for the repository-managed delivery boundary."""

from __future__ import annotations

import importlib.util
import os
import subprocess
import sys
import tempfile
import unittest
from contextlib import contextmanager
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "managed_delivery", ROOT / "scripts" / "managed_delivery.py"
)
assert SPEC and SPEC.loader
MD = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MD
SPEC.loader.exec_module(MD)


def command(
    cwd: Path,
    *args: str,
    check: bool = True,
    env: dict[str, str] | None = None,
) -> subprocess.CompletedProcess[str]:
    completed = subprocess.run(
        list(args),
        cwd=cwd,
        env=env,
        capture_output=True,
        text=True,
        check=False,
    )
    if check and completed.returncode != 0:
        raise AssertionError(
            f"{' '.join(args)} failed ({completed.returncode})\n"
            f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}"
        )
    return completed


@contextmanager
def cwd(path: Path):
    previous = Path.cwd()
    os.chdir(path)
    try:
        yield
    finally:
        os.chdir(previous)


class Fixture:
    def __init__(self, base: Path) -> None:
        self.base = base
        self.remote = base / "remote.git"
        self.seed = base / "seed"
        self.repo_root = base / "repo"
        self.common = self.repo_root / ".git"
        self.worktree = self.repo_root / "local" / "worktrees" / "task"

        command(base, "git", "init", "--bare", str(self.remote))
        self.seed.mkdir()
        command(self.seed, "git", "init", "-b", "main")
        command(self.seed, "git", "config", "user.name", "Fixture")
        command(self.seed, "git", "config", "user.email", "fixture@example.invalid")
        (self.seed / ".gitignore").write_text("ignored.txt\n", encoding="utf-8")
        (self.seed / "file.txt").write_text("base\n", encoding="utf-8")
        scripts = self.seed / "scripts"
        scripts.mkdir()
        (scripts / "verify.sh").write_text(
            "#!/usr/bin/env bash\nset -u\n"
            "[ -n \"${OPTIMUS_VERIFY_FORBID_SKIPS:-}\" ]\n"
            "printf 'fixture verify ok\\n'\n",
            encoding="utf-8",
        )
        command(self.seed, "git", "add", "-A")
        command(self.seed, "git", "commit", "-m", "seed")
        command(self.seed, "git", "remote", "add", "origin", str(self.remote))
        command(self.seed, "git", "push", "origin", "main")
        command(base, "git", f"--git-dir={self.remote}", "symbolic-ref", "HEAD", "refs/heads/main")

        self.repo_root.mkdir()
        command(
            base,
            "git",
            "clone",
            "--bare",
            str(self.remote),
            str(self.common),
        )
        self.worktree.parent.mkdir(parents=True)
        command(
            base,
            "git",
            f"--git-dir={self.common}",
            "worktree",
            "add",
            "-b",
            "task/test-managed-delivery",
            str(self.worktree),
            "main",
        )
        self.repo = MD.Repository.discover(self.worktree)
        self.initial = self.repo.git(["rev-parse", "HEAD"]).stdout.strip()

    def remote_main(self) -> str:
        return command(
            self.base,
            "git",
            f"--git-dir={self.remote}",
            "rev-parse",
            "refs/heads/main",
        ).stdout.strip()

    def local_main(self) -> str:
        return self.repo.git(["rev-parse", "refs/heads/main"]).stdout.strip()

    def advance_remote(self) -> str:
        clone = self.base / "other"
        command(self.base, "git", "clone", str(self.remote), str(clone))
        command(clone, "git", "config", "user.name", "Other")
        command(clone, "git", "config", "user.email", "other@example.invalid")
        (clone / "other.txt").write_text("concurrent\n", encoding="utf-8")
        command(clone, "git", "add", "other.txt")
        command(clone, "git", "commit", "-m", "concurrent")
        command(clone, "git", "push", "origin", "main")
        return self.remote_main()


class ManagedDeliveryTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.base = Path(self.temporary.name)
        self.fixture = Fixture(self.base)

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def test_checkpoint_captures_source_without_moving_or_staging(self) -> None:
        fixture = self.fixture
        (fixture.worktree / "file.txt").write_text("edited\n", encoding="utf-8")
        (fixture.worktree / "new.txt").write_text("new\n", encoding="utf-8")
        (fixture.worktree / "ignored.txt").write_text("secret\n", encoding="utf-8")
        status_before = fixture.repo.git(["status", "--porcelain=v1"]).stdout

        result = MD.checkpoint(fixture.repo, "alpha")

        self.assertEqual(fixture.initial, fixture.repo.git(["rev-parse", "HEAD"]).stdout.strip())
        self.assertEqual(status_before, fixture.repo.git(["status", "--porcelain=v1"]).stdout)
        names = fixture.repo.git(
            ["ls-tree", "-r", "--name-only", str(result["tree"])]
        ).stdout.splitlines()
        self.assertIn("file.txt", names)
        self.assertIn("new.txt", names)
        self.assertNotIn("ignored.txt", names)
        self.assertEqual("edited\n", (fixture.worktree / "file.txt").read_text())

    def test_checkpoint_label_cannot_silently_move(self) -> None:
        fixture = self.fixture
        (fixture.worktree / "file.txt").write_text("one\n", encoding="utf-8")
        first = MD.checkpoint(fixture.repo, "alpha")
        same = MD.checkpoint(fixture.repo, "alpha")
        self.assertEqual(first["commit"], same["commit"])
        (fixture.worktree / "file.txt").write_text("two\n", encoding="utf-8")
        with self.assertRaisesRegex(MD.Refusal, "different progress"):
            MD.checkpoint(fixture.repo, "alpha")

    def test_undo_restores_exact_nonignored_tree_and_keeps_head(self) -> None:
        fixture = self.fixture
        (fixture.worktree / "file.txt").write_text("checkpoint\n", encoding="utf-8")
        (fixture.worktree / "present.txt").write_text("present\n", encoding="utf-8")
        MD.checkpoint(fixture.repo, "good")

        (fixture.worktree / "file.txt").write_text("later\n", encoding="utf-8")
        (fixture.worktree / "present.txt").unlink()
        (fixture.worktree / "later.txt").write_text("remove me\n", encoding="utf-8")
        (fixture.worktree / "ignored.txt").write_text("preserve me\n", encoding="utf-8")
        head_before = fixture.repo.git(["rev-parse", "HEAD"]).stdout.strip()

        result = MD.undo(fixture.repo, "good")

        self.assertEqual("checkpoint\n", (fixture.worktree / "file.txt").read_text())
        self.assertEqual("present\n", (fixture.worktree / "present.txt").read_text())
        self.assertFalse((fixture.worktree / "later.txt").exists())
        self.assertEqual("preserve me\n", (fixture.worktree / "ignored.txt").read_text())
        self.assertEqual(head_before, fixture.repo.git(["rev-parse", "HEAD"]).stdout.strip())
        self.assertTrue(str(result["safety_checkpoint"]).startswith("before-undo-"))

    def test_undo_is_scoped_to_the_invoking_worktree(self) -> None:
        with self.assertRaisesRegex(MD.Refusal, "does not exist"):
            MD.undo(self.fixture.repo, "foreign")

    def test_red_verify_refuses_without_moving_any_branch(self) -> None:
        fixture = self.fixture
        (fixture.worktree / "file.txt").write_text("candidate\n", encoding="utf-8")
        (fixture.worktree / "scripts" / "verify.sh").write_text(
            "#!/usr/bin/env bash\nprintf 'red fixture\\n'\nexit 7\n",
            encoding="utf-8",
        )
        remote_before = fixture.remote_main()
        branch_before = fixture.repo.git(["rev-parse", fixture.repo.branch]).stdout.strip()

        with self.assertRaisesRegex(MD.Refusal, "just verify failed"):
            MD.land(fixture.repo, "red-gate", "fixture-model", "high")

        self.assertEqual(remote_before, fixture.remote_main())
        self.assertEqual(
            branch_before, fixture.repo.git(["rev-parse", fixture.repo.branch]).stdout.strip()
        )
        receipt = fixture.repo.state_dir / "tasks" / "red-gate" / "receipt.json"
        self.assertFalse(receipt.exists())

    def test_land_creates_one_machine_commit_and_only_advances_remote_main(self) -> None:
        fixture = self.fixture
        (fixture.worktree / "file.txt").write_text("candidate\n", encoding="utf-8")
        local_main_before = fixture.local_main()

        receipt = MD.land(
            fixture.repo, "managed-delivery-smoke", "fixture-model", "xhigh"
        )
        commit = str(receipt["commit"]["sha"])

        self.assertEqual(commit, fixture.remote_main())
        self.assertEqual(local_main_before, fixture.local_main())
        self.assertEqual(commit, fixture.repo.git(["rev-parse", "HEAD"]).stdout.strip())
        self.assertEqual("", fixture.repo.git(["status", "--porcelain=v1"]).stdout)
        self.assertEqual(
            fixture.initial,
            fixture.repo.git(["show", "-s", "--format=%P", commit]).stdout.strip(),
        )
        message = fixture.repo.git(["show", "-s", "--format=%B", commit]).stdout
        self.assertIn("🔧 chore(delivery): managed-delivery-smoke", message)
        self.assertIn("Gates: just verify PASS (no skips)", message)
        self.assertIn("Model: fixture-model", message)
        self.assertIn("Effort: xhigh", message)
        self.assertTrue(
            (
                fixture.repo.state_dir
                / "tasks"
                / "managed-delivery-smoke"
                / "receipt.json"
            ).is_file()
        )

        repeated = MD.land(
            fixture.repo, "managed-delivery-smoke", "fixture-model", "xhigh"
        )
        self.assertEqual(commit, repeated["commit"]["sha"])
        with self.assertRaisesRegex(MD.Refusal, "different provenance"):
            MD.land(fixture.repo, "managed-delivery-smoke", "other-model", "xhigh")

    def test_a_killed_land_does_not_wedge_its_task_id(self) -> None:
        fixture = self.fixture
        (fixture.worktree / "file.txt").write_text("first try\n", encoding="utf-8")
        # A kill mid-verify leaves exactly this state: the automatic pre-land
        # checkpoint exists, and no attempt receipt was ever written.
        MD._create_checkpoint(
            fixture.repo,
            "pre-land-wedge-proof-1",
            kind="automatic-before-land",
            tree=fixture.repo.snapshot_tree(),
        )
        # The retry carries different progress, as any fixed-up tree does.
        (fixture.worktree / "file.txt").write_text("second try\n", encoding="utf-8")

        receipt = MD.land(fixture.repo, "wedge-proof", "fixture-model", "high")
        self.assertEqual(receipt["state"], "landed")
        self.assertEqual(int(str(receipt["attempt"])), 2)

    def test_remote_main_advance_refuses_instead_of_rebasing(self) -> None:
        fixture = self.fixture
        (fixture.worktree / "file.txt").write_text("candidate\n", encoding="utf-8")
        advanced = fixture.advance_remote()
        branch_before = fixture.repo.git(["rev-parse", "HEAD"]).stdout.strip()

        with self.assertRaisesRegex(MD.Refusal, "remote main advanced"):
            MD.land(fixture.repo, "stale-task", "fixture-model", "medium")

        self.assertEqual(advanced, fixture.remote_main())
        self.assertEqual(branch_before, fixture.repo.git(["rev-parse", "HEAD"]).stdout.strip())

    def test_argument_validation_rejects_ref_and_shell_injection(self) -> None:
        for value in ("../escape", "UPPER", "-flag", "a/b", "a..b"):
            with self.subTest(value=value):
                with self.assertRaises(MD.Refusal):
                    MD.validate_slug(value, "label")
        with self.assertRaises(MD.Refusal):
            MD.validate_model("model; touch escaped")
        with self.assertRaises(MD.Refusal):
            MD.validate_effort("enormous")

    def test_discovery_refuses_a_normal_checkout(self) -> None:
        with self.assertRaisesRegex(MD.Refusal, "assigned linked worktree"):
            MD.Repository.discover(self.fixture.seed)


if __name__ == "__main__":
    unittest.main(verbosity=2)
