#!/usr/bin/env python3
"""Migrate the Optimus wrapper into clear Source and Development planes."""

from __future__ import annotations

import argparse
import hashlib
import os
import shutil
import subprocess
from pathlib import Path


class LayoutRefusal(RuntimeError):
    """The workspace could not be migrated without broad or ambiguous effects."""


def git(git_dir: Path, cwd: Path, *args: str) -> subprocess.CompletedProcess[str]:
    env = {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}
    return subprocess.run(
        ["git", f"--git-dir={git_dir}", *args],
        cwd=cwd,
        env=env,
        text=True,
        capture_output=True,
        check=False,
    )


def discover_legacy_root(worktree: Path) -> tuple[Path, Path]:
    worktree = worktree.resolve()
    marker = worktree / ".git"
    if not marker.is_file() or marker.is_symlink():
        raise LayoutRefusal("run from an assigned linked worktree")
    line = marker.read_text(encoding="utf-8").strip()
    if not line.startswith("gitdir: "):
        raise LayoutRefusal("linked worktree has no absolute Git pointer")
    git_dir = Path(line.removeprefix("gitdir: ").strip()).resolve()
    commondir = git_dir / "commondir"
    if not commondir.is_file():
        raise LayoutRefusal("linked worktree has no common Git directory")
    common = (git_dir / commondir.read_text(encoding="utf-8").strip()).resolve()
    if common.name != ".git":
        raise LayoutRefusal("workspace is already migrated or is not the legacy layout")
    root = common.parent.resolve()
    assigned = (root / "local" / "worktrees").resolve()
    try:
        worktree.relative_to(assigned)
    except ValueError as error:
        raise LayoutRefusal("caller is outside the legacy assigned-worktree root") from error
    return root, common


def preflight(root: Path, common: Path, worktree: Path) -> list[str]:
    if (root / "Development").exists() or (root / "Source").exists():
        raise LayoutRefusal("Source or Development already exists; refusing a partial re-run")
    local = root / "local"
    if local.is_symlink() or not local.is_dir():
        raise LayoutRefusal("legacy local directory is missing or already redirected")
    if common.is_symlink() or not common.is_dir():
        raise LayoutRefusal("legacy bare Git directory is missing or redirected")
    marker_line = (worktree / ".git").read_text(encoding="utf-8").strip()
    git_dir = Path(marker_line.removeprefix("gitdir: ").strip())
    status = subprocess.run(
        [
            "git", f"--git-dir={git_dir}", f"--work-tree={worktree}",
            "status", "--porcelain=v1",
        ],
        cwd=worktree,
        text=True,
        capture_output=True,
        check=False,
    )
    if status.returncode != 0 or status.stdout.strip():
        raise LayoutRefusal("assigned worktree must be clean before workspace migration")
    return sorted(path.name for path in root.iterdir() if path.name not in {".git", "local"})


def preserve_worktree_identities(common: Path) -> None:
    registrations = common / "worktrees"
    if not registrations.is_dir():
        return
    for registration in registrations.iterdir():
        backpointer = registration / "gitdir"
        if not backpointer.is_file():
            continue
        marker = Path(backpointer.read_text(encoding="utf-8").strip())
        checkout = marker.parent.resolve(strict=False)
        identity = hashlib.sha256(str(checkout).encode()).hexdigest()[:16]
        identity_file = registration / "optimus-worktree-id"
        if identity_file.exists() and identity_file.read_text(encoding="utf-8").strip() != identity:
            raise LayoutRefusal(f"worktree identity conflict: {registration.name}")
        identity_file.write_text(identity + "\n", encoding="utf-8")


def write_workspace_guides(root: Path) -> None:
    (root / "WORKSPACE.md").write_text(
        "# Optimus Agent workspace\n\n"
        "- `Source/` is a clean, read-only view of the currently landed GitHub `main`.\n"
        "- `Development/` contains agent worktrees, managed delivery records, build output, "
        "tools, raw evidence, caches, and the recoverable pre-migration snapshot.\n\n"
        "Coding agents must work in an assigned `Development/worktrees/*` checkout. They must "
        "not edit `Source/` directly. The compatibility links `.git` and `local` keep older "
        "automation working while resolving into `Development/`.\n",
        encoding="utf-8",
    )
    (root / "Development" / "README.md").write_text(
        "# Optimus Agent Development\n\n"
        "This directory is machine-local and is not the GitHub source tree.\n\n"
        "- `git/`: shared bare Git control store\n"
        "- `worktrees/`: isolated coding-agent checkouts\n"
        "- `land/`: managed checkpoints, immutable receipts, locks, and gate evidence\n"
        "- `tools/`: repository-local development tools\n"
        "- `tmp/` and `t/`: raw or temporary evidence\n"
        "- `Archive/stale-root-snapshot/`: recoverable source-looking files removed from the "
        "old mixed root\n",
        encoding="utf-8",
    )


def apply_layout(worktree: Path) -> dict[str, object]:
    root, common = discover_legacy_root(worktree)
    worktree = worktree.resolve()
    shadow_names = preflight(root, common, worktree)
    preserve_worktree_identities(common)

    development = root / "Development"
    os.replace(root / "local", development)
    os.replace(root / ".git", development / "git")

    archive = development / "Archive" / "stale-root-snapshot"
    archive.mkdir(parents=True)
    for name in shadow_names:
        source = root / name
        if source.exists() or source.is_symlink():
            os.replace(source, archive / name)

    (root / ".git").symlink_to(Path("Development") / "git", target_is_directory=True)
    (root / "local").symlink_to("Development", target_is_directory=True)

    git_dir = development / "git"
    remote_main = git(git_dir, root, "show-ref", "--verify", "refs/remotes/origin/main")
    main_ref = "refs/remotes/origin/main" if remote_main.returncode == 0 else "refs/heads/main"
    source = root / "Source"
    added = git(git_dir, root, "worktree", "add", "--detach", str(source), main_ref)
    if added.returncode != 0:
        raise LayoutRefusal(
            "layout moved safely but Source creation failed: "
            + (added.stderr.strip() or added.stdout.strip())
        )
    write_workspace_guides(root)
    return {
        "root": str(root),
        "source": str(source),
        "development": str(development),
        "archived_entries": len(shadow_names),
        "source_ref": main_ref,
    }


def report(worktree: Path) -> dict[str, object]:
    root, common = discover_legacy_root(worktree)
    shadow = preflight(root, common, worktree.resolve())
    return {
        "root": str(root),
        "planned_source": str(root / "Source"),
        "planned_development": str(root / "Development"),
        "source_shadow_entries": shadow,
        "compatibility_links": [".git -> Development/git", "local -> Development"],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("report", "apply"))
    args = parser.parse_args()
    try:
        result = report(Path.cwd()) if args.command == "report" else apply_layout(Path.cwd())
    except LayoutRefusal as error:
        print(f"workspace layout refused: {error}")
        return 1
    for key, value in result.items():
        print(f"{key}: {value}")
    print("outcome: completed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
