#!/usr/bin/env python3
"""Repository-managed checkpoints, worktree restore, and verified delivery.

This is the only repository program permitted to create development history.
Every Git invocation is bound explicitly to the linked worktree which invoked
it; the canonical Optimus root is bare and its global ``core.worktree`` is not
safe discovery state.
"""

from __future__ import annotations

import argparse
import contextlib
import datetime as dt
import fcntl
import hashlib
import json
import os
import re
import secrets
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Iterator, Sequence


SCHEMA_VERSION = 1
SLUG = re.compile(r"^[a-z0-9][a-z0-9._-]{0,63}$")
MODEL = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._:/+-]{0,127}$")
EFFORTS = {"none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra"}
ZERO_OID = "0" * 40
MACHINE_NAME = "Optimus Managed Delivery"
MACHINE_EMAIL = "optimus-land@local"


class Refusal(RuntimeError):
    """A fail-closed managed-delivery refusal."""


@dataclass(frozen=True)
class GitResult:
    returncode: int
    stdout: str
    stderr: str


@dataclass(frozen=True)
class Repository:
    root: Path
    git_dir: Path
    common_dir: Path
    repo_root: Path
    state_dir: Path
    worktree_id: str
    branch: str

    @classmethod
    def discover(cls, start: Path | None = None) -> "Repository":
        root = (start or Path.cwd()).resolve()
        marker = root / ".git"
        if not marker.is_file():
            raise Refusal("run from the root of an assigned linked worktree")
        line = marker.read_text(encoding="utf-8").strip()
        prefix = "gitdir:"
        if not line.startswith(prefix):
            raise Refusal("the worktree .git pointer is malformed")
        raw_git_dir = line[len(prefix) :].strip()
        git_dir = Path(raw_git_dir)
        if not git_dir.is_absolute():
            git_dir = marker.parent / git_dir
        git_dir = git_dir.resolve()
        if not git_dir.is_dir():
            raise Refusal(f"linked Git directory does not exist: {git_dir}")

        commondir_file = git_dir / "commondir"
        if not commondir_file.is_file():
            raise Refusal("the checkout is not a registered linked worktree")
        common_dir = (git_dir / commondir_file.read_text(encoding="utf-8").strip()).resolve()
        if common_dir.name != ".git" or not common_dir.is_dir():
            raise Refusal("cannot resolve the canonical bare Git directory")
        repo_root = common_dir.parent.resolve()
        assigned_root = (repo_root / "local" / "worktrees").resolve()
        try:
            relative = root.relative_to(assigned_root)
        except ValueError as error:
            raise Refusal(f"worktree is outside the assigned root {assigned_root}") from error
        if not relative.parts:
            raise Refusal("the assigned worktree root itself is not a checkout")

        back_pointer = git_dir / "gitdir"
        if not back_pointer.is_file():
            raise Refusal("linked worktree registration has no back-pointer")
        registered_marker = Path(back_pointer.read_text(encoding="utf-8").strip()).resolve()
        if registered_marker != marker.resolve():
            raise Refusal("linked worktree registration points at a different checkout")

        provisional = cls(
            root=root,
            git_dir=git_dir,
            common_dir=common_dir,
            repo_root=repo_root,
            state_dir=repo_root / "local" / "land",
            worktree_id=hashlib.sha256(str(root).encode()).hexdigest()[:16],
            branch="",
        )
        branch = provisional.git(["symbolic-ref", "--quiet", "HEAD"]).stdout.strip()
        if not branch.startswith("refs/heads/"):
            raise Refusal("managed delivery requires a symbolic task branch")
        if branch == "refs/heads/main":
            raise Refusal("managed delivery may not run from the main worktree")
        if provisional.git(["ls-files", "-u"]).stdout:
            raise Refusal("the worktree has unmerged index entries")
        for sentinel in (
            "MERGE_HEAD",
            "CHERRY_PICK_HEAD",
            "REVERT_HEAD",
            "rebase-apply",
            "rebase-merge",
            "BISECT_LOG",
        ):
            if (git_dir / sentinel).exists() or (common_dir / sentinel).exists():
                raise Refusal(f"Git operation in progress: {sentinel}")
        return cls(
            root=root,
            git_dir=git_dir,
            common_dir=common_dir,
            repo_root=repo_root,
            state_dir=repo_root / "local" / "land",
            worktree_id=provisional.worktree_id,
            branch=branch,
        )

    def git(
        self,
        args: Sequence[str],
        *,
        check: bool = True,
        input_text: str | None = None,
        extra_env: dict[str, str] | None = None,
    ) -> GitResult:
        env = sanitized_env()
        if extra_env:
            env.update(extra_env)
        completed = subprocess.run(
            [
                "git",
                f"--git-dir={self.git_dir}",
                f"--work-tree={self.root}",
                *args,
            ],
            cwd=self.root,
            env=env,
            input=input_text,
            capture_output=True,
            text=True,
            check=False,
        )
        result = GitResult(completed.returncode, completed.stdout, completed.stderr)
        if check and result.returncode != 0:
            detail = result.stderr.strip() or result.stdout.strip() or "unknown Git failure"
            raise Refusal(f"git {' '.join(args[:2])} refused: {detail}")
        return result

    def snapshot_tree(self) -> str:
        tmp_dir = self.state_dir / "tmp"
        tmp_dir.mkdir(parents=True, exist_ok=True)
        descriptor, raw_path = tempfile.mkstemp(prefix="index-", dir=tmp_dir)
        os.close(descriptor)
        index = Path(raw_path)
        index.unlink()
        env = {"GIT_INDEX_FILE": str(index)}
        try:
            self.git(["read-tree", "HEAD"], extra_env=env)
            self.git(["add", "-A", "--", "."], extra_env=env)
            return self.git(["write-tree"], extra_env=env).stdout.strip()
        finally:
            index.unlink(missing_ok=True)
            index.with_suffix(index.suffix + ".lock").unlink(missing_ok=True)


def sanitized_env() -> dict[str, str]:
    return {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}


def now() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds")


def validate_slug(value: str, kind: str) -> str:
    if not SLUG.fullmatch(value) or ".." in value:
        raise Refusal(
            f"{kind} must match [a-z0-9][a-z0-9._-]{{0,63}} and may not contain '..'"
        )
    return value


def validate_model(value: str) -> str:
    if not MODEL.fullmatch(value):
        raise Refusal("model is empty or contains unsafe characters")
    return value


def validate_effort(value: str) -> str:
    if value not in EFFORTS:
        raise Refusal(f"effort must be one of: {', '.join(sorted(EFFORTS))}")
    return value


@contextlib.contextmanager
def lock(path: Path) -> Iterator[None]:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a+", encoding="utf-8") as handle:
        fcntl.flock(handle.fileno(), fcntl.LOCK_EX)
        try:
            yield
        finally:
            fcntl.flock(handle.fileno(), fcntl.LOCK_UN)


def atomic_json(path: Path, payload: dict[str, object], *, exclusive: bool = False) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if exclusive and path.exists():
        raise Refusal(f"immutable receipt already exists: {path}")
    temporary = path.with_name(f".{path.name}.{os.getpid()}.{secrets.token_hex(4)}.tmp")
    data = (json.dumps(payload, indent=2, sort_keys=True) + "\n").encode()
    descriptor = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    try:
        with os.fdopen(descriptor, "wb") as handle:
            handle.write(data)
            handle.flush()
            os.fsync(handle.fileno())
        if exclusive and path.exists():
            temporary.unlink(missing_ok=True)
            raise Refusal(f"immutable receipt already exists: {path}")
        os.replace(temporary, path)
        directory_fd = os.open(path.parent, os.O_RDONLY)
        try:
            os.fsync(directory_fd)
        finally:
            os.close(directory_fd)
    finally:
        temporary.unlink(missing_ok=True)


def commit_tree(repo: Repository, tree: str, parent: str, message: str, timestamp: str) -> str:
    identity = {
        "GIT_AUTHOR_NAME": MACHINE_NAME,
        "GIT_AUTHOR_EMAIL": MACHINE_EMAIL,
        "GIT_COMMITTER_NAME": MACHINE_NAME,
        "GIT_COMMITTER_EMAIL": MACHINE_EMAIL,
        "GIT_AUTHOR_DATE": timestamp,
        "GIT_COMMITTER_DATE": timestamp,
    }
    return repo.git(
        ["commit-tree", tree, "-p", parent],
        input_text=message,
        extra_env=identity,
    ).stdout.strip()


def ref_oid(repo: Repository, reference: str) -> str | None:
    result = repo.git(["rev-parse", "--verify", "--quiet", reference], check=False)
    return result.stdout.strip() if result.returncode == 0 else None


def checkpoint_ref(repo: Repository, label: str) -> str:
    return f"refs/optimus/checkpoints/{repo.worktree_id}/{label}"


def _create_checkpoint(
    repo: Repository,
    label: str,
    *,
    kind: str,
    tree: str | None = None,
) -> dict[str, object]:
    tree = tree or repo.snapshot_tree()
    reference = checkpoint_ref(repo, label)
    existing = ref_oid(repo, reference)
    if existing:
        existing_tree = repo.git(["show", "-s", "--format=%T", existing]).stdout.strip()
        if existing_tree != tree:
            raise Refusal(f"checkpoint label already names different progress: {label}")
        commit = existing
    else:
        parent = repo.git(["rev-parse", "HEAD"]).stdout.strip()
        created_at = now()
        message = (
            "checkpoint: managed worktree snapshot\n\n"
            f"Label: {label}\n"
            f"Worktree: {repo.worktree_id}\n"
            f"Kind: {kind}\n"
        )
        commit = commit_tree(repo, tree, parent, message, created_at)
        repo.git(["update-ref", "--create-reflog", reference, commit, ZERO_OID])
    payload: dict[str, object] = {
        "schema": SCHEMA_VERSION,
        "kind": kind,
        "label": label,
        "worktree_id": repo.worktree_id,
        "worktree": str(repo.root),
        "branch": repo.branch,
        "commit": commit,
        "tree": tree,
        "created_at": now(),
        "ignored_files_included": False,
    }
    receipt = repo.state_dir / "checkpoints" / repo.worktree_id / f"{label}.json"
    if not receipt.exists():
        atomic_json(receipt, payload, exclusive=True)
    return payload


def checkpoint(repo: Repository, label: str) -> dict[str, object]:
    validate_slug(label, "checkpoint label")
    with lock(repo.state_dir / "locks" / f"worktree-{repo.worktree_id}.lock"):
        result = _create_checkpoint(repo, label, kind="user")
    print(f"checkpoint {label}: {result['commit']}")
    print("ignored files were not included")
    return result


def _restore_tree(repo: Repository, target_tree: str, safety_tree: str) -> None:
    current_tree = repo.snapshot_tree()
    if current_tree != safety_tree:
        raise Refusal("worktree changed while preparing undo")
    try:
        # Make non-ignored untracked paths Git-known before the update so paths
        # absent from the target are removed without a broad filesystem clean.
        repo.git(["read-tree", "--reset", current_tree])
        repo.git(["read-tree", "--reset", "-u", target_tree])
        if repo.snapshot_tree() != target_tree:
            raise Refusal("restored worktree does not match the checkpoint tree")
    except Exception:
        repo.git(["read-tree", "--reset", "-u", safety_tree], check=False)
        raise


def undo(repo: Repository, label: str) -> dict[str, object]:
    validate_slug(label, "checkpoint label")
    with lock(repo.state_dir / "locks" / f"worktree-{repo.worktree_id}.lock"):
        reference = checkpoint_ref(repo, label)
        commit = ref_oid(repo, reference)
        if not commit:
            raise Refusal(f"checkpoint does not exist in this worktree: {label}")
        target_tree = repo.git(["show", "-s", "--format=%T", commit]).stdout.strip()
        safety_tree = repo.snapshot_tree()
        safety_label = (
            f"before-undo-{dt.datetime.now(dt.timezone.utc).strftime('%Y%m%d%H%M%S')}-"
            f"{secrets.token_hex(3)}"
        )
        safety = _create_checkpoint(
            repo, safety_label, kind="automatic-before-undo", tree=safety_tree
        )
        head_before = repo.git(["rev-parse", "HEAD"]).stdout.strip()
        _restore_tree(repo, target_tree, safety_tree)
        if repo.git(["rev-parse", "HEAD"]).stdout.strip() != head_before:
            repo.git(["read-tree", "--reset", "-u", safety_tree], check=False)
            raise Refusal("undo attempted to move HEAD; safety state restored")
    result: dict[str, object] = {
        "checkpoint": label,
        "tree": target_tree,
        "safety_checkpoint": safety["label"],
        "head": head_before,
    }
    print(f"restored checkpoint {label}")
    print(f"safety checkpoint: {safety['label']}")
    return result


def remote_main(repo: Repository) -> str:
    result = repo.git(
        ["ls-remote", "--exit-code", "origin", "refs/heads/main"], check=False
    )
    if result.returncode != 0 or not result.stdout.strip():
        detail = result.stderr.strip() or "origin has no refs/heads/main"
        raise Refusal(f"cannot resolve remote main: {detail}")
    return result.stdout.split()[0]


def ensure_commit_present(repo: Repository, oid: str) -> None:
    present = repo.git(["cat-file", "-e", f"{oid}^{{commit}}"], check=False)
    if present.returncode == 0:
        return
    repo.git(["fetch", "--no-tags", "origin", oid])


def is_ancestor(repo: Repository, ancestor: str, descendant: str) -> bool:
    ensure_commit_present(repo, ancestor)
    ensure_commit_present(repo, descendant)
    return (
        repo.git(["merge-base", "--is-ancestor", ancestor, descendant], check=False).returncode
        == 0
    )


def changed_paths(repo: Repository, base: str, tree: str) -> list[str]:
    output = repo.git(
        ["diff", "--name-only", "--no-renames", f"{base}^{{tree}}", tree]
    ).stdout
    return sorted(line for line in output.splitlines() if line)


def seams_for(paths: Sequence[str]) -> list[str]:
    seams: set[str] = set()
    for raw in paths:
        parts = Path(raw).parts
        if len(parts) >= 2 and parts[0] in {"apps", "crates"}:
            seams.add("/".join(parts[:2]))
        elif parts:
            seams.add(parts[0])
    return sorted(seams)


def symbols_for(repo: Repository, base: str, tree: str, paths: Sequence[str]) -> list[str]:
    symbols: list[str] = []
    current_path = ""
    diff = repo.git(
        ["diff", "--no-ext-diff", "--no-renames", "--unified=0", f"{base}^{{tree}}", tree]
    ).stdout
    for line in diff.splitlines():
        if line.startswith("+++ b/"):
            current_path = line[6:]
        elif line.startswith("@@"):
            header = line.split("@@", 2)[-1].strip()
            if current_path and header:
                value = f"{current_path}::{header}"
                if value not in symbols:
                    symbols.append(value)
    represented = {entry.split("::", 1)[0] for entry in symbols}
    for path in paths:
        if path not in represented:
            symbols.append(f"{path}::<module-scope>")
    return symbols[:16]


def bounded(values: Sequence[str], limit: int = 8) -> str:
    if not values:
        return "none"
    shown = list(values[:limit])
    if len(values) > limit:
        shown.append(f"+{len(values) - limit} more")
    return ", ".join(shown)


def commit_message(
    task_id: str,
    seams: Sequence[str],
    symbols: Sequence[str],
    model: str,
    effort: str,
) -> str:
    return (
        f"🔧 chore(delivery): {task_id}\n\n"
        f"Task: {task_id}\n"
        f"Seam: {bounded(seams)}\n"
        f"Symbols: {bounded(symbols)}\n"
        "Fixtures: full verification suite PASS\n"
        "Gates: just verify PASS (no skips)\n"
        f"Model: {model}\n"
        f"Effort: {effort}\n"
    )


def next_attempt(task_dir: Path) -> int:
    existing = []
    if task_dir.is_dir():
        for path in task_dir.glob("attempt-*.json"):
            try:
                existing.append(int(path.stem.split("-", 1)[1]))
            except ValueError:
                continue
    return max(existing, default=0) + 1


def run_verify(repo: Repository, evidence_path: Path) -> tuple[int, str]:
    verify = repo.root / "scripts" / "verify.sh"
    if not verify.is_file():
        raise Refusal("scripts/verify.sh is missing")
    env = sanitized_env()
    env["OPTIMUS_VERIFY_FORBID_SKIPS"] = "1"
    completed = subprocess.run(
        ["bash", str(verify), "all"],
        cwd=repo.root,
        env=env,
        capture_output=True,
        text=True,
        check=False,
    )
    output = completed.stdout + completed.stderr
    evidence_path.parent.mkdir(parents=True, exist_ok=True)
    evidence_path.write_text(output, encoding="utf-8")
    sys.stdout.write(output)
    return completed.returncode, hashlib.sha256(output.encode()).hexdigest()


def completed_receipt(
    repo: Repository, task_id: str, model: str, effort: str
) -> dict[str, object] | None:
    receipt_path = repo.state_dir / "tasks" / task_id / "receipt.json"
    if not receipt_path.is_file():
        return None
    receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
    provenance = receipt.get("provenance", {})
    if provenance.get("model") != model or provenance.get("effort") != effort:
        raise Refusal("landed task id is immutable and has different provenance")
    commit = str(receipt["commit"]["sha"])
    remote = remote_main(repo)
    if remote != commit and not is_ancestor(repo, commit, remote):
        raise Refusal("task receipt exists but its commit is not on remote main")
    print(f"task {task_id} already landed: {commit}")
    return receipt


def land(repo: Repository, task_id: str, model: str, effort: str) -> dict[str, object]:
    validate_slug(task_id, "task id")
    validate_model(model)
    validate_effort(effort)
    prior = completed_receipt(repo, task_id, model, effort)
    if prior is not None:
        return prior

    task_dir = repo.state_dir / "tasks" / task_id
    attempt = next_attempt(task_dir)
    base = remote_main(repo)
    ensure_commit_present(repo, base)
    head = repo.git(["rev-parse", "HEAD"]).stdout.strip()
    if not is_ancestor(repo, base, head):
        raise Refusal(
            "remote main advanced outside this task branch; managed delivery cannot "
            "merge or rebase it"
        )

    tree = repo.snapshot_tree()
    remote_tree = repo.git(["rev-parse", f"{base}^{{tree}}"]).stdout.strip()
    if tree == remote_tree:
        raise Refusal("candidate tree has no changes relative to remote main")
    paths = changed_paths(repo, base, tree)
    seams = seams_for(paths)
    symbols = symbols_for(repo, base, tree, paths)

    with lock(repo.state_dir / "locks" / f"worktree-{repo.worktree_id}.lock"):
        if repo.snapshot_tree() != tree:
            raise Refusal("worktree changed before the pre-land checkpoint")
        preland_label = f"pre-land-{task_id}-{attempt}"
        _create_checkpoint(repo, preland_label, kind="automatic-before-land", tree=tree)

    evidence = repo.state_dir / "evidence" / task_id / f"attempt-{attempt}-verify.log"
    status, output_digest = run_verify(repo, evidence)
    if status != 0:
        failure = {
            "schema": SCHEMA_VERSION,
            "task_id": task_id,
            "attempt": attempt,
            "state": "refused_gate",
            "base": base,
            "tree": tree,
            "gate": {
                "command": "bash scripts/verify.sh all",
                "status": status,
                "output_sha256": output_digest,
                "evidence": str(evidence),
            },
            "provenance": {"model": model, "effort": effort, "verification": "caller-attested"},
            "recorded_at": now(),
        }
        atomic_json(task_dir / f"attempt-{attempt}.json", failure, exclusive=True)
        raise Refusal(f"just verify failed; evidence: {evidence}")
    if repo.snapshot_tree() != tree:
        raise Refusal("tracked worktree content changed during verification")

    message = commit_message(task_id, seams, symbols, model, effort)
    passed_record: dict[str, object] = {
        "schema": SCHEMA_VERSION,
        "task_id": task_id,
        "attempt": attempt,
        "state": "verified",
        "worktree_id": repo.worktree_id,
        "branch": repo.branch,
        "base": base,
        "tree": tree,
        "changed_paths": paths,
        "seams": seams,
        "symbols": symbols,
        "gate": {
            "command": "bash scripts/verify.sh all",
            "status": 0,
            "skips_forbidden": True,
            "output_sha256": output_digest,
            "evidence": str(evidence),
        },
        "fixtures": [{"id": "full-verification-suite", "status": "pass"}],
        "provenance": {"model": model, "effort": effort, "verification": "caller-attested"},
        "commit_message": message,
        "recorded_at": now(),
    }
    attempt_path = task_dir / f"attempt-{attempt}.json"
    atomic_json(attempt_path, passed_record, exclusive=True)

    with lock(repo.state_dir / "locks" / "land.lock"):
        current_remote = remote_main(repo)
        if current_remote != base:
            raise Refusal(
                f"remote main changed during verification: expected {base}, got {current_remote}"
            )
        if repo.snapshot_tree() != tree:
            raise Refusal("worktree changed before commit creation")
        head_now = repo.git(["rev-parse", "HEAD"]).stdout.strip()
        if head_now != head:
            raise Refusal("task branch moved during verification")

        candidate_ref = (
            f"refs/optimus/land-candidates/{repo.worktree_id}/{task_id}"
        )
        candidate = ref_oid(repo, candidate_ref)
        if candidate:
            candidate_tree = repo.git(["show", "-s", "--format=%T", candidate]).stdout.strip()
            candidate_parent = repo.git(["show", "-s", "--format=%P", candidate]).stdout.strip()
            candidate_message = repo.git(["show", "-s", "--format=%B", candidate]).stdout
            if (
                candidate_tree != tree
                or candidate_parent != base
                or candidate_message.strip() != message.strip()
            ):
                candidate = None
        if candidate is None:
            candidate = commit_tree(repo, tree, base, message, passed_record["recorded_at"])
            old_candidate = ref_oid(repo, candidate_ref) or ZERO_OID
            repo.git(
                [
                    "update-ref",
                    "--create-reflog",
                    candidate_ref,
                    candidate,
                    old_candidate,
                ]
            )

        push = repo.git(
            [
                "push",
                "--porcelain",
                "--no-verify",
                "origin",
                f"{candidate}:refs/heads/main",
            ],
            check=False,
        )
        if push.returncode != 0:
            failed = dict(passed_record)
            failed["state"] = "refused_push"
            failed["candidate_commit"] = candidate
            failed["push_error"] = push.stderr.strip() or push.stdout.strip()
            atomic_json(
                task_dir / f"push-failure-{attempt}.json", failed, exclusive=True
            )
            raise Refusal(f"non-force push to main refused: {failed['push_error']}")

        readback = remote_main(repo)
        landed_state = "landed" if readback == candidate else "landed_remote_advanced"
        if readback != candidate:
            ensure_commit_present(repo, readback)
            if not is_ancestor(repo, candidate, readback):
                landed_state = "landed_readback_conflict"

        receipt = dict(passed_record)
        receipt["state"] = landed_state
        receipt["commit"] = {
            "sha": candidate,
            "parent": base,
            "remote_readback": readback,
            "push_output": push.stdout.strip(),
        }
        receipt["landed_at"] = now()
        atomic_json(task_dir / "receipt.json", receipt, exclusive=True)

        # Move only the assigned task branch. Local refs/heads/main may be
        # checked out in another worktree and is deliberately untouched.
        repo.git(["update-ref", repo.branch, candidate, head])
        repo.git(["read-tree", "--reset", candidate])
        repo.git(["update-ref", "refs/remotes/origin/main", readback])

    if receipt["state"] != "landed":
        raise Refusal(
            f"push was accepted as {candidate}, but remote readback is {readback}; "
            f"immutable receipt state is {receipt['state']}"
        )
    print(f"landed {task_id}: {candidate}")
    print(f"remote main: {readback}")
    return receipt


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    subcommands = result.add_subparsers(dest="command", required=True)
    checkpoint_parser = subcommands.add_parser("checkpoint")
    checkpoint_parser.add_argument("label")
    undo_parser = subcommands.add_parser("undo")
    undo_parser.add_argument("label")
    land_parser = subcommands.add_parser("land")
    land_parser.add_argument("task_id")
    land_parser.add_argument("--model", required=True)
    land_parser.add_argument("--effort", required=True)
    return result


def main(argv: Sequence[str] | None = None) -> int:
    args = parser().parse_args(argv)
    try:
        repo = Repository.discover()
        if args.command == "checkpoint":
            checkpoint(repo, args.label)
        elif args.command == "undo":
            undo(repo, args.label)
        else:
            land(repo, args.task_id, args.model, args.effort)
        return 0
    except Refusal as error:
        print(f"managed delivery refused: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
