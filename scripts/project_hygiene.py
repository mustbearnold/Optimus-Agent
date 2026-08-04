#!/usr/bin/env python3
"""Report project clutter and clean a closed worktree-local allowlist."""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Callable

ROOT = Path(__file__).resolve().parents[1]
WARNING_BYTES = 20 * 1024**3

CLEANABLE = (
    "target",
    "apps/optimus-desktop/node_modules",
    "apps/optimus-ui/node_modules",
    "apps/optimus-ui/dist",
    "scripts/__pycache__",
    ".engineering-memory",
    "apps/optimus-ui/tsconfig.tsbuildinfo",
)

SHARED_REPORT_ONLY = (
    "target",
)


class HygieneError(RuntimeError):
    """The requested cleanup could not be proven worktree-local and safe."""


@dataclass(frozen=True)
class Worktree:
    root: Path
    git_dir: Path
    common_dir: Path
    repository_root: Path
    development_root: Path


def resolve_worktree(root: Path = ROOT) -> Worktree:
    root = root.resolve(strict=True)
    gitfile = root / ".git"
    if gitfile.is_symlink() or not gitfile.is_file():
        raise HygieneError("clean must run from a linked worktree with a .git file")
    first = gitfile.read_text(encoding="utf-8").strip()
    if not first.startswith("gitdir: "):
        raise HygieneError("worktree .git file has no gitdir pointer")
    git_dir = Path(first.removeprefix("gitdir: ").strip())
    if not git_dir.is_absolute():
        git_dir = gitfile.parent / git_dir
    git_dir = git_dir.resolve(strict=True)

    commondir_file = git_dir / "commondir"
    if not commondir_file.is_file():
        raise HygieneError("worktree metadata has no commondir")
    common_dir = (git_dir / commondir_file.read_text(encoding="utf-8").strip()).resolve(
        strict=True
    )
    backpointer = git_dir / "gitdir"
    if not backpointer.is_file():
        raise HygieneError("worktree metadata has no checkout backpointer")
    pointed = Path(backpointer.read_text(encoding="utf-8").strip()).resolve(strict=False)
    if pointed != gitfile.resolve(strict=False):
        raise HygieneError("worktree metadata backpointer does not match this checkout")
    if common_dir.name == ".git" and common_dir.is_dir():
        repository_root = common_dir.parent
        development_root = repository_root / "local"
    elif (
        common_dir.name == "git"
        and common_dir.parent.name == "Development"
        and common_dir.is_dir()
    ):
        development_root = common_dir.parent
        repository_root = development_root.parent
    else:
        raise HygieneError("common Git directory is not the Optimus bare repository")
    return Worktree(
        root, git_dir, common_dir, repository_root.resolve(), development_root.resolve()
    )


def path_size(path: Path) -> int:
    if path.is_symlink() or path.is_file():
        return path.lstat().st_size
    total = 0
    for current, dirs, files in os.walk(path, followlinks=False):
        base = Path(current)
        for name in files:
            try:
                total += (base / name).lstat().st_size
            except FileNotFoundError:
                pass
        for name in dirs:
            child = base / name
            if child.is_symlink():
                try:
                    total += child.lstat().st_size
                except FileNotFoundError:
                    pass
    return total


def git(worktree: Worktree, *args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [
            "git",
            f"--git-dir={worktree.git_dir}",
            f"--work-tree={worktree.root}",
            *args,
        ],
        cwd=worktree.root,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )


def _reject_symlink_components(root: Path, path: Path) -> None:
    current = root
    for part in path.relative_to(root).parts:
        current = current / part
        if current.is_symlink():
            raise HygieneError(f"cleanable path contains symlink component: {current}")


def validate_candidate(worktree: Worktree, relative: str) -> Path | None:
    if relative not in CLEANABLE:
        raise HygieneError(f"path is not in the closed cleanable manifest: {relative}")
    path = worktree.root / relative
    if not path.exists() and not path.is_symlink():
        return None
    _reject_symlink_components(worktree.root, path)
    try:
        path.resolve(strict=True).relative_to(worktree.root)
    except (OSError, ValueError) as exc:
        raise HygieneError(f"cleanable path escapes assigned worktree: {relative}") from exc
    # A trailing-slash ignore rule matches directory contents and the directory
    # path in Git's pathspec semantics only when the probe is also directory-shaped.
    ignore_probe = f"{relative}/" if path.is_dir() else relative
    ignored = git(worktree, "check-ignore", "--no-index", "-q", "--", ignore_probe)
    if ignored.returncode != 0:
        raise HygieneError(f"cleanable path is not gitignored: {relative}")
    tracked = git(worktree, "ls-files", "--", relative)
    if tracked.returncode != 0:
        raise HygieneError(f"could not inspect tracked files under: {relative}")
    if tracked.stdout.strip():
        raise HygieneError(f"cleanable path contains tracked files: {relative}")
    return path


def current_candidates(worktree: Worktree) -> list[tuple[str, Path, int]]:
    found: list[tuple[str, Path, int]] = []
    for relative in CLEANABLE:
        path = validate_candidate(worktree, relative)
        if path is not None:
            found.append((relative, path, path_size(path)))
    return found


def _report_external(worktree: Worktree) -> list[str]:
    lines: list[str] = []
    for relative in SHARED_REPORT_ONLY:
        path = worktree.repository_root / relative
        if path.exists() and not path.is_symlink():
            lines.append(f"REPORT_ONLY {path} bytes={path_size(path)}")
    worktrees_root = worktree.development_root / "worktrees"
    if worktrees_root.is_dir():
        for sibling in sorted(worktrees_root.iterdir()):
            if sibling.resolve(strict=False) == worktree.root:
                continue
            for relative in CLEANABLE:
                path = sibling / relative
                if path.exists() and not path.is_symlink():
                    lines.append(f"REPORT_ONLY {path} bytes={path_size(path)}")
    return lines


def report(worktree: Worktree) -> tuple[int, list[str]]:
    found = current_candidates(worktree)
    total = sum(size for _, _, size in found)
    lines = [f"CLEANABLE {relative} bytes={size}" for relative, _, size in found]
    lines.append(f"CURRENT_WORKTREE paths={len(found)} bytes={total}")
    if total > WARNING_BYTES:
        lines.append(f"WARNING rebuildable artifacts exceed {WARNING_BYTES} bytes")
    lines.extend(_report_external(worktree))
    lines.append("OUTCOME completed")
    return total, lines


def clean(
    worktree: Worktree,
    remover: Callable[[Path], None] | None = None,
) -> tuple[int, list[str]]:
    found = current_candidates(worktree)  # validate everything before mutation
    remove = remover or (
        lambda path: path.unlink()
        if path.is_file()
        else shutil.rmtree(path, ignore_errors=False)
    )
    removed = 0
    lines: list[str] = []
    try:
        for relative, path, size in found:
            remove(path)
            removed += size
            lines.append(f"REMOVED {relative} bytes={size}")
    except KeyboardInterrupt:
        lines.append(f"RECLAIMED bytes={removed}")
        lines.append("OUTCOME cancelled")
        return removed, lines
    except OSError as exc:
        lines.append(f"ERROR {exc}")
        lines.append(f"RECLAIMED bytes={removed}")
        lines.append("OUTCOME partial_failure")
        return removed, lines
    lines.append(f"RECLAIMED bytes={removed}")
    lines.extend(_report_external(worktree))
    lines.append("OUTCOME completed")
    return removed, lines


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("command", nargs="?", choices=("report", "clean"), default="report")
    args = parser.parse_args(argv)
    try:
        worktree = resolve_worktree()
        _, lines = clean(worktree) if args.command == "clean" else report(worktree)
    except HygieneError as exc:
        print(f"OUTCOME refused: {exc}", file=sys.stderr)
        return 1
    print("\n".join(lines))
    return 1 if lines[-1] == "OUTCOME partial_failure" else 130 if lines[-1] == "OUTCOME cancelled" else 0


if __name__ == "__main__":
    raise SystemExit(main())
