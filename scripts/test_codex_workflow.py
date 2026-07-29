#!/usr/bin/env python3
"""Unit tests for the fail-closed Codex workflow gate (#120)."""

from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path
from types import ModuleType

SCRIPT = Path(__file__).with_name("check-codex-workflow.py")
SPEC = importlib.util.spec_from_file_location("check_codex_workflow", SCRIPT)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"cannot import {SCRIPT}")
workflow: ModuleType = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(workflow)


class CodexWorkflowGateTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self._write_valid_fixture()

    def tearDown(self) -> None:
        self.temp.cleanup()

    def _write(self, relative: str, text: str) -> None:
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")

    def _write_valid_fixture(self) -> None:
        for relative, markers in workflow.REQUIRED_MARKERS.items():
            self._write(relative, "\n".join(markers) + "\n")
        for relative in workflow.ISSUE_FORMS:
            fields = "\n".join(f"  id: {field}" for field in workflow.ISSUE_FIELDS)
            self._write(relative, fields + "\n")

    def test_complete_contract_passes(self) -> None:
        self.assertEqual(workflow.validate(self.root), [])

    def test_missing_required_marker_fails(self) -> None:
        path = self.root / "AGENTS.md"
        path.write_text("incomplete\n", encoding="utf-8")
        errors = workflow.validate(self.root)
        self.assertTrue(any("missing workflow marker" in error for error in errors))

    def test_missing_issue_field_fails(self) -> None:
        path = self.root / workflow.ISSUE_FORMS[0]
        text = path.read_text(encoding="utf-8").replace("  id: done_when\n", "")
        path.write_text(text, encoding="utf-8")
        errors = workflow.validate(self.root)
        self.assertIn(
            f"{workflow.ISSUE_FORMS[0]}: missing required field id 'done_when'",
            errors,
        )

    def test_legacy_manual_workflow_fails(self) -> None:
        path = self.root / "AGENTS.md"
        with path.open("a", encoding="utf-8") as handle:
            handle.write("One session → one PR → merged that session\n")
        errors = workflow.validate(self.root)
        self.assertTrue(any("forbidden legacy workflow marker" in error for error in errors))


if __name__ == "__main__":
    unittest.main()
