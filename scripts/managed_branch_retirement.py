#!/usr/bin/env python3
"""Plan and atomically retire verified remote branches while protecting main."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from dataclasses import dataclass
from typing import Mapping, Sequence

import managed_delivery as delivery


SCHEMA_VERSION = 1
OID = re.compile(r"^[0-9a-f]{40}$")
REASON = re.compile(r"^[A-Za-z0-9][A-Za-z0-9 ._:/+@#=-]{0,199}$")


@dataclass(frozen=True)
class RemoteBranch:
    name: str
    sha: str


def validate_branch_name(branch: str) -> str:
    forbidden = (" ", "~", "^", ":", "?", "*", "[", "\\")
    if (
        not branch
        or branch.startswith(("-", "/"))
        or branch.endswith(("/", ".", ".lock"))
        or ".." in branch
        or "//" in branch
        or "@{" in branch
        or any(character in branch for character in forbidden)
        or any(ord(character) < 32 or ord(character) == 127 for character in branch)
    ):
        raise delivery.Refusal(f"unsafe remote branch name: {branch}")
    return branch


def parse_superseded(raw: str) -> dict[str, str]:
    try:
        value = json.loads(raw)
    except json.JSONDecodeError as error:
        raise delivery.Refusal(f"superseded map is not valid JSON: {error}") from error
    if not isinstance(value, dict):
        raise delivery.Refusal("superseded map must be a JSON object")
    parsed: dict[str, str] = {}
    for branch, reason in value.items():
        if not isinstance(branch, str) or not isinstance(reason, str):
            raise delivery.Refusal("superseded branch names and reasons must be strings")
        validate_branch_name(branch)
        if branch == "main":
            raise delivery.Refusal("main can never be classified as superseded")
        if not REASON.fullmatch(reason):
            raise delivery.Refusal(f"unsafe or empty supersession reason for {branch}")
        parsed[branch] = reason
    return parsed


def remote_heads(repo: delivery.Repository) -> list[RemoteBranch]:
    result = repo.git(["ls-remote", "--heads", "origin"])
    branches: list[RemoteBranch] = []
    for line in result.stdout.splitlines():
        fields = line.split()
        if len(fields) != 2 or not fields[1].startswith("refs/heads/"):
            raise delivery.Refusal(f"unexpected ls-remote row: {line}")
        sha, reference = fields
        name = validate_branch_name(reference.removeprefix("refs/heads/"))
        if not OID.fullmatch(sha):
            raise delivery.Refusal(f"unexpected remote object id for {name}")
        branches.append(RemoteBranch(name=name, sha=sha))
    branches.sort(key=lambda branch: branch.name)
    if not branches:
        raise delivery.Refusal("origin has no remote branches")
    if sum(branch.name == "main" for branch in branches) != 1:
        raise delivery.Refusal("origin must have exactly one main branch")
    return branches


def is_ancestor(repo: delivery.Repository, ancestor: str, descendant: str) -> bool:
    result = repo.git(
        ["merge-base", "--is-ancestor", ancestor, descendant],
        check=False,
    )
    if result.returncode == 0:
        return True
    if result.returncode == 1:
        return False
    detail = result.stderr.strip() or result.stdout.strip() or "unknown ancestry failure"
    raise delivery.Refusal(f"cannot prove branch ancestry: {detail}")


def canonical_bytes(payload: Mapping[str, object]) -> bytes:
    return json.dumps(
        payload,
        ensure_ascii=True,
        separators=(",", ":"),
        sort_keys=True,
    ).encode()


def plan(
    repo: delivery.Repository,
    superseded: Mapping[str, str],
) -> tuple[dict[str, object], str]:
    branches = remote_heads(repo)
    by_name = {branch.name: branch for branch in branches}
    unknown = sorted(set(superseded) - set(by_name))
    if unknown:
        raise delivery.Refusal(
            f"superseded map names branches that do not exist: {', '.join(unknown)}"
        )
    main = by_name["main"]
    retirements: list[dict[str, str]] = []
    unresolved: list[str] = []
    for branch in branches:
        if branch.name == "main":
            continue
        if is_ancestor(repo, branch.sha, main.sha):
            disposition = "contained-in-main"
            reason = f"ancestor-of:{main.sha}"
        elif branch.name in superseded:
            disposition = "verified-superseded"
            reason = superseded[branch.name]
        else:
            unresolved.append(branch.name)
            continue
        retirements.append(
            {
                "branch": branch.name,
                "sha": branch.sha,
                "disposition": disposition,
                "reason": reason,
            }
        )
    if unresolved:
        raise delivery.Refusal(
            "remote branches are neither contained nor explicitly superseded: "
            + ", ".join(unresolved)
        )
    if not retirements:
        raise delivery.Refusal("origin already contains only main")
    remote_url = repo.git(["remote", "get-url", "origin"]).stdout.strip()
    payload: dict[str, object] = {
        "schema": SCHEMA_VERSION,
        "remote_url_sha256": hashlib.sha256(remote_url.encode()).hexdigest(),
        "protected": {"branch": "main", "sha": main.sha},
        "retirements": retirements,
    }
    digest = hashlib.sha256(canonical_bytes(payload)).hexdigest()
    return payload, digest


def execute(
    repo: delivery.Repository,
    expected_digest: str,
    superseded: Mapping[str, str],
) -> dict[str, object]:
    if not re.fullmatch(r"[0-9a-f]{64}", expected_digest):
        raise delivery.Refusal("plan digest must be 64 lowercase hexadecimal characters")
    receipt_path = repo.state_dir / "branch-retirements" / f"{expected_digest}.json"
    if receipt_path.is_file():
        existing = json.loads(receipt_path.read_text(encoding="utf-8"))
        remaining = [branch.name for branch in remote_heads(repo) if branch.name != "main"]
        if remaining:
            raise delivery.Refusal(
                "retirement receipt exists but remote branches remain: "
                + ", ".join(remaining)
            )
        print(f"branches already retired: {expected_digest}")
        return existing

    with delivery.lock(repo.state_dir / "locks" / "land.lock"):
        payload, actual_digest = plan(repo, superseded)
        if actual_digest != expected_digest:
            raise delivery.Refusal(
                f"remote branch plan changed: expected {expected_digest}, got {actual_digest}"
            )
        retirements = payload["retirements"]
        assert isinstance(retirements, list)
        leases: list[str] = []
        deletions: list[str] = []
        for item in retirements:
            assert isinstance(item, dict)
            branch = str(item["branch"])
            sha = str(item["sha"])
            leases.append(f"--force-with-lease=refs/heads/{branch}:{sha}")
            deletions.append(f":refs/heads/{branch}")
        result = repo.git(
            ["push", "--atomic", "--porcelain", *leases, "origin", *deletions]
        )
        remaining = [branch.name for branch in remote_heads(repo) if branch.name != "main"]
        if remaining:
            raise delivery.Refusal(
                "atomic retirement returned success but branches remain: "
                + ", ".join(remaining)
            )
        protected = payload["protected"]
        assert isinstance(protected, dict)
        receipt: dict[str, object] = {
            **payload,
            "kind": "remote-branch-retirement",
            "plan_sha256": expected_digest,
            "retired_at": delivery.now(),
            "push_output": result.stdout.strip(),
            "state": "retired",
        }
        delivery.atomic_json(receipt_path, receipt, exclusive=True)
    print(f"retired {len(retirements)} remote branches atomically")
    print(f"protected main: {protected['sha']}")
    return receipt


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    subcommands = result.add_subparsers(dest="command", required=True)
    plan_parser = subcommands.add_parser("plan")
    plan_parser.add_argument("--superseded-json", default="{}")
    execute_parser = subcommands.add_parser("execute")
    execute_parser.add_argument("plan_sha256")
    execute_parser.add_argument("--superseded-json", default="{}")
    return result


def main(argv: Sequence[str] | None = None) -> int:
    args = parser().parse_args(argv)
    try:
        repo = delivery.Repository.discover()
        superseded = parse_superseded(args.superseded_json)
        if args.command == "plan":
            payload, digest = plan(repo, superseded)
            print(json.dumps(payload, indent=2, sort_keys=True))
            print(f"plan sha256: {digest}")
        else:
            execute(repo, args.plan_sha256, superseded)
        return 0
    except delivery.Refusal as error:
        print(f"managed branch retirement refused: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
