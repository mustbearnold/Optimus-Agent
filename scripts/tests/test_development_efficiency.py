#!/usr/bin/env python3


"""Tests for the compact development-efficiency report."""


from __future__ import annotations


import pathlib, sys
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / "tools"))
import json
import sqlite3
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import development_efficiency as efficiency  # noqa: E402


class DevelopmentEfficiencyTest(unittest.TestCase):
    def test_report_keeps_unknown_provider_usage_explicit(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            database = root / "execution.db"
            with sqlite3.connect(database) as connection:
                connection.executescript(
                    """
                    CREATE TABLE execution_manifests(status TEXT, duration_ms INTEGER);
                    CREATE TABLE execution_model_calls(
                      duration_ms INTEGER, input_tokens INTEGER, output_tokens INTEGER,
                      total_tokens INTEGER, reasoning_tokens INTEGER,
                      cached_input_tokens INTEGER, cache_write_tokens INTEGER
                    );
                    CREATE TABLE execution_tool_calls(duration_ms INTEGER, suppressed INTEGER);
                    INSERT INTO execution_manifests VALUES ('succeeded', 100);
                    INSERT INTO execution_manifests VALUES ('failed', 300);
                    INSERT INTO execution_model_calls VALUES (80, 10, 4, 14, 1, 2, NULL);
                    INSERT INTO execution_model_calls VALUES (90, NULL, NULL, NULL, NULL, NULL, NULL);
                    INSERT INTO execution_tool_calls VALUES (20, 0);
                    INSERT INTO execution_tool_calls VALUES (30, 1);
                    """
                )
            actions = root / "actions.log"
            actions.write_text(
                json.dumps({"duration_ms": 7, "outcome": "ok"})
                + "\n"
                + json.dumps({"duration_ms": 11, "outcome": "error"})
                + "\n",
                encoding="utf-8",
            )

            report = efficiency.build_report(database, actions)

        self.assertEqual(report["execution"]["model_calls"]["accounted"], 1)
        self.assertEqual(report["execution"]["model_calls"]["unaccounted"], 1)
        self.assertEqual(report["execution"]["model_calls"]["tokens"]["total_tokens"], 14)
        self.assertEqual(report["execution"]["tool_calls"]["suppressed"], 1)
        self.assertEqual(report["developer_actions"]["duration_ms"]["p95"], 11)

    def test_missing_action_log_is_not_a_zero_measurement(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            database = Path(directory) / "execution.db"
            with sqlite3.connect(database) as connection:
                connection.executescript(
                    """
                    CREATE TABLE execution_manifests(status TEXT, duration_ms INTEGER);
                    CREATE TABLE execution_model_calls(duration_ms INTEGER);
                    CREATE TABLE execution_tool_calls(duration_ms INTEGER, suppressed INTEGER);
                    """
                )
            report = efficiency.build_report(database, None)
        self.assertFalse(report["developer_actions"]["available"])


if __name__ == "__main__":
    unittest.main()
