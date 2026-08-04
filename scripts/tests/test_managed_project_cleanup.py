#!/usr/bin/env python3


"""Regression tests for exact-plan inactive generated cleanup."""


from __future__ import annotations


import pathlib, sys
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / "tools"))
import tempfile
from pathlib import Path
from unittest import mock

import managed_project_cleanup


def fixture() -> tuple[tempfile.TemporaryDirectory[str], Path, Path, Path]:
    temporary = tempfile.TemporaryDirectory()
    wrapper = Path(temporary.name)
    development = wrapper / "Development"
    root = development / "worktrees" / "assigned"
    candidate = development / "tmp" / "cargo-target-fixture"
    root.mkdir(parents=True)
    candidate.mkdir(parents=True)
    (candidate / ".rustc_info.json").write_text("{}\n", encoding="utf-8")
    (candidate / "debug").mkdir()
    (candidate / "debug" / "object").write_bytes(b"object")
    return temporary, wrapper, root, candidate


def observation(candidate: Path) -> dict[str, object]:
    return {
        "inactive_generated_paths": [{
            "path": "workspace://Development/tmp/cargo-target-fixture",
            "proof": "fixture", "bytes": 8,
        }]
    }


def test_execute_removes_only_unchanged_exact_plan() -> None:
    temporary, wrapper, root, candidate = fixture()
    try:
        with (
            mock.patch.object(managed_project_cleanup.repository_ontology, "workspace_root", return_value=wrapper),
            mock.patch.object(managed_project_cleanup.project_knowledge, "workspace_observation", return_value=observation(candidate)),
        ):
            payload = managed_project_cleanup.plan(root)
            receipt = managed_project_cleanup.execute(payload["sha256"], root)
        assert receipt["outcome"] == "completed"
        assert not candidate.exists()
        assert root.exists()
    finally:
        temporary.cleanup()


def test_execute_refuses_changed_candidate() -> None:
    temporary, wrapper, root, candidate = fixture()
    try:
        with (
            mock.patch.object(managed_project_cleanup.repository_ontology, "workspace_root", return_value=wrapper),
            mock.patch.object(managed_project_cleanup.project_knowledge, "workspace_observation", return_value=observation(candidate)),
        ):
            payload = managed_project_cleanup.plan(root)
            (candidate / "debug" / "new").write_bytes(b"changed")
            try:
                managed_project_cleanup.execute(payload["sha256"], root)
            except managed_project_cleanup.Refusal:
                pass
            else:
                raise AssertionError("changed cleanup target was accepted")
        assert candidate.exists()
    finally:
        temporary.cleanup()


def test_fingerprint_records_symlinks_without_following() -> None:
    temporary, _wrapper, _root, candidate = fixture()
    try:
        link = candidate / "escape"
        link.symlink_to(Path(temporary.name) / "outside")
        first = managed_project_cleanup.fingerprint(candidate)
        link.unlink()
        link.symlink_to(Path(temporary.name) / "elsewhere")
        second = managed_project_cleanup.fingerprint(candidate)
        assert first["metadata_sha256"] != second["metadata_sha256"]
        assert first["file_bytes"] == second["file_bytes"] == 9
        assert first["entries"] == second["entries"] == 4
    finally:
        temporary.cleanup()


def test_fingerprint_refuses_symlink_candidate_root() -> None:
    temporary, _wrapper, _root, candidate = fixture()
    try:
        alias = candidate.parent / "alias"
        alias.symlink_to(candidate)
        try:
            managed_project_cleanup.fingerprint(alias)
        except managed_project_cleanup.Refusal:
            pass
        else:
            raise AssertionError("symlink candidate root was accepted")
    finally:
        temporary.cleanup()


def test_execute_removes_symlink_entries_without_following() -> None:
    temporary, wrapper, root, candidate = fixture()
    try:
        outside_file = Path(temporary.name) / "outside-file"
        outside_file.write_bytes(b"keep")
        outside_dir = Path(temporary.name) / "outside-dir"
        outside_dir.mkdir()
        (outside_dir / "kept").write_bytes(b"keep")
        (candidate / "bin").mkdir()
        (candidate / "bin" / "tool").symlink_to(outside_file)
        (candidate / "linked-dir").symlink_to(outside_dir)
        (candidate / "dangling").symlink_to(Path(temporary.name) / "missing")
        with (
            mock.patch.object(managed_project_cleanup.repository_ontology, "workspace_root", return_value=wrapper),
            mock.patch.object(managed_project_cleanup.project_knowledge, "workspace_observation", return_value=observation(candidate)),
        ):
            payload = managed_project_cleanup.plan(root)
            receipt = managed_project_cleanup.execute(payload["sha256"], root)
        assert receipt["outcome"] == "completed"
        assert not candidate.exists()
        assert outside_file.read_bytes() == b"keep"
        assert (outside_dir / "kept").read_bytes() == b"keep"
    finally:
        temporary.cleanup()


def main() -> int:
    test_execute_removes_only_unchanged_exact_plan()
    test_execute_refuses_changed_candidate()
    test_fingerprint_records_symlinks_without_following()
    test_fingerprint_refuses_symlink_candidate_root()
    test_execute_removes_symlink_entries_without_following()
    print("MANAGED_PROJECT_CLEANUP_TESTS_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
