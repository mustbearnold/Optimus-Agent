#!/usr/bin/env python3
"""Recoverably retire stale Optimus worktrees through a reviewed exact-state plan."""

from __future__ import annotations

import argparse
import contextlib
import fcntl
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any, Iterator


class Refusal(RuntimeError):
    """Retirement cannot preserve or prove the requested scope."""


def run(common: Path, cwd: Path, *args: str, input_text: str | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", f"--git-dir={common}", *args], cwd=cwd,
        env={key: value for key, value in os.environ.items() if not key.startswith("GIT_")},
        input=input_text, text=True, capture_output=True, check=False,
    )


def canonical(payload: Any) -> str:
    return json.dumps(payload, indent=2, sort_keys=True) + "\n"


def discover(caller: Path) -> tuple[Path, Path, Path]:
    caller = caller.resolve()
    marker = caller / ".git"
    if not marker.is_file():
        raise Refusal("run from an assigned linked worktree")
    git_dir = Path(marker.read_text().strip().removeprefix("gitdir: ").strip()).resolve()
    common = (git_dir / (git_dir / "commondir").read_text().strip()).resolve()
    if common.name != "git" or common.parent.name != "Development":
        raise Refusal("workspace is not the managed Repository/Development layout")
    root = common.parent.parent.resolve()
    try:
        caller.relative_to((root / "Development" / "worktrees").resolve())
    except ValueError as error:
        raise Refusal("caller is outside the assigned-worktree root") from error
    return root, common, caller


@contextlib.contextmanager
def lock(root: Path) -> Iterator[None]:
    path = root / "Development" / "land" / "locks" / "land.lock"
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a+", encoding="utf-8") as handle:
        fcntl.flock(handle.fileno(), fcntl.LOCK_EX)
        try:
            yield
        finally:
            fcntl.flock(handle.fileno(), fcntl.LOCK_UN)


def worktree_blocks(common: Path, root: Path) -> list[dict[str, str]]:
    listed = run(common, root, "worktree", "list", "--porcelain")
    if listed.returncode != 0:
        raise Refusal(listed.stderr.strip())
    blocks: list[dict[str, str]] = []
    for raw in listed.stdout.strip().split("\n\n"):
        item: dict[str, str] = {}
        for line in raw.splitlines():
            key, _, value = line.partition(" ")
            item[key] = value
        if item:
            blocks.append(item)
    return blocks


def snapshot_tree(common: Path, path: Path, head: str, state_dir: Path) -> str:
    state_dir.mkdir(parents=True, exist_ok=True)
    descriptor, raw = tempfile.mkstemp(prefix="retire-index-", dir=state_dir)
    os.close(descriptor)
    index = Path(raw)
    index.unlink()
    environment = {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}
    environment["GIT_INDEX_FILE"] = str(index)
    try:
        for args in (("read-tree", head), ("add", "-A", "--", ".")):
            result = subprocess.run(
                ["git", f"--git-dir={common}", f"--work-tree={path}", *args], cwd=path,
                env=environment, text=True, capture_output=True, check=False,
            )
            if result.returncode != 0:
                raise Refusal(f"cannot snapshot {path}: {result.stderr.strip()}")
        result = subprocess.run(
            ["git", f"--git-dir={common}", f"--work-tree={path}", "write-tree"], cwd=path,
            env=environment, text=True, capture_output=True, check=False,
        )
        if result.returncode != 0:
            raise Refusal(f"cannot write snapshot tree for {path}: {result.stderr.strip()}")
        return result.stdout.strip()
    finally:
        index.unlink(missing_ok=True)


def build_plan(caller: Path) -> dict[str, Any]:
    root, common, caller = discover(caller)
    assigned = (root / "Development" / "worktrees").resolve()
    retire: list[dict[str, Any]] = []
    prunable: list[dict[str, str]] = []
    blocks = worktree_blocks(common, root)
    registered_paths: set[Path] = set()
    registration_heads: dict[str, str] = {}
    for item in blocks:
        shown = item.get("worktree", "")
        path = Path(shown).resolve(strict=False)
        registration_heads[Path(shown).name] = item.get("HEAD", "")
        if "prunable" in item:
            prunable.append({"registered": shown, "head": item.get("HEAD", "")})
            continue
        registered_paths.add(path)
        if path == caller or path == (root / "Repository").resolve() or "bare" in item:
            continue
        try:
            path.relative_to(assigned)
        except ValueError:
            continue
        if not (path / ".git").is_file():
            raise Refusal(f"registered assigned worktree is missing without prunable state: {path}")
        head = item.get("HEAD", "")
        tree = snapshot_tree(common, path, head, root / "Development" / "land" / "tmp")
        head_tree = run(common, root, "show", "-s", "--format=%T", head).stdout.strip()
        retire.append({
            "path": str(path), "registered": shown, "head": head,
            "branch": item.get("branch", "detached"), "tree": tree,
            "dirty": tree != head_tree,
        })
    for receipt_path in sorted(
        (root / "Development" / "land" / "worktree-retirements").glob("receipt-*.json")
    ):
        receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
        for item in [*receipt.get("pruned", []), *receipt.get("retired", [])]:
            shown = str(item.get("registered") or item.get("path") or "")
            if shown and item.get("head"):
                registration_heads.setdefault(Path(shown).name, str(item["head"]))
    orphan: list[dict[str, Any]] = []
    if assigned.is_dir():
        for path in sorted(assigned.iterdir(), key=lambda value: value.name):
            resolved = path.resolve(strict=False)
            if not path.is_dir() or resolved == caller or resolved in registered_paths:
                continue
            marker = path / ".git"
            registration = path.name
            if marker.is_file():
                line = marker.read_text(encoding="utf-8").strip()
                if line.startswith("gitdir: "):
                    registration = Path(line.removeprefix("gitdir: ").strip()).name
            head = registration_heads.get(registration)
            if not head:
                raise Refusal(f"orphan checkout has no recorded former HEAD: {path}")
            tree = snapshot_tree(common, path, head, root / "Development" / "land" / "tmp")
            head_tree = run(common, root, "show", "-s", "--format=%T", head).stdout.strip()
            orphan.append({
                "path": str(resolved), "registration": registration,
                "head": head, "tree": tree, "dirty": tree != head_tree,
            })
    plan = {
        "schema_version": 1,
        "caller": str(caller),
        "retire": sorted(retire, key=lambda value: value["path"]),
        "prunable": sorted(prunable, key=lambda value: value["registered"]),
        "orphan": sorted(orphan, key=lambda value: value["path"]),
    }
    digest = hashlib.sha256(canonical(plan).encode()).hexdigest()
    plan["sha256"] = digest
    return plan


def plan(caller: Path) -> dict[str, Any]:
    root, _, _ = discover(caller)
    payload = build_plan(caller)
    directory = root / "Development" / "land" / "worktree-retirements" / "plans"
    directory.mkdir(parents=True, exist_ok=True)
    path = directory / f"{payload['sha256']}.json"
    if not path.exists():
        path.write_text(canonical(payload), encoding="utf-8")
    return payload


def execute(caller: Path, digest: str) -> dict[str, Any]:
    if not re.fullmatch(r"[0-9a-f]{64}", digest):
        raise Refusal("plan digest must be a full SHA-256")
    root, common, caller = discover(caller)
    path = root / "Development" / "land" / "worktree-retirements" / "plans" / f"{digest}.json"
    if not path.is_file():
        raise Refusal("reviewed retirement plan does not exist")
    expected = json.loads(path.read_text(encoding="utf-8"))
    with lock(root):
        actual = build_plan(caller)
        if actual != expected:
            raise Refusal("worktree state changed after planning; generate a new plan")
        recovered: list[dict[str, str]] = []
        for item in [*expected["retire"], *expected.get("orphan", [])]:
            if item["dirty"]:
                message = (
                    "Optimus managed retired-worktree recovery\n\n"
                    f"Path: {item['path']}\n"
                    f"Branch: {item.get('branch', 'orphaned-registration')}\nPlan: {digest}\n"
                )
                created = run(
                    common, root,
                    "-c", "user.name=Optimus Managed Delivery",
                    "-c", "user.email=optimus-land@local",
                    "commit-tree", item["tree"], "-p", item["head"],
                    input_text=message,
                )
                if created.returncode != 0:
                    raise Refusal(f"cannot preserve {item['path']}: {created.stderr.strip()}")
                slug = Path(item["path"]).name.replace("_", "-")
                ref = f"refs/optimus/retired-worktrees/{slug}/{digest[:16]}"
                updated = run(common, root, "update-ref", ref, created.stdout.strip(), "0" * 40)
                if updated.returncode != 0:
                    raise Refusal(f"cannot record recovery ref for {item['path']}")
                recovered.append({"path": item["path"], "ref": ref, "commit": created.stdout.strip()})
        for item in expected.get("orphan", []):
            path_to_remove = Path(item["path"])
            try:
                path_to_remove.relative_to((root / "Development" / "worktrees").resolve())
            except ValueError as error:
                raise Refusal(f"orphan path escaped assigned root: {path_to_remove}") from error
            if path_to_remove == caller:
                raise Refusal("active caller appeared in orphan retirement plan")
            shutil.rmtree(path_to_remove)
        for item in expected["retire"]:
            removed = run(common, root, "worktree", "remove", "--force", item["registered"])
            if removed.returncode != 0:
                raise Refusal(f"cannot retire {item['path']}: {removed.stderr.strip()}")
        pruned = run(common, root, "worktree", "prune")
        if pruned.returncode != 0:
            raise Refusal(f"cannot prune dead registrations: {pruned.stderr.strip()}")
        receipt = {
            "schema_version": 1, "plan_sha256": digest,
            "retired": expected["retire"], "pruned": expected["prunable"],
            "orphaned_directories": expected.get("orphan", []),
            "recoveries": recovered,
        }
        receipt_path = root / "Development" / "land" / "worktree-retirements" / f"receipt-{digest}.json"
        receipt_path.write_text(canonical(receipt), encoding="utf-8")
        return receipt


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("plan")
    execute_parser = sub.add_parser("execute")
    execute_parser.add_argument("sha256")
    args = parser.parse_args()
    try:
        if args.command == "plan":
            result = plan(Path.cwd())
            print(canonical(result), end="")
        else:
            result = execute(Path.cwd(), args.sha256)
            print(
                f"WORKTREES_RETIRED count={len(result['retired'])} "
                f"orphans={len(result['orphaned_directories'])} pruned={len(result['pruned'])}"
            )
        return 0
    except Refusal as error:
        print(f"managed worktree retirement refused: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
