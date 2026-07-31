#!/usr/bin/env python3
"""Disposable integration test for the Source/Development migration."""

from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def load(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


LAYOUT = load("workspace_layout", ROOT / "scripts" / "workspace_layout.py")
DELIVERY = load("layout_managed_delivery", ROOT / "scripts" / "managed_delivery.py")


def command(cwd: Path, *args: str) -> str:
    result = subprocess.run(args, cwd=cwd, text=True, capture_output=True, check=False)
    if result.returncode != 0:
        raise AssertionError(f"{' '.join(args)} failed\n{result.stdout}\n{result.stderr}")
    return result.stdout.strip()


def main() -> int:
    with tempfile.TemporaryDirectory() as temporary:
        base = Path(temporary)
        seed = base / "seed"
        root = base / "Optimus Agent"
        seed.mkdir()
        command(seed, "git", "init", "-b", "main")
        command(seed, "git", "config", "user.name", "Fixture")
        command(seed, "git", "config", "user.email", "fixture@example.invalid")
        (seed / "README.md").write_text("landed source\n", encoding="utf-8")
        command(seed, "git", "add", "README.md")
        command(seed, "git", "commit", "-m", "fixture")

        root.mkdir()
        command(base, "git", "clone", "--bare", str(seed), str(root / ".git"))
        task = root / "local" / "worktrees" / "task"
        task.parent.mkdir(parents=True)
        command(
            base, "git", f"--git-dir={root / '.git'}", "worktree", "add", "-b",
            "task/layout", str(task), "main",
        )
        command(
            base, "git", f"--git-dir={root / '.git'}", "config", "core.worktree", str(task)
        )
        command(
            base, "git", f"--git-dir={root / '.git'}", "config",
            "extensions.worktreeConfig", "true",
        )
        (root / "README.md").write_text("stale root shadow\n", encoding="utf-8")
        (root / "target").mkdir()
        (root / "target" / "cache").write_text("rebuildable\n", encoding="utf-8")
        (root / "local" / "land").mkdir()

        planned = LAYOUT.report(task)
        assert "README.md" in planned["source_shadow_entries"]
        result = LAYOUT.apply_layout(task)
        assert result["source_ref"] == "refs/heads/main"
        assert (root / "Source" / "README.md").read_text() == "landed source\n"
        assert (root / "Development" / "git").is_dir()
        assert (root / "Development" / "worktrees" / "task").is_dir()
        assert (root / "Development" / "Archive" / "stale-root-snapshot" / "README.md").is_file()
        assert (root / ".git").is_symlink()
        assert (root / "local").is_symlink()
        assert (root / "WORKSPACE.md").is_file()

        moved_task = root / "Development" / "worktrees" / "task"
        repository = DELIVERY.Repository.discover(moved_task)
        assert repository.repo_root == root.resolve()
        assert repository.state_dir == (root / "Development" / "land").resolve()
        assert repository.branch == "refs/heads/task/layout"
        assert repository.git(["status", "--porcelain=v1"]).stdout == ""

        try:
            LAYOUT.apply_layout(moved_task)
        except LAYOUT.LayoutRefusal:
            pass
        else:
            raise AssertionError("workspace migration accepted a second application")

    print("WORKSPACE_LAYOUT_SELFTEST_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
