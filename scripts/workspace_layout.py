#!/usr/bin/env python3
"""Manage the Optimus Repository and Development workspace planes."""

from __future__ import annotations

import argparse
import contextlib
import datetime as dt
import fcntl
import hashlib
import json
import os
import shutil
import subprocess
import re
import tempfile
from pathlib import Path
from typing import Iterator


class LayoutRefusal(RuntimeError):
    """The workspace could not be migrated without broad or ambiguous effects."""


OID = re.compile(r"^[0-9a-f]{40}$")


def git(
    git_dir: Path,
    cwd: Path,
    *args: str,
    work_tree: Path | None = None,
) -> subprocess.CompletedProcess[str]:
    env = {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}
    return subprocess.run(
        [
            "git",
            f"--git-dir={git_dir}",
            *([f"--work-tree={work_tree}"] if work_tree is not None else []),
            *args,
        ],
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


def discover_migrated_root(worktree: Path) -> tuple[Path, Path]:
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
    if common.name != "git" or common.parent.name != "Development":
        raise LayoutRefusal("workspace has not been migrated")
    root = common.parent.parent.resolve()
    assigned = (root / "Development" / "worktrees").resolve()
    try:
        worktree.relative_to(assigned)
    except ValueError as error:
        raise LayoutRefusal("caller is outside the migrated assigned-worktree root") from error
    return root, common


def worktree_git_dir(view: Path, common: Path) -> Path:
    marker = view / ".git"
    if not marker.is_file() or marker.is_symlink():
        raise LayoutRefusal(f"clean repository view has no linked-worktree pointer: {view}")
    line = marker.read_text(encoding="utf-8").strip()
    if not line.startswith("gitdir: "):
        raise LayoutRefusal("clean repository view has a malformed Git pointer")
    git_dir = Path(line.removeprefix("gitdir: ").strip()).resolve()
    commondir = git_dir / "commondir"
    if not commondir.is_file():
        raise LayoutRefusal("clean repository view has no common Git directory")
    resolved_common = (git_dir / commondir.read_text(encoding="utf-8").strip()).resolve()
    if resolved_common != common:
        raise LayoutRefusal("clean repository view belongs to a different Git repository")
    return git_dir


def current_view(root: Path, common: Path) -> tuple[Path | None, Path | None]:
    repository = root / "Repository"
    source = root / "Source"
    if repository.exists() and source.exists():
        raise LayoutRefusal("both Repository and obsolete Source views exist")
    view = repository if repository.exists() else source if source.exists() else None
    return view, worktree_git_dir(view, common) if view is not None else None


def exact_live_main(common: Path, root: Path) -> str:
    result = git(common, root, "ls-remote", "--exit-code", "origin", "refs/heads/main")
    if result.returncode != 0:
        raise LayoutRefusal(
            "cannot read live GitHub main: " + (result.stderr.strip() or result.stdout.strip())
        )
    fields = result.stdout.split()
    if len(fields) != 2 or fields[1] != "refs/heads/main" or not OID.fullmatch(fields[0]):
        raise LayoutRefusal("live GitHub main returned an invalid identity")
    return fields[0]


def ensure_local_commit(common: Path, root: Path, sha: str) -> None:
    if not OID.fullmatch(sha):
        raise LayoutRefusal("repository view target is not a full commit SHA")
    present = git(common, root, "cat-file", "-e", f"{sha}^{{commit}}")
    if present.returncode == 0:
        return
    fetched = git(common, root, "fetch", "--no-write-fetch-head", "origin", sha)
    if fetched.returncode != 0 or git(common, root, "cat-file", "-e", f"{sha}^{{commit}}").returncode != 0:
        raise LayoutRefusal("live main commit is not available locally after exact fetch")


def ref_sha(common: Path, root: Path, ref: str) -> str | None:
    result = git(common, root, "rev-parse", "--verify", ref)
    return result.stdout.strip() if result.returncode == 0 else None


@contextlib.contextmanager
def workspace_lock(root: Path) -> Iterator[None]:
    path = root / "Development" / "land" / "locks" / "land.lock"
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a+", encoding="utf-8") as handle:
        fcntl.flock(handle.fileno(), fcntl.LOCK_EX)
        try:
            yield
        finally:
            fcntl.flock(handle.fileno(), fcntl.LOCK_UN)


def checked_out_ref(common: Path, ref: str) -> list[str]:
    checkouts: list[str] = []
    registrations = common / "worktrees"
    if not registrations.is_dir():
        return checkouts
    for registration in sorted(registrations.iterdir()):
        head = registration / "HEAD"
        backpointer = registration / "gitdir"
        if not head.is_file() or head.read_text(encoding="utf-8").strip() != f"ref: {ref}":
            continue
        if backpointer.is_file():
            checkouts.append(str(Path(backpointer.read_text(encoding="utf-8").strip()).parent))
        else:
            checkouts.append(str(registration))
    return checkouts


def repair_obsolete_core_worktree(common: Path, root: Path, caller: Path) -> bool:
    configured = git(common, root, "config", "--get", "core.worktree")
    if configured.returncode != 0:
        return False
    raw = configured.stdout.strip()
    if not raw:
        return False
    target = Path(raw)
    if not target.is_absolute():
        target = root / target
    resolved_target = target.resolve(strict=False)
    assigned = (root / "Development" / "worktrees").resolve(strict=False)
    try:
        resolved_target.relative_to(assigned)
    except ValueError as error:
        raise LayoutRefusal(
            "obsolete core.worktree is outside the assigned-worktree area; refusing repair: "
            + raw
        ) from error
    repaired = git(common, root, "config", "--unset-all", "core.worktree")
    if repaired.returncode != 0:
        raise LayoutRefusal("cannot remove obsolete core.worktree configuration")
    caller_git_dir = worktree_git_dir(caller, common)
    for key, value in (("core.bare", "false"), ("core.worktree", str(caller))):
        configured_caller = git(
            caller_git_dir,
            caller,
            "config",
            "--worktree",
            key,
            value,
            work_tree=caller,
        )
        if configured_caller.returncode != 0:
            raise LayoutRefusal(f"cannot repair assigned-worktree {key}")
    return True


def update_refs_transaction(
    common: Path,
    root: Path,
    updates: list[tuple[str, str, str]],
) -> None:
    if not updates:
        return
    commands = ["start"]
    commands.extend(f"update {ref} {target} {old}" for ref, target, old in updates)
    commands.extend(("prepare", "commit", ""))
    result = subprocess.run(
        ["git", f"--git-dir={common}", "update-ref", "--stdin"],
        cwd=root,
        env={key: value for key, value in os.environ.items() if not key.startswith("GIT_")},
        input="\n".join(commands),
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        raise LayoutRefusal(
            "cannot atomically update local repository identities: "
            + (result.stderr.strip() or result.stdout.strip())
        )


def record_sync_attempt(root: Path, result: dict[str, object]) -> None:
    directory = root / "Development" / "land" / "workspace-sync"
    directory.mkdir(parents=True, exist_ok=True)
    stamp = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%S.%fZ")
    path = directory / f"attempt-{stamp}.json"
    payload = json.dumps(result, indent=2, sort_keys=True) + "\n"
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "w", encoding="utf-8") as handle:
        handle.write(payload)


def require_fast_forwardable(common: Path, root: Path, ref: str, target: str) -> None:
    current = ref_sha(common, root, ref)
    if current is None or current == target:
        return
    relation = git(common, root, "merge-base", "--is-ancestor", current, target)
    if relation.returncode != 0:
        raise LayoutRefusal(f"{ref} is not an ancestor of live main; refusing to rewrite it")


def preflight_repository_view(worktree: Path, target_sha: str | None = None) -> dict[str, str]:
    root, common = discover_migrated_root(worktree)
    target = target_sha or exact_live_main(common, root)
    ensure_local_commit(common, root, target)
    if not checked_out_ref(common, "refs/heads/main"):
        require_fast_forwardable(common, root, "refs/heads/main", target)
    require_fast_forwardable(common, root, "refs/remotes/origin/main", target)
    view, view_git_dir = current_view(root, common)
    if view is not None and view_git_dir is not None:
        status = git(view_git_dir, view, "status", "--porcelain=v1", work_tree=view)
        if status.returncode != 0 or status.stdout.strip():
            raise LayoutRefusal("clean repository view has local changes; refusing synchronization")
    return {"root": str(root), "target": target, "view": str(view) if view else "missing"}


def _sync_repository_view(worktree: Path, target_sha: str | None = None) -> dict[str, object]:
    root, common = discover_migrated_root(worktree)
    repaired_config = repair_obsolete_core_worktree(common, root, worktree)
    preflight = preflight_repository_view(worktree, target_sha)
    target = preflight["target"]
    view, _ = current_view(root, common)
    renamed = False
    if view == root / "Source":
        moved = git(common, root, "worktree", "move", str(view), str(root / "Repository"))
        if moved.returncode != 0:
            raise LayoutRefusal(
                "cannot rename Source to Repository: "
                + (moved.stderr.strip() or moved.stdout.strip())
            )
        view = root / "Repository"
        renamed = True
    elif view is None:
        view = root / "Repository"
        added = git(common, root, "worktree", "add", "--detach", str(view), target)
        if added.returncode != 0:
            raise LayoutRefusal(
                "cannot create clean Repository view: "
                + (added.stderr.strip() or added.stdout.strip())
            )

    view_git_dir = worktree_git_dir(view, common)
    switched = git(view_git_dir, view, "checkout", "--detach", target, work_tree=view)
    if switched.returncode != 0:
        raise LayoutRefusal(
            "cannot refresh clean Repository view: "
            + (switched.stderr.strip() or switched.stdout.strip())
        )
    old_remote = ref_sha(common, root, "refs/remotes/origin/main") or "0" * 40
    updates = [("refs/remotes/origin/main", target, old_remote)]
    main_checkouts = checked_out_ref(common, "refs/heads/main")
    local_main_status = "blocked_checked_out" if main_checkouts else "synchronized"
    if not main_checkouts:
        old_main = ref_sha(common, root, "refs/heads/main") or "0" * 40
        updates.append(("refs/heads/main", target, old_main))
    update_refs_transaction(common, root, updates)
    head = git(view_git_dir, view, "rev-parse", "HEAD", work_tree=view).stdout.strip()
    status = git(view_git_dir, view, "status", "--porcelain=v1", work_tree=view)
    if head != target or status.returncode != 0 or status.stdout.strip():
        raise LayoutRefusal("Repository view readback is not clean at the requested identity")
    write_workspace_guides(root)
    result: dict[str, object] = {
        "root": str(root),
        "repository": str(view),
        "target": target,
        "local_main": {
            "status": local_main_status,
            "sha": ref_sha(common, root, "refs/heads/main"),
            "checked_out_at": main_checkouts,
        },
        "origin_main": ref_sha(common, root, "refs/remotes/origin/main"),
        "renamed_source": renamed,
        "repaired_core_worktree": repaired_config,
        "status": "synchronized",
    }
    record_sync_attempt(root, result)
    return result


def sync_repository_view(
    worktree: Path,
    target_sha: str | None = None,
    *,
    already_locked: bool = False,
) -> dict[str, object]:
    root, _ = discover_migrated_root(worktree)
    if already_locked:
        return _sync_repository_view(worktree, target_sha)
    with workspace_lock(root):
        return _sync_repository_view(worktree, target_sha)


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


def atomic_text(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, raw = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    temporary = Path(raw)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8") as handle:
            handle.write(content)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


def write_workspace_guides(root: Path) -> None:
    atomic_text(
        root / "WORKSPACE.md",
        "# Optimus Agent workspace\n\n"
        "- `Repository/` is the complete reproducible GitHub repository: product source, "
        "tests, evaluation definitions, documentation, and build logic. It is a clean, "
        "read-only view of live GitHub `main`.\n"
        "- `Development/` contains agent worktrees, managed delivery records, build output, "
        "tools, raw evidence, caches, and the recoverable pre-migration snapshot.\n\n"
        "Coding agents must work in an assigned `Development/worktrees/*` checkout. They must "
        "not edit `Repository/` directly. The compatibility links `.git` and `local` keep older "
        "automation working while resolving into `Development/`.\n",
    )
    atomic_text(
        root / "Development" / "README.md",
        "# Optimus Agent Development\n\n"
        "This directory is machine-local and is not the GitHub source tree.\n\n"
        "- `git/`: shared bare Git control store\n"
        "- `worktrees/`: isolated coding-agent checkouts\n"
        "- `land/`: managed checkpoints, immutable receipts, locks, and gate evidence\n"
        "- `tools/`: repository-local development tools\n"
        "- `tmp/` and `t/`: raw or temporary evidence\n"
        "- `Archive/stale-root-snapshot/`: recoverable source-looking files removed from the "
        "old mixed root\n",
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
    repair_obsolete_core_worktree(git_dir, root, worktree)
    remote_main = git(git_dir, root, "show-ref", "--verify", "refs/remotes/origin/main")
    main_ref = "refs/remotes/origin/main" if remote_main.returncode == 0 else "refs/heads/main"
    repository = root / "Repository"
    added = git(git_dir, root, "worktree", "add", "--detach", str(repository), main_ref)
    if added.returncode != 0:
        raise LayoutRefusal(
            "layout moved safely but Repository creation failed: "
            + (added.stderr.strip() or added.stdout.strip())
        )
    write_workspace_guides(root)
    return {
        "root": str(root),
        "repository": str(repository),
        "development": str(development),
        "archived_entries": len(shadow_names),
        "repository_ref": main_ref,
    }


def report(worktree: Path) -> dict[str, object]:
    root, common = discover_legacy_root(worktree)
    shadow = preflight(root, common, worktree.resolve())
    return {
        "root": str(root),
        "planned_repository": str(root / "Repository"),
        "planned_development": str(root / "Development"),
        "source_shadow_entries": shadow,
        "compatibility_links": [".git -> Development/git", "local -> Development"],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("report", "apply", "sync"))
    args = parser.parse_args()
    try:
        if args.command == "report":
            result = report(Path.cwd())
        elif args.command == "apply":
            result = apply_layout(Path.cwd())
        else:
            result = sync_repository_view(Path.cwd())
    except LayoutRefusal as error:
        print(f"workspace layout refused: {error}")
        return 1
    for key, value in result.items():
        print(f"{key}: {value}")
    print("outcome: completed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
