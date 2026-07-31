#!/usr/bin/env python3
"""SQLite storage and queries for the derived temporal project graph."""

from __future__ import annotations

import datetime as dt
import json
import os
import sqlite3
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any


DATABASE_SCHEMA_VERSION = 1


class DatabaseError(RuntimeError):
    """The temporal project database is unavailable or internally inconsistent."""


@dataclass(frozen=True)
class TemporalBoundary:
    label: str
    commit_position: int | None
    timestamp: str | None


MIGRATIONS: tuple[tuple[int, str, str], ...] = (
    (
        1,
        "initial-temporal-property-graph",
        """
        CREATE TABLE schema_migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE
        );
        CREATE TABLE metadata (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE commits (
            position INTEGER NOT NULL UNIQUE,
            sha TEXT PRIMARY KEY CHECK(length(sha) = 40),
            committed_at TEXT NOT NULL,
            subject TEXT NOT NULL,
            touch_count INTEGER NOT NULL CHECK(touch_count >= 0)
        );
        CREATE INDEX commits_committed_at_idx ON commits(committed_at, position);
        CREATE TABLE commit_parents (
            commit_sha TEXT NOT NULL REFERENCES commits(sha) ON DELETE CASCADE,
            position INTEGER NOT NULL CHECK(position >= 0),
            parent_sha TEXT NOT NULL REFERENCES commits(sha),
            PRIMARY KEY(commit_sha, position)
        );
        CREATE INDEX commit_parents_parent_idx ON commit_parents(parent_sha);
        CREATE TABLE components (
            position INTEGER NOT NULL UNIQUE,
            component_id TEXT PRIMARY KEY,
            path TEXT NOT NULL,
            lifecycle TEXT NOT NULL,
            retention TEXT NOT NULL,
            safe_to_delete INTEGER NOT NULL CHECK(safe_to_delete IN (0, 1)),
            review_by TEXT,
            current_file_count INTEGER NOT NULL CHECK(current_file_count >= 0),
            last_committed_change_at TEXT,
            parent_component_id TEXT REFERENCES components(component_id)
        );
        CREATE INDEX components_lifecycle_idx ON components(lifecycle, review_by);
        CREATE TABLE component_pairs (
            component_id TEXT NOT NULL REFERENCES components(component_id) ON DELETE CASCADE,
            position INTEGER NOT NULL CHECK(position >= 0),
            paired_component_id TEXT NOT NULL REFERENCES components(component_id),
            PRIMARY KEY(component_id, position)
        );
        CREATE INDEX component_pairs_target_idx ON component_pairs(paired_component_id);
        CREATE TABLE component_lifecycle_events (
            component_id TEXT NOT NULL REFERENCES components(component_id) ON DELETE CASCADE,
            sequence INTEGER NOT NULL CHECK(sequence >= 0),
            occurred_at TEXT NOT NULL,
            state TEXT NOT NULL,
            event_json TEXT NOT NULL CHECK(json_valid(event_json)),
            PRIMARY KEY(component_id, sequence)
        );
        CREATE INDEX lifecycle_events_time_idx
            ON component_lifecycle_events(occurred_at, component_id);
        CREATE TABLE files (
            path TEXT PRIMARY KEY,
            exists_now INTEGER NOT NULL CHECK(exists_now IN (0, 1)),
            bytes INTEGER NOT NULL CHECK(bytes >= 0),
            content_sha256 TEXT,
            component_id TEXT NOT NULL REFERENCES components(component_id),
            lifecycle TEXT NOT NULL,
            distribution TEXT NOT NULL,
            working_state TEXT NOT NULL,
            created_at TEXT,
            last_committed_change_at TEXT,
            last_commit_sha TEXT REFERENCES commits(sha),
            change_count INTEGER NOT NULL CHECK(change_count >= 0)
        );
        CREATE INDEX files_component_idx ON files(component_id, exists_now, path);
        CREATE INDEX files_last_change_idx ON files(last_committed_change_at, path);
        CREATE TABLE file_events (
            path TEXT NOT NULL REFERENCES files(path) ON DELETE CASCADE,
            sequence INTEGER NOT NULL CHECK(sequence >= 0),
            commit_sha TEXT NOT NULL REFERENCES commits(sha),
            commit_position INTEGER NOT NULL REFERENCES commits(position),
            occurred_at TEXT NOT NULL,
            status TEXT NOT NULL CHECK(length(status) = 1),
            PRIMARY KEY(path, sequence)
        );
        CREATE INDEX file_events_time_idx
            ON file_events(occurred_at, path, sequence);
        CREATE INDEX file_events_commit_idx
            ON file_events(commit_position, path, sequence);
        CREATE TABLE entities (
            entity_id TEXT PRIMARY KEY,
            kind TEXT NOT NULL,
            label TEXT NOT NULL,
            properties_json TEXT NOT NULL CHECK(json_valid(properties_json))
        );
        CREATE INDEX entities_kind_label_idx ON entities(kind, label);
        CREATE TABLE relations (
            relation_id TEXT PRIMARY KEY,
            source_id TEXT NOT NULL REFERENCES entities(entity_id) ON DELETE CASCADE,
            predicate TEXT NOT NULL,
            target_id TEXT NOT NULL REFERENCES entities(entity_id) ON DELETE CASCADE,
            event_time TEXT,
            commit_position INTEGER REFERENCES commits(position),
            properties_json TEXT NOT NULL CHECK(json_valid(properties_json))
        );
        CREATE INDEX relations_source_idx ON relations(source_id, predicate, event_time);
        CREATE INDEX relations_target_idx ON relations(target_id, predicate, event_time);
        CREATE INDEX relations_commit_idx ON relations(commit_position, predicate);
        CREATE VIEW current_files AS SELECT * FROM files WHERE exists_now = 1;
        CREATE VIEW retired_files AS SELECT * FROM files WHERE exists_now = 0;
        """,
    ),
)


def compact(payload: Any) -> str:
    return json.dumps(payload, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def connect(path: Path | str, *, readonly: bool = False) -> sqlite3.Connection:
    if readonly:
        uri = Path(path).resolve().as_uri() + "?mode=ro"
        connection = sqlite3.connect(uri, uri=True)
    else:
        connection = sqlite3.connect(path)
    connection.row_factory = sqlite3.Row
    connection.execute("PRAGMA foreign_keys = ON")
    connection.execute("PRAGMA busy_timeout = 5000")
    if readonly:
        connection.execute("PRAGMA query_only = ON")
    else:
        connection.execute("PRAGMA journal_mode = DELETE")
        connection.execute("PRAGMA synchronous = FULL")
    return connection


def migrate(connection: sqlite3.Connection) -> None:
    current = int(connection.execute("PRAGMA user_version").fetchone()[0])
    if current > DATABASE_SCHEMA_VERSION:
        raise DatabaseError(
            f"database schema {current} is newer than supported {DATABASE_SCHEMA_VERSION}"
        )
    for version, name, sql in MIGRATIONS:
        if version <= current:
            continue
        escaped = name.replace("'", "''")
        connection.executescript(
            "BEGIN IMMEDIATE;\n"
            + sql
            + f"\nINSERT INTO schema_migrations(version, name) VALUES ({version}, '{escaped}');\n"
            + f"PRAGMA user_version = {version};\nCOMMIT;\n"
        )
        current = version
    if current != DATABASE_SCHEMA_VERSION:
        raise DatabaseError(f"no migration reaches schema {DATABASE_SCHEMA_VERSION}")


def _insert_entity(
    connection: sqlite3.Connection, entity_id: str, kind: str, label: str, properties: Any
) -> None:
    connection.execute(
        "INSERT INTO entities(entity_id, kind, label, properties_json) VALUES (?, ?, ?, ?)",
        (entity_id, kind, label, compact(properties)),
    )


def _insert_relation(
    connection: sqlite3.Connection,
    relation_id: str,
    source_id: str,
    predicate: str,
    target_id: str,
    *,
    event_time: str | None = None,
    commit_position: int | None = None,
    properties: Any | None = None,
) -> None:
    connection.execute(
        """INSERT INTO relations(
               relation_id, source_id, predicate, target_id, event_time,
               commit_position, properties_json
           ) VALUES (?, ?, ?, ?, ?, ?, ?)""",
        (
            relation_id,
            source_id,
            predicate,
            target_id,
            event_time,
            commit_position,
            compact(properties or {}),
        ),
    )


def populate(connection: sqlite3.Connection, graph: dict[str, Any]) -> None:
    commit_positions = {
        commit["sha"]: position for position, commit in enumerate(graph["commits"])
    }
    with connection:
        metadata = {
            "graph_schema_version": str(graph["schema_version"]),
            "graph_identity": graph["identity"],
            "head": graph["head"],
            "scope": compact(graph["scope"]),
            "counts": compact(graph["counts"]),
        }
        connection.executemany(
            "INSERT INTO metadata(key, value) VALUES (?, ?)", metadata.items()
        )
        for position, commit in enumerate(graph["commits"]):
            connection.execute(
                """INSERT INTO commits(position, sha, committed_at, subject, touch_count)
                   VALUES (?, ?, ?, ?, ?)""",
                (
                    position,
                    commit["sha"],
                    commit["committed_at"],
                    commit["subject"],
                    commit["touch_count"],
                ),
            )
            _insert_entity(
                connection,
                commit["id"],
                "commit",
                commit["sha"],
                {
                    "committed_at": commit["committed_at"],
                    "subject": commit["subject"],
                    "touch_count": commit["touch_count"],
                },
            )
        for commit in graph["commits"]:
            for position, parent in enumerate(commit["parents"]):
                connection.execute(
                    "INSERT INTO commit_parents(commit_sha, position, parent_sha) VALUES (?, ?, ?)",
                    (commit["sha"], position, parent),
                )
                _insert_relation(
                    connection,
                    f"parent:{commit['sha']}:{position}",
                    commit["id"],
                    "has_parent",
                    f"commit:{parent}",
                    event_time=commit["committed_at"],
                    commit_position=commit_positions[commit["sha"]],
                    properties={"position": position},
                )
        for position, component in enumerate(graph["components"]):
            connection.execute(
                """INSERT INTO components(
                       position, component_id, path, lifecycle, retention, safe_to_delete,
                       review_by, current_file_count, last_committed_change_at,
                       parent_component_id
                   ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)""",
                (
                    position,
                    component["component_id"],
                    component["path"],
                    component["lifecycle"],
                    component["retention"],
                    int(component["safe_to_delete"]),
                    component["review_by"],
                    component["current_file_count"],
                    component["last_committed_change_at"],
                    component["parent"],
                ),
            )
            _insert_entity(
                connection,
                component["id"],
                "component",
                component["component_id"],
                {key: value for key, value in component.items() if key != "lifecycle_events"},
            )
        for component in graph["components"]:
            source_id = component["id"]
            if component["parent"]:
                _insert_relation(
                    connection,
                    f"component-parent:{component['component_id']}",
                    source_id,
                    "has_parent_component",
                    f"component:{component['parent']}",
                )
            for position, paired in enumerate(component["paired_with"]):
                connection.execute(
                    """INSERT INTO component_pairs(component_id, position, paired_component_id)
                       VALUES (?, ?, ?)""",
                    (component["component_id"], position, paired),
                )
                _insert_relation(
                    connection,
                    f"component-pair:{component['component_id']}:{position}",
                    source_id,
                    "paired_with",
                    f"component:{paired}",
                    properties={"position": position},
                )
            for sequence, event in enumerate(component["lifecycle_events"]):
                event_id = f"lifecycle:{component['component_id']}:{sequence}"
                connection.execute(
                    """INSERT INTO component_lifecycle_events(
                           component_id, sequence, occurred_at, state, event_json
                       ) VALUES (?, ?, ?, ?, ?)""",
                    (
                        component["component_id"],
                        sequence,
                        event["at"],
                        event["state"],
                        compact(event),
                    ),
                )
                _insert_entity(connection, event_id, "lifecycle_event", event["state"], event)
                _insert_relation(
                    connection,
                    f"component-lifecycle:{component['component_id']}:{sequence}",
                    source_id,
                    "has_lifecycle_event",
                    event_id,
                    event_time=event["at"],
                    properties={"sequence": sequence},
                )
        for file in graph["files"]:
            connection.execute(
                """INSERT INTO files(
                       path, exists_now, bytes, content_sha256, component_id, lifecycle,
                       distribution, working_state, created_at, last_committed_change_at,
                       last_commit_sha, change_count
                   ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)""",
                (
                    file["path"],
                    int(file["exists"]),
                    file["bytes"],
                    file["content_sha256"],
                    file["component"],
                    file["lifecycle"],
                    file["distribution"],
                    file["working_state"],
                    file["created_at"],
                    file["last_committed_change_at"],
                    file["last_commit"],
                    file["change_count"],
                ),
            )
            _insert_entity(
                connection,
                file["id"],
                "file",
                file["path"],
                {key: value for key, value in file.items() if key != "events"},
            )
            _insert_relation(
                connection,
                f"classification:{file['path']}",
                file["id"],
                "classified_as",
                f"component:{file['component']}",
            )
            for sequence, event in enumerate(file["events"]):
                position = commit_positions[event["commit"]]
                connection.execute(
                    """INSERT INTO file_events(
                           path, sequence, commit_sha, commit_position, occurred_at, status
                       ) VALUES (?, ?, ?, ?, ?, ?)""",
                    (
                        file["path"],
                        sequence,
                        event["commit"],
                        position,
                        event["at"],
                        event["status"],
                    ),
                )
                _insert_relation(
                    connection,
                    f"file-change:{file['path']}:{sequence}",
                    file["id"],
                    "changed_in",
                    f"commit:{event['commit']}",
                    event_time=event["at"],
                    commit_position=position,
                    properties={"sequence": sequence, "status": event["status"]},
                )


def _metadata(connection: sqlite3.Connection) -> dict[str, str]:
    return {
        str(row["key"]): str(row["value"])
        for row in connection.execute("SELECT key, value FROM metadata")
    }


def load_graph(connection: sqlite3.Connection) -> dict[str, Any]:
    metadata = _metadata(connection)
    parents: dict[str, list[str]] = {}
    for row in connection.execute(
        "SELECT commit_sha, parent_sha FROM commit_parents ORDER BY commit_sha, position"
    ):
        parents.setdefault(row["commit_sha"], []).append(row["parent_sha"])
    commits = [
        {
            "id": f"commit:{row['sha']}",
            "sha": row["sha"],
            "parents": parents.get(row["sha"], []),
            "committed_at": row["committed_at"],
            "subject": row["subject"],
            "touch_count": row["touch_count"],
        }
        for row in connection.execute("SELECT * FROM commits ORDER BY position")
    ]
    pairs: dict[str, list[str]] = {}
    for row in connection.execute(
        "SELECT component_id, paired_component_id FROM component_pairs ORDER BY component_id, position"
    ):
        pairs.setdefault(row["component_id"], []).append(row["paired_component_id"])
    lifecycle: dict[str, list[dict[str, Any]]] = {}
    for row in connection.execute(
        "SELECT component_id, event_json FROM component_lifecycle_events ORDER BY component_id, sequence"
    ):
        lifecycle.setdefault(row["component_id"], []).append(json.loads(row["event_json"]))
    components = [
        {
            "id": f"component:{row['component_id']}",
            "component_id": row["component_id"],
            "path": row["path"],
            "lifecycle": row["lifecycle"],
            "retention": row["retention"],
            "safe_to_delete": bool(row["safe_to_delete"]),
            "review_by": row["review_by"],
            "current_file_count": row["current_file_count"],
            "last_committed_change_at": row["last_committed_change_at"],
            "parent": row["parent_component_id"],
            "paired_with": pairs.get(row["component_id"], []),
            "lifecycle_events": lifecycle.get(row["component_id"], []),
        }
        for row in connection.execute("SELECT * FROM components ORDER BY position")
    ]
    events: dict[str, list[dict[str, str]]] = {}
    for row in connection.execute(
        "SELECT path, commit_sha, occurred_at, status FROM file_events ORDER BY path, sequence"
    ):
        events.setdefault(row["path"], []).append(
            {"commit": row["commit_sha"], "at": row["occurred_at"], "status": row["status"]}
        )
    files = [
        {
            "id": f"file:{row['path']}",
            "path": row["path"],
            "exists": bool(row["exists_now"]),
            "bytes": row["bytes"],
            "content_sha256": row["content_sha256"],
            "component": row["component_id"],
            "lifecycle": row["lifecycle"],
            "distribution": row["distribution"],
            "working_state": row["working_state"],
            "created_at": row["created_at"],
            "last_committed_change_at": row["last_committed_change_at"],
            "last_commit": row["last_commit_sha"],
            "change_count": row["change_count"],
            "events": events.get(row["path"], []),
        }
        for row in connection.execute("SELECT * FROM files ORDER BY path")
    ]
    return {
        "schema_version": int(metadata["graph_schema_version"]),
        "identity": metadata["graph_identity"],
        "head": metadata["head"],
        "scope": json.loads(metadata["scope"]),
        "counts": json.loads(metadata["counts"]),
        "components": components,
        "files": files,
        "commits": commits,
    }


def database_stats(connection: sqlite3.Connection) -> dict[str, int]:
    return {
        table: int(connection.execute(f"SELECT count(*) FROM {table}").fetchone()[0])
        for table in ("commits", "components", "files", "file_events", "entities", "relations")
    }


def validate(connection: sqlite3.Connection, expected: dict[str, Any] | None = None) -> None:
    version = int(connection.execute("PRAGMA user_version").fetchone()[0])
    if version != DATABASE_SCHEMA_VERSION:
        raise DatabaseError(
            f"database schema is {version}, expected {DATABASE_SCHEMA_VERSION}"
        )
    integrity = connection.execute("PRAGMA integrity_check").fetchone()[0]
    if integrity != "ok":
        raise DatabaseError(f"SQLite integrity check failed: {integrity}")
    foreign_keys = list(connection.execute("PRAGMA foreign_key_check"))
    if foreign_keys:
        raise DatabaseError(f"SQLite foreign-key check failed: {foreign_keys[0]}")
    metadata = _metadata(connection)
    required = {"graph_schema_version", "graph_identity", "head", "scope", "counts"}
    missing = required - set(metadata)
    if missing:
        raise DatabaseError(f"database metadata is missing: {', '.join(sorted(missing))}")
    counts = json.loads(metadata["counts"])
    stats = database_stats(connection)
    actual = {
        "commits": stats["commits"],
        "components": stats["components"],
        "current_files": int(
            connection.execute("SELECT count(*) FROM files WHERE exists_now = 1").fetchone()[0]
        ),
        "historical_files": int(
            connection.execute("SELECT count(*) FROM files WHERE exists_now = 0").fetchone()[0]
        ),
        "file_events": stats["file_events"],
    }
    if actual != counts:
        raise DatabaseError(f"database counts differ from metadata: {actual} != {counts}")
    mismatched_files = int(
        connection.execute(
            """SELECT count(*) FROM files AS f
               WHERE f.change_count != (
                   SELECT count(*) FROM file_events AS e WHERE e.path = f.path
               )"""
        ).fetchone()[0]
    )
    if mismatched_files:
        raise DatabaseError(f"{mismatched_files} file change counts are inconsistent")
    mismatched_commits = int(
        connection.execute(
            """SELECT count(*) FROM commits AS c
               WHERE c.touch_count != (
                   SELECT count(*) FROM file_events AS e WHERE e.commit_sha = c.sha
               )"""
        ).fetchone()[0]
    )
    if mismatched_commits:
        raise DatabaseError(f"{mismatched_commits} commit touch counts are inconsistent")
    expected_entities = {
        "commit": stats["commits"],
        "component": stats["components"],
        "file": stats["files"],
        "lifecycle_event": int(
            connection.execute("SELECT count(*) FROM component_lifecycle_events").fetchone()[0]
        ),
    }
    actual_entities = {
        row["kind"]: row["count"]
        for row in connection.execute(
            "SELECT kind, count(*) AS count FROM entities GROUP BY kind"
        )
    }
    if actual_entities != expected_entities:
        raise DatabaseError(
            f"property-graph entities are inconsistent: {actual_entities} != {expected_entities}"
        )
    expected_relations = {
        "changed_in": stats["file_events"],
        "classified_as": stats["files"],
        "has_parent": int(connection.execute("SELECT count(*) FROM commit_parents").fetchone()[0]),
        "has_parent_component": int(
            connection.execute(
                "SELECT count(*) FROM components WHERE parent_component_id IS NOT NULL"
            ).fetchone()[0]
        ),
        "paired_with": int(connection.execute("SELECT count(*) FROM component_pairs").fetchone()[0]),
        "has_lifecycle_event": expected_entities["lifecycle_event"],
    }
    actual_relations = {
        row["predicate"]: row["count"]
        for row in connection.execute(
            "SELECT predicate, count(*) AS count FROM relations GROUP BY predicate"
        )
    }
    if actual_relations != expected_relations:
        raise DatabaseError(
            f"property-graph relations are inconsistent: {actual_relations} != {expected_relations}"
        )
    if expected is not None and load_graph(connection) != expected:
        raise DatabaseError("database round trip differs from the computed graph")


def validate_projection(graph: dict[str, Any]) -> dict[str, int]:
    try:
        with connect(":memory:") as connection:
            migrate(connection)
            populate(connection, graph)
            validate(connection, graph)
            return database_stats(connection)
    except sqlite3.DatabaseError as error:
        raise DatabaseError(f"cannot validate database projection: {error}") from error


def write_database(path: Path, graph: dict[str, Any]) -> dict[str, int]:
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary_name = tempfile.mkstemp(
        prefix=f".{path.name}.", suffix=".tmp", dir=path.parent
    )
    os.close(descriptor)
    temporary = Path(temporary_name)
    temporary.unlink()
    try:
        try:
            with connect(temporary) as connection:
                migrate(connection)
                populate(connection, graph)
                validate(connection, graph)
                stats = database_stats(connection)
        except sqlite3.DatabaseError as error:
            raise DatabaseError(f"cannot build temporal project database: {error}") from error
        os.chmod(temporary, 0o600)
        sync_descriptor = os.open(temporary, os.O_RDONLY)
        try:
            os.fsync(sync_descriptor)
        finally:
            os.close(sync_descriptor)
        os.replace(temporary, path)
        directory_descriptor = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(directory_descriptor)
        finally:
            os.close(directory_descriptor)
        return stats
    finally:
        temporary.unlink(missing_ok=True)


def is_current(path: Path, graph: dict[str, Any]) -> bool:
    if not matches_source(path, graph["identity"], graph["head"]):
        return False
    try:
        with connect(path, readonly=True) as connection:
            validate(connection, graph)
            return True
    except (DatabaseError, KeyError, json.JSONDecodeError, sqlite3.DatabaseError, OSError):
        return False


def matches_source(path: Path, identity: str, head: str) -> bool:
    if not path.is_file():
        return False
    try:
        with connect(path, readonly=True) as connection:
            metadata = _metadata(connection)
            if (
                metadata.get("graph_identity") != identity
                or metadata.get("head") != head
            ):
                return False
            validate(connection)
            return True
    except (DatabaseError, KeyError, json.JSONDecodeError, sqlite3.DatabaseError, OSError):
        return False


def ensure_database(path: Path, graph: dict[str, Any]) -> bool:
    if is_current(path, graph):
        return False
    write_database(path, graph)
    return True


def _parse_timestamp(value: str) -> str:
    try:
        parsed = dt.datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as error:
        raise DatabaseError(f"not a commit id or ISO-8601 timestamp: {value}") from error
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=dt.timezone.utc)
    return parsed.astimezone(dt.timezone.utc).isoformat()


def resolve_boundary(connection: sqlite3.Connection, point: str) -> TemporalBoundary:
    matches = list(
        connection.execute(
            "SELECT position, sha, committed_at FROM commits WHERE sha LIKE ? ORDER BY position",
            (point + "%",),
        )
    )
    if len(matches) == 1:
        row = matches[0]
        return TemporalBoundary(row["sha"], row["position"], row["committed_at"])
    if len(matches) > 1:
        raise DatabaseError(f"commit prefix is ambiguous: {point}")
    timestamp = _parse_timestamp(point)
    return TemporalBoundary(timestamp, None, timestamp)


def _path_clause(path: str) -> tuple[str, tuple[str, str]]:
    normalized = path.rstrip("/")
    return "(f.path = ? OR f.path LIKE ?)", (normalized, normalized + "/%")


def path_states(path: Path, project_path: str, point: str | None = None) -> list[dict[str, Any]]:
    with connect(path, readonly=True) as connection:
        clause, parameters = _path_clause(project_path)
        if point is None:
            return [
                {
                    "path": row["path"],
                    "exists": bool(row["exists_now"]),
                    "status": "current" if row["exists_now"] else "D",
                    "at": row["last_committed_change_at"],
                    "commit": row["last_commit_sha"],
                    "component": row["component_id"],
                }
                for row in connection.execute(
                    f"""SELECT f.* FROM files AS f WHERE {clause} ORDER BY f.path""",
                    parameters,
                )
            ]
        boundary = resolve_boundary(connection, point)
        condition: str
        boundary_value: int | str
        if boundary.commit_position is not None:
            condition = "e.commit_position <= ?"
            boundary_value = boundary.commit_position
        else:
            condition = "e.occurred_at <= ?"
            boundary_value = boundary.timestamp or ""
        rows = connection.execute(
            f"""WITH ranked AS (
                    SELECT f.path, f.component_id, e.status, e.occurred_at, e.commit_sha,
                           row_number() OVER (
                               PARTITION BY f.path ORDER BY e.commit_position DESC, e.sequence DESC
                           ) AS rank
                    FROM files AS f
                    JOIN file_events AS e ON e.path = f.path
                    WHERE {clause} AND {condition}
                )
                SELECT * FROM ranked WHERE rank = 1 ORDER BY path""",
            (*parameters, boundary_value),
        )
        return [
            {
                "path": row["path"],
                "exists": row["status"] != "D",
                "status": row["status"],
                "at": row["occurred_at"],
                "commit": row["commit_sha"],
                "component": row["component_id"],
                "as_of": boundary.label,
            }
            for row in rows
        ]


def path_history(path: Path, project_path: str) -> list[dict[str, Any]]:
    with connect(path, readonly=True) as connection:
        clause, parameters = _path_clause(project_path)
        nodes = []
        for file_row in connection.execute(
            f"SELECT f.* FROM files AS f WHERE {clause} ORDER BY f.path", parameters
        ):
            events = [
                {
                    "commit": row["commit_sha"],
                    "at": row["occurred_at"],
                    "status": row["status"],
                }
                for row in connection.execute(
                    """SELECT commit_sha, occurred_at, status FROM file_events
                       WHERE path = ? ORDER BY sequence""",
                    (file_row["path"],),
                )
            ]
            nodes.append(
                {
                    "path": file_row["path"],
                    "exists": bool(file_row["exists_now"]),
                    "component": file_row["component_id"],
                    "change_count": file_row["change_count"],
                    "last_committed_change_at": file_row["last_committed_change_at"],
                    "events": events,
                }
            )
        return nodes


def resolve_entity(connection: sqlite3.Connection, value: str) -> str:
    exact = connection.execute(
        "SELECT entity_id FROM entities WHERE entity_id = ?", (value,)
    ).fetchone()
    if exact:
        return str(exact["entity_id"])
    candidates = list(
        connection.execute(
            """SELECT entity_id FROM entities
               WHERE entity_id IN (?, ?) OR (kind = 'commit' AND label LIKE ?)
               ORDER BY entity_id""",
            (f"file:{value}", f"component:{value}", value + "%"),
        )
    )
    if len(candidates) == 1:
        return str(candidates[0]["entity_id"])
    if not candidates:
        raise DatabaseError(f"no graph entity matches {value}")
    raise DatabaseError(f"graph entity is ambiguous: {value}")


def neighbors(
    path: Path, entity: str, *, depth: int = 1, point: str | None = None
) -> list[dict[str, Any]]:
    if depth < 1 or depth > 4:
        raise DatabaseError("graph traversal depth must be between 1 and 4")
    with connect(path, readonly=True) as connection:
        start = resolve_entity(connection, entity)
        boundary = resolve_boundary(connection, point) if point else None
        seen = {start}
        frontier = {start}
        results: list[dict[str, Any]] = []
        for current_depth in range(1, depth + 1):
            following: set[str] = set()
            for current in sorted(frontier):
                conditions = ["(r.source_id = ? OR r.target_id = ?)"]
                parameters: list[Any] = [current, current]
                if boundary and boundary.commit_position is not None:
                    conditions.append("(r.commit_position IS NULL OR r.commit_position <= ?)")
                    parameters.append(boundary.commit_position)
                elif boundary:
                    conditions.append("(r.event_time IS NULL OR r.event_time <= ?)")
                    parameters.append(boundary.timestamp)
                for row in connection.execute(
                    f"""SELECT r.*, source.kind AS source_kind, target.kind AS target_kind
                         FROM relations AS r
                         JOIN entities AS source ON source.entity_id = r.source_id
                         JOIN entities AS target ON target.entity_id = r.target_id
                         WHERE {' AND '.join(conditions)} ORDER BY r.relation_id""",
                    parameters,
                ):
                    outgoing = row["source_id"] == current
                    adjacent = row["target_id"] if outgoing else row["source_id"]
                    results.append(
                        {
                            "depth": current_depth,
                            "from": current,
                            "direction": "out" if outgoing else "in",
                            "predicate": row["predicate"],
                            "to": adjacent,
                            "event_time": row["event_time"],
                            "properties": json.loads(row["properties_json"]),
                        }
                    )
                    if adjacent not in seen:
                        following.add(adjacent)
            seen.update(following)
            frontier = following
            if not frontier:
                break
        unique = {compact(item): item for item in results}
        return [unique[key] for key in sorted(unique)]


def readonly_query(path: Path, query: str) -> tuple[list[str], list[dict[str, Any]]]:
    with connect(path, readonly=True) as connection:
        allowed = {
            sqlite3.SQLITE_FUNCTION,
            sqlite3.SQLITE_READ,
            sqlite3.SQLITE_SELECT,
            getattr(sqlite3, "SQLITE_RECURSIVE", -1),
        }

        def authorize(
            action: int, _first: str | None, second: str | None, _database: str | None,
            _trigger: str | None,
        ) -> int:
            if action in allowed or (action == sqlite3.SQLITE_PRAGMA and second is None):
                return sqlite3.SQLITE_OK
            return sqlite3.SQLITE_DENY

        connection.set_authorizer(authorize)
        try:
            cursor = connection.execute(query)
            if cursor.description is None:
                raise DatabaseError("query returned no result set")
            columns = [str(item[0]) for item in cursor.description]
            rows = [{column: row[column] for column in columns} for row in cursor]
            return columns, rows
        except sqlite3.DatabaseError as error:
            raise DatabaseError(str(error)) from error
