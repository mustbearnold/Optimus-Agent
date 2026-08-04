#!/usr/bin/env python3


"""Regression tests for deterministic temporal project knowledge."""


from __future__ import annotations


import pathlib, sys
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / "tools"))
import datetime as dt
import tempfile
from pathlib import Path
from unittest import mock

import project_knowledge
import project_knowledge_db


FIRST = "1" * 40
SECOND = "2" * 40

# Mixed committer offsets are deliberate: this repository's real history mixes
# +12:00 and Z, and lexical comparison of raw offsets once broke as-of queries.
FIRST_RAW = "2026-07-01T12:00:00+12:00"    # 2026-07-01T00:00:00Z
SECOND_RAW = "2026-07-15T23:00:00+12:00"   # 2026-07-15T11:00:00Z

CARGO_FIRST = '[package]\nname = "fixture"\n\n[dependencies]\nserde = "1"\n'
CARGO_SECOND = '[package]\nname = "fixture"\n\n[dependencies]\nanyhow = "1"\n'
LIB_RS = "pub fn hello() {}\n\npub struct World;\n"


def fake_git(*args: str, root: Path) -> str:
    command = tuple(args)
    if command and command[0] == "log":
        assert "--topo-order" in command, "history walk must be topological"
        return (
            f"@@@{FIRST}\t\t{FIRST_RAW}\t{FIRST_RAW}\tAda\tada@example.test\tinitial\n"
            "A\tkeep.txt\nA\told.txt\nA\tCargo.toml\nA\tlib.rs\n"
            f"@@@{SECOND}\t{FIRST}\t{SECOND_RAW}\t{SECOND_RAW}\tAda\tada@example.test\treplace old\n"
            "M\tkeep.txt\nD\told.txt\nM\tCargo.toml\n"
        )
    if command[:1] == ("ls-files",):
        return "keep.txt\0Cargo.toml\0lib.rs\0"
    if command[:2] == ("status", "--porcelain=v1"):
        return ""
    if command == ("rev-parse", "HEAD"):
        return SECOND + "\n"
    if command[:3] == ("worktree", "list", "--porcelain"):
        return f"worktree {root}\nHEAD {SECOND}\ndetached\n"
    if command[:1] == ("show",):
        if command[1] == f"{FIRST}:Cargo.toml":
            return CARGO_FIRST
        if command[1] == f"{SECOND}:Cargo.toml":
            return CARGO_SECOND
    raise AssertionError(f"unexpected git query: {command}")


def write_fixture_tree(root: Path) -> None:
    (root / "keep.txt").write_text("current\n", encoding="utf-8")
    (root / "Cargo.toml").write_text(CARGO_SECOND, encoding="utf-8")
    (root / "lib.rs").write_text(LIB_RS, encoding="utf-8")


def fixture_graph(root: Path) -> dict[str, object]:
    write_fixture_tree(root)
    with mock.patch.object(project_knowledge, "git", side_effect=fake_git):
        return project_knowledge.build_graph(root)


def test_graph_retains_deleted_paths_and_is_deterministic() -> None:
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        write_fixture_tree(root)
        with mock.patch.object(project_knowledge, "git", side_effect=fake_git):
            first = project_knowledge.build_graph(root)
            second = project_knowledge.build_graph(root)
        assert first == second
        assert first["counts"] == {
            "commits": 2,
            "components": first["counts"]["components"],
            "current_files": 3,
            "historical_files": 1,
            "file_events": 7,
            "packages": 3,
            "package_dependencies": 2,
            "code_symbols": 2,
        }
        old = next(item for item in first["files"] if item["path"] == "old.txt")
        assert old["exists"] is False
        assert [event["status"] for event in old["events"]] == ["A", "D"]
        keep = next(item for item in first["files"] if item["path"] == "keep.txt")
        assert keep["exists"] is True
        assert keep["content_sha256"]


def test_history_times_are_normalized_to_utc() -> None:
    with tempfile.TemporaryDirectory() as directory:
        graph = fixture_graph(Path(directory))
        assert [commit["committed_at"] for commit in graph["commits"]] == [
            "2026-07-01T00:00:00+00:00",
            "2026-07-15T11:00:00+00:00",
        ]
        assert graph["commits"][0]["author_email"] == "ada@example.test"
        assert graph["commits"][0]["authored_at"] == "2026-07-01T00:00:00+00:00"


def test_database_migrates_and_round_trips_the_computed_graph() -> None:
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        graph = fixture_graph(root)
        database = root / "graph.sqlite3"
        stats = project_knowledge_db.write_database(database, graph)
        assert stats["file_events"] == 7
        assert stats["entities"] > stats["files"]
        with project_knowledge_db.connect(database, readonly=True) as connection:
            assert connection.execute("PRAGMA user_version").fetchone()[0] == 2
            migrations = list(connection.execute("SELECT version, name FROM schema_migrations"))
            assert [tuple(row) for row in migrations] == [
                (1, "initial-temporal-property-graph"),
                (2, "temporal-code-graph-and-utc-order"),
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
        assert after[0]["component_now"]
        timestamp = project_knowledge_db.path_states(
            database, "keep.txt", "2026-07-10T00:00:00Z"
        )
        assert timestamp[0]["commit"] == FIRST


def test_as_of_timestamps_compare_instants_across_offsets() -> None:
    # SECOND committed at 2026-07-15T23:00:00+12:00 == 11:00Z. A 12:00Z
    # boundary is after that instant even though the raw offset string sorts
    # later; raw lexical comparison once hid the deletion.
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        graph = fixture_graph(root)
        database = root / "graph.sqlite3"
        project_knowledge_db.write_database(database, graph)
        states = project_knowledge_db.path_states(
            database, "old.txt", "2026-07-15T12:00:00Z"
        )
        assert states[0]["status"] == "D"
        earlier = project_knowledge_db.path_states(
            database, "old.txt", "2026-07-15T10:59:59Z"
        )
        assert earlier[0]["status"] == "A"


def test_boundary_prefixes_never_act_as_wildcards() -> None:
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        graph = fixture_graph(root)
        database = root / "graph.sqlite3"
        project_knowledge_db.write_database(database, graph)
        try:
            project_knowledge_db.path_states(database, "old.txt", "1%")
        except project_knowledge_db.DatabaseError as error:
            assert "ISO-8601" in str(error)
        else:
            raise AssertionError("a LIKE wildcard acted as a commit prefix")


def test_dependency_intervals_open_and_close_with_history() -> None:
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        graph = fixture_graph(root)
        intervals = {
            (item["dependency"], item["valid_from_position"], item["valid_to_position"])
            for item in graph["package_dependencies"]
        }
        assert intervals == {("serde", 0, 1), ("anyhow", 1, None)}
        packages = {item["package_id"]: item["origin"] for item in graph["packages"]}
        assert packages == {
            "cargo:fixture": "internal",
            "cargo:anyhow": "external",
            "cargo:serde": "external",
        }


def test_neighbors_respect_dependency_validity_intervals() -> None:
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        graph = fixture_graph(root)
        database = root / "graph.sqlite3"
        project_knowledge_db.write_database(database, graph)
        at_first = project_knowledge_db.neighbors(
            database, "cargo:fixture", depth=1, point=FIRST[:12]
        )
        deps_first = {
            item["properties"]["dependency"]
            for item in at_first if item["predicate"] == "depends_on"
        }
        assert deps_first == {"serde"}
        at_second = project_knowledge_db.neighbors(
            database, "cargo:fixture", depth=1, point=SECOND[:12]
        )
        deps_second = {
            item["properties"]["dependency"]
            for item in at_second if item["predicate"] == "depends_on"
        }
        assert deps_second == {"anyhow"}


def test_symbols_and_authors_are_first_class_graph_facts() -> None:
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        graph = fixture_graph(root)
        symbols = {(item["name"], item["kind"]) for item in graph["code_symbols"]}
        assert symbols == {("hello", "fn"), ("World", "struct")}
        database = root / "graph.sqlite3"
        project_knowledge_db.write_database(database, graph)
        relations = project_knowledge_db.neighbors(database, "lib.rs", depth=1)
        declared = {
            item["to"] for item in relations if item["predicate"] == "declares"
        }
        assert declared == {
            "symbol:lib.rs:fn:hello:1", "symbol:lib.rs:struct:World:3",
        }
        commit_relations = project_knowledge_db.neighbors(database, FIRST[:12], depth=1)
        authored = [
            item for item in commit_relations if item["predicate"] == "authored_by"
        ]
        assert authored and authored[0]["to"] == "author:ada@example.test"


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


def test_integrity_gate_detects_count_preserving_tampering() -> None:
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        graph = fixture_graph(root)
        database = root / "graph.sqlite3"
        project_knowledge_db.write_database(database, graph)
        with project_knowledge_db.connect(database) as connection:
            connection.execute(
                """UPDATE relations SET properties_json = '{"forged":true}'
                   WHERE predicate = 'changed_in'
                   AND relation_id = (
                       SELECT relation_id FROM relations
                       WHERE predicate = 'changed_in' LIMIT 1
                   )"""
            )
        with project_knowledge_db.connect(database, readonly=True) as connection:
            try:
                project_knowledge_db.validate(connection)
            except project_knowledge_db.DatabaseError as error:
                assert "digest" in str(error)
            else:
                raise AssertionError("count-preserving tampering passed validation")


def test_projection_rejects_non_topological_commit_order() -> None:
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        graph = fixture_graph(root)
        broken = {**graph, "commits": list(reversed(graph["commits"]))}
        try:
            project_knowledge_db.validate_projection(broken)
        except project_knowledge_db.DatabaseError as error:
            assert "topological" in str(error)
        else:
            raise AssertionError("a child before its parent passed validation")


def test_check_requires_every_historical_path_to_be_retained() -> None:
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        write_fixture_tree(root)
        with mock.patch.object(project_knowledge, "git", side_effect=fake_git):
            counts = project_knowledge.check(root)
            assert counts["historical_files"] == 1
            complete = project_knowledge.build_graph(root)
            dropped = {
                **complete,
                "files": [item for item in complete["files"] if item["path"] != "old.txt"],
            }
            with mock.patch.object(
                project_knowledge, "build_graph", return_value=dropped
            ):
                try:
                    project_knowledge.check(root)
                except project_knowledge.KnowledgeError as error:
                    assert "retired paths" in str(error)
                else:
                    raise AssertionError("a dropped historical path passed check")


def test_workspace_conventions_prove_inactive_generated_output() -> None:
    with tempfile.TemporaryDirectory() as directory:
        wrapper = Path(directory)
        root = wrapper / "Repository"
        root.mkdir()
        development = wrapper / "Development"
        (development / "tools" / "playwright-browsers" / "chromium-1").mkdir(parents=True)
        (development / "tmp" / "ms-playwright" / "chromium-1").mkdir(parents=True)
        home = development / "tmp" / "compiled-workbench" / "optimus-home-4242"
        home.mkdir(parents=True)
        (home / "optimus.db").write_bytes(b"")
        snapshot = development / "Archive" / "stale-root-snapshot" / "apps" / "ui"
        (snapshot / "node_modules").mkdir(parents=True)
        (snapshot / "package.json").write_text("{}\n", encoding="utf-8")
        with mock.patch.object(project_knowledge, "git", side_effect=fake_git), \
                mock.patch.object(
                    project_knowledge.repository_ontology, "workspace_root",
                    return_value=wrapper,
                ):
            observation = project_knowledge.workspace_observation(root)
        proofs = {
            item["path"]: item["proof"]
            for item in observation["inactive_generated_paths"]
        }
        assert "workspace://Development/tmp/ms-playwright" in proofs
        assert (
            "workspace://Development/tmp/compiled-workbench/optimus-home-4242" in proofs
        )
        assert (
            "workspace://Development/Archive/stale-root-snapshot/apps/ui/node_modules"
            in proofs
        )
        for proof in proofs.values():
            assert proof


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
    test_history_times_are_normalized_to_utc()
    test_database_migrates_and_round_trips_the_computed_graph()
    test_database_rebuilds_atomically_when_projection_identity_changes()
    test_as_of_queries_distinguish_commit_time_and_deletion()
    test_as_of_timestamps_compare_instants_across_offsets()
    test_boundary_prefixes_never_act_as_wildcards()
    test_dependency_intervals_open_and_close_with_history()
    test_neighbors_respect_dependency_validity_intervals()
    test_symbols_and_authors_are_first_class_graph_facts()
    test_property_graph_traversal_and_read_only_sql()
    test_integrity_gate_detects_property_graph_tampering()
    test_integrity_gate_detects_count_preserving_tampering()
    test_projection_rejects_non_topological_commit_order()
    test_check_requires_every_historical_path_to_be_retained()
    test_workspace_conventions_prove_inactive_generated_output()
    test_cleanup_separates_age_from_deletion_authority()
    print("PROJECT_KNOWLEDGE_TESTS_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
