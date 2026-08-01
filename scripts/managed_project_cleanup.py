#!/usr/bin/env python3
"""Delete only structurally proven inactive generated output via an exact plan."""

from __future__ import annotations

import argparse
import contextlib
import fcntl
import hashlib
import json
import os
import re
import shutil
import sys
from pathlib import Path
from typing import Any, Iterator

import project_knowledge
import repository_ontology


ROOT = Path(__file__).resolve().parents[1]


class Refusal(RuntimeError):
    """Cleanup scope or state could not be proven exactly."""


def canonical(payload: Any) -> str:
    return json.dumps(payload, indent=2, sort_keys=True) + "\n"


def discover(root: Path = ROOT) -> tuple[Path, Path]:
    root = root.resolve(strict=True)
    wrapper = repository_ontology.workspace_root(root).resolve(strict=True)
    development = (wrapper / "Development").resolve(strict=True)
    try:
        root.relative_to((development / "worktrees").resolve(strict=True))
    except ValueError as error:
        raise Refusal("run from an assigned worktree in the managed workspace") from error
    return wrapper, development


def fingerprint(path: Path) -> dict[str, Any]:
    if path.is_symlink() or not path.is_dir():
        raise Refusal(f"cleanup candidate is not a real directory: {path}")
    digest = hashlib.sha256()
    entries = 0
    total = 0
    for current, dirs, files in os.walk(path, followlinks=False):
        base = Path(current)
        for name in sorted([*dirs, *files]):
            child = base / name
            relative = child.relative_to(path).as_posix()
            if child.is_symlink():
                stat = child.lstat()
                record = f"{relative}\0link\0{stat.st_mode}\0{stat.st_mtime_ns}\0{os.readlink(child)}\n"
                digest.update(record.encode())
                entries += 1
                continue
            stat = child.stat()
            record = f"{relative}\0{stat.st_mode}\0{stat.st_size}\0{stat.st_mtime_ns}\n"
            digest.update(record.encode())
            entries += 1
            if child.is_file():
                total += stat.st_size
    return {"entries": entries, "file_bytes": total, "metadata_sha256": digest.hexdigest()}


def build_plan(root: Path = ROOT) -> dict[str, Any]:
    wrapper, development = discover(root)
    observation = project_knowledge.workspace_observation(root)
    candidates: list[dict[str, Any]] = []
    prefix = "workspace://Development/"
    for item in observation.get("inactive_generated_paths", []):
        uri = str(item["path"])
        if not uri.startswith(prefix):
            raise Refusal(f"generated candidate is outside Development: {uri}")
        relative = uri.removeprefix(prefix)
        path = (development / relative).resolve(strict=True)
        try:
            path.relative_to(development)
        except ValueError as error:
            raise Refusal(f"generated candidate escapes Development: {uri}") from error
        candidates.append({
            "uri": uri, "path": str(path), "proof": item["proof"],
            "fingerprint": fingerprint(path),
        })
    payload = {
        "schema_version": 1, "wrapper": str(wrapper),
        "candidates": sorted(candidates, key=lambda item: item["path"]),
    }
    payload["sha256"] = hashlib.sha256(canonical(payload).encode()).hexdigest()
    return payload


@contextlib.contextmanager
def lock(development: Path) -> Iterator[None]:
    path = development / "land" / "locks" / "land.lock"
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a+", encoding="utf-8") as handle:
        fcntl.flock(handle.fileno(), fcntl.LOCK_EX)
        try:
            yield
        finally:
            fcntl.flock(handle.fileno(), fcntl.LOCK_UN)


def plan(root: Path = ROOT) -> dict[str, Any]:
    _, development = discover(root)
    payload = build_plan(root)
    directory = development / "land" / "project-cleanups" / "plans"
    directory.mkdir(parents=True, exist_ok=True)
    path = directory / f"{payload['sha256']}.json"
    if not path.exists():
        path.write_text(canonical(payload), encoding="utf-8")
    return payload


def execute(digest: str, root: Path = ROOT) -> dict[str, Any]:
    if not re.fullmatch(r"[0-9a-f]{64}", digest):
        raise Refusal("plan digest must be a full SHA-256")
    _, development = discover(root)
    plan_path = development / "land" / "project-cleanups" / "plans" / f"{digest}.json"
    if not plan_path.is_file():
        raise Refusal("reviewed cleanup plan does not exist")
    expected = json.loads(plan_path.read_text(encoding="utf-8"))
    with lock(development):
        actual = build_plan(root)
        if actual != expected:
            raise Refusal("cleanup state changed after planning; generate a new plan")
        removed: list[dict[str, Any]] = []
        for item in expected["candidates"]:
            path = Path(item["path"])
            try:
                path.relative_to(development)
            except ValueError as error:
                raise Refusal(f"cleanup path escaped Development: {path}") from error
            shutil.rmtree(path)
            removed.append(item)
        receipt = {
            "schema_version": 1, "plan_sha256": digest, "removed": removed,
            "outcome": "completed",
        }
        receipt_path = development / "land" / "project-cleanups" / f"receipt-{digest}.json"
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
            print(canonical(plan()), end="")
        else:
            result = execute(args.sha256)
            print(f"PROJECT_CLEANUP_COMPLETE paths={len(result['removed'])}")
        return 0
    except (OSError, Refusal, project_knowledge.KnowledgeError) as error:
        print(f"managed project cleanup refused: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
