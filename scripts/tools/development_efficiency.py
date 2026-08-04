#!/usr/bin/env python3
"""Emit compact, honest development-efficiency telemetry.

The report is intentionally local and deterministic. It reads execution
manifests and the Developer Full Access JSONL action log, never estimates
tokens from characters, and keeps provider-missing usage visible as unknown.
"""

from __future__ import annotations

import argparse
import json
import sqlite3
from pathlib import Path
from typing import Any, Iterable


USAGE_COLUMNS = (
    "input_tokens",
    "output_tokens",
    "total_tokens",
    "reasoning_tokens",
    "cached_input_tokens",
    "cache_write_tokens",
)


def percentile(values: Iterable[int], quantile: float) -> int | None:
    ordered = sorted(values)
    if not ordered:
        return None
    if len(ordered) == 1:
        return ordered[0]
    position = (len(ordered) - 1) * quantile
    lower = int(position)
    upper = min(lower + 1, len(ordered) - 1)
    fraction = position - lower
    return round(ordered[lower] + (ordered[upper] - ordered[lower]) * fraction)


def optional_sum(values: Iterable[int | None]) -> int | None:
    present = [value for value in values if value is not None]
    return sum(present) if present else None


def table_columns(connection: sqlite3.Connection, table: str) -> set[str]:
    return {row[1] for row in connection.execute(f"PRAGMA table_info({table})")}


def execution_report(path: Path) -> dict[str, Any]:
    with sqlite3.connect(path) as connection:
        manifests = connection.execute(
            "SELECT status,duration_ms FROM execution_manifests"
        ).fetchall()
        model_columns = table_columns(connection, "execution_model_calls")
        selected_usage = [column for column in USAGE_COLUMNS if column in model_columns]
        model_rows = connection.execute(
            "SELECT "
            + ",".join(["duration_ms", *selected_usage])
            + " FROM execution_model_calls"
        ).fetchall()
        tool_columns = table_columns(connection, "execution_tool_calls")
        suppressed_expression = "suppressed" if "suppressed" in tool_columns else "0"
        tool_rows = connection.execute(
            f"SELECT duration_ms,{suppressed_expression} FROM execution_tool_calls"
        ).fetchall()

    accounted = 0
    token_totals: dict[str, int | None] = {column: None for column in USAGE_COLUMNS}
    for index, column in enumerate(selected_usage):
        token_totals[column] = optional_sum(row[index + 1] for row in model_rows)
    for row in model_rows:
        if any(value is not None for value in row[1:]):
            accounted += 1

    statuses: dict[str, int] = {}
    for status, _duration in manifests:
        statuses[status] = statuses.get(status, 0) + 1
    return {
        "manifests": len(manifests),
        "status": dict(sorted(statuses.items())),
        "wall_ms": {
            "sum": sum(duration or 0 for _status, duration in manifests),
            "p50": percentile((duration or 0 for _status, duration in manifests), 0.50),
            "p95": percentile((duration or 0 for _status, duration in manifests), 0.95),
        },
        "model_calls": {
            "count": len(model_rows),
            "accounted": accounted,
            "unaccounted": len(model_rows) - accounted,
            "tokens": token_totals,
        },
        "tool_calls": {
            "count": len(tool_rows),
            "suppressed": sum(1 for _duration, suppressed in tool_rows if suppressed),
            "duration_ms": {
                "sum": sum(duration or 0 for duration, _suppressed in tool_rows),
                "p50": percentile((duration or 0 for duration, _suppressed in tool_rows), 0.50),
                "p95": percentile((duration or 0 for duration, _suppressed in tool_rows), 0.95),
            },
        },
    }


def action_report(path: Path | None) -> dict[str, Any]:
    if path is None or not path.is_file():
        return {"available": False, "count": 0}
    durations: list[int] = []
    outcomes: dict[str, int] = {}
    malformed = 0
    for line in path.read_text(encoding="utf-8").splitlines():
        try:
            record = json.loads(line)
            duration = record.get("duration_ms")
            outcome = record.get("outcome", "unknown")
            if not isinstance(duration, int) or duration < 0:
                malformed += 1
                continue
            durations.append(duration)
            outcomes[outcome] = outcomes.get(outcome, 0) + 1
        except json.JSONDecodeError:
            malformed += 1
    return {
        "available": True,
        "count": len(durations),
        "malformed": malformed,
        "outcome": dict(sorted(outcomes.items())),
        "duration_ms": {
            "sum": sum(durations),
            "p50": percentile(durations, 0.50),
            "p95": percentile(durations, 0.95),
        },
    }


def build_report(database: Path, actions: Path | None) -> dict[str, Any]:
    return {
        "schema": 1,
        "execution": execution_report(database),
        "developer_actions": action_report(actions),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--db", required=True, type=Path, help="execution SQLite database")
    parser.add_argument("--actions", type=Path, help="Developer Full Access actions.log")
    parser.add_argument("--pretty", action="store_true")
    args = parser.parse_args()
    report = build_report(args.db, args.actions)
    print(json.dumps(report, indent=2 if args.pretty else None, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
