#!/usr/bin/env python3
"""Regression tests for deterministic temporal project knowledge."""

from __future__ import annotations

import datetime as dt
import tempfile
from pathlib import Path
from unittest import mock

import project_knowledge
import project_knowledge_db


FIRST = "1" * 40
SECOND = "2" * 40


def fake_git(*args: str, root: Path) -> str:
    command = tuple(args)
    if command and command[0] == "log":
        return (
            f"@@@{FIRST}\t\t2026-07-01T00:00:00+00:00\tinitial\n"
            "A\tkeep.txt\nA\told.txt\n"
            f"@@@{SECOND}\t{FIRST}\t2026-08-01T00:00:00+00:00\treplace old\n"
            "M\tkeep.txt\nD\told.txt\n"
        )
    if command[:1] == ("ls-files",):
        return "keep.txt\0"
    if command[:2] == ("status", "--porcelain=v1"):
        return ""
    if command == ("rev-parse", "HEAD"):
        return SECOND + "\n"
    if command[:3] == ("worktree", "list", "--porcelain"):
        return f"worktree {root}\nHEAD {SECOND}\ndetached\n"
    raise AssertionError(f"unexpected git query: {command}")


def test_graph_retains_deleted_paths_and_is_deterministic() -> None:
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        (root / "keep.txt").write_text("current\n", encoding="utf-8")
        with mock.patch.object(project_knowledge, "git", side_effect=fake_git):
            first = project_knowledge.build_graph(root)
            second = project_knowledge.build_graph(root)
        assert first == second
        assert first["counts"] == {
            "commits": 2,
            "components": first["counts"]["components"],
            "current_files": 1,
            "historical_files": 1,
            "file_events": 4,
        }
        old = next(item for item in first["files"] if item["path"] == "old.txt")
        assert old["exists"] is False
        assert [event["status"] for event in old["events"]] == ["A", "D"]
        keep = next(item for item in first["files"] if item["path"] == "keep.txt")
        assert keep["exists"] is True
        assert keep["content_sha256"]


def fixture_graph(root: Path) -> dict[str, object]:
    (root / "keep.txt").write_text("current\n", encoding="utf-8")
    with mock.patch.object(project_knowledge, "git", side_effect=fake_git):
        return project_knowledge.build_graph(root)


def test_database_migrates_and_round_trips_the_computed_graph() -> None:
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        graph = fixture_graph(root)
        database = root / "graph.sqlite3"
        stats = project_knowledge_db.write_database(database, graph)
        assert stats["file_events"] == 4
        assert stats["entities"] > stats["files"]
        with project_knowledge_db.connect(database, readonly=True) as connection:
            assert connection.execute("PRAGMA user_version").fetchone()[0] == 1
            migrations = list(connection.execute("SELECT version, name FROM schema_migrations"))
            assert [tuple(row) for row in migrations] == [
                (1, "initial-temporal-property-graph")
            ]
            project_knowledge_db.validate(connection, graph)
            assert project_knowledge_db.load_graph(connection) == graph


def test_database_rebuilds_atomically_when_projection_identity_changes() -> None:
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        graph = fixture_graph(root)
        database = root / "graph.sqlite3"
        assert project_knowledge_db.ensure_database(database, graph) is True
        assert project_knowledge_db.ensure_database(database, graph) is False
        changed = {**graph, "identity": "a" * 64}
        assert project_knowledge_db.ensure_database(database, changed) is True
        with project_knowledge_db.connect(database, readonly=True) as connection:
            assert project_knowledge_db.load_graph(connection)["identity"] == "a" * 64


def test_as_of_queries_distinguish_commit_time_and_deletion() -> None:
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        graph = fixture_graph(root)
        database = root / "graph.sqlite3"
        project_knowledge_db.write_database(database, graph)
        before = project_knowledge_db.path_states(database, "old.txt", FIRST[:8])
        after = project_knowledge_db.path_states(database, "old.txt", SECOND)
        assert before[0]["exists"] is True
        assert before[0]["status"] == "A"
        assert after[0]["exists"] is False
        assert after[0]["status"] == "D"
        timestamp = project_knowledge_db.path_states(
            database, "keep.txt", "2026-07-15T00:00:00Z"
        )
        assert timestamp[0]["commit"] == FIRST


def test_property_graph_traversal_and_read_only_sql() -> None:
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        graph = fixture_graph(root)
        database = root / "graph.sqlite3"
        project_knowledge_db.write_database(database, graph)
        relations = project_knowledge_db.neighbors(database, "keep.txt", depth=1)
        assert {item["predicate"] for item in relations} == {"changed_in", "classified_as"}
        columns, rows = project_knowledge_db.readonly_query(
            database, "SELECT count(*) AS retained FROM retired_files"
        )
        assert columns == ["retained"]
        assert rows == [{"retained": 1}]
        try:
            project_knowledge_db.readonly_query(database, "DELETE FROM files")
        except project_knowledge_db.DatabaseError as error:
            assert "authorized" in str(error).casefold()
        else:
            raise AssertionError("read-only query surface accepted a mutation")
        try:
            project_knowledge_db.readonly_query(
                database, f"ATTACH DATABASE '{root / 'escape.sqlite3'}' AS extra_db"
            )
        except project_knowledge_db.DatabaseError as error:
            assert "authorized" in str(error).casefold()
        else:
            raise AssertionError("read-only query surface accepted ATTACH")
        assert not (root / "escape.sqlite3").exists()


def test_integrity_gate_detects_property_graph_tampering() -> None:
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        graph = fixture_graph(root)
        database = root / "graph.sqlite3"
        project_knowledge_db.write_database(database, graph)
        with project_knowledge_db.connect(database) as connection:
            connection.execute("DELETE FROM relations WHERE predicate = 'classified_as'")
        with project_knowledge_db.connect(database, readonly=True) as connection:
            try:
                project_knowledge_db.validate(connection)
            except project_knowledge_db.DatabaseError as error:
                assert "relations are inconsistent" in str(error)
            else:
                raise AssertionError("property-graph tampering passed integrity validation")


def test_cleanup_separates_age_from_deletion_authority() -> None:
    old = "2025-01-01T00:00:00+00:00"
    graph = {
        "components": [
            {
                "component_id": "stable-source", "lifecycle": "primary",
                "last_committed_change_at": old, "review_by": None,
            },
            {
                "component_id": "old-experiment", "lifecycle": "historical",
                "last_committed_change_at": old, "review_by": None,
            },
        ]
    }
    observation = {
        "generated_paths": [
            {"path": "target", "component": "cargo", "bytes": 10,
             "safe_to_delete": True, "activity": "active-worktree-cache"}
        ],
        "inactive_generated_paths": [
            {"path": "workspace://Development/tmp/cargo-target-old",
             "component": "temporary", "bytes": 20, "safe_to_delete": True,
             "activity": "inactive-generated-cache", "proof": "fixture"}
        ],
        "areas": [],
        "worktrees": [
            {"path": "workspace://Development/worktrees/orphan", "bytes": 30,
             "registered": False, "state": "physical-orphan"}
        ],
    }
    report = project_knowledge.cleanup_report(
        graph, observation, now=dt.datetime(2026, 8, 1, tzinfo=dt.timezone.utc)
    )
    assert [item["path"] for item in report["recommended_cleanup"]] == [
        "workspace://Development/tmp/cargo-target-old"
    ]
    assert report["regenerable_active_caches"][0]["path"] == "target"
    assert report["orphaned_worktrees"][0]["state"] == "physical-orphan"
    assert report["decision_required"][0]["component"] == "old-experiment"
    assert report["old_but_not_cleanup"][0]["component"] == "stable-source"
    assert report["rule"] == "age alone never authorizes deletion"


def main() -> int:
    test_graph_retains_deleted_paths_and_is_deterministic()
    test_database_migrates_and_round_trips_the_computed_graph()
    test_database_rebuilds_atomically_when_projection_identity_changes()
    test_as_of_queries_distinguish_commit_time_and_deletion()
    test_property_graph_traversal_and_read_only_sql()
    test_integrity_gate_detects_property_graph_tampering()
    test_cleanup_separates_age_from_deletion_authority()
    print("PROJECT_KNOWLEDGE_TESTS_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
