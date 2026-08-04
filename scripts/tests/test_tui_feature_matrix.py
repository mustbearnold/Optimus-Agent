#!/usr/bin/env python3


"""Unit checks for scripts/tui_feature_matrix.py box extraction."""


from __future__ import annotations


import pathlib, sys
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / "tools"))
import importlib.util
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "tools" / "tui_feature_matrix.py"


def load_matrix():
    spec = importlib.util.spec_from_file_location("tui_feature_matrix", SCRIPT)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class ComposerTextTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.mod = load_matrix()

    def test_sidebar_text_on_composer_border_row_is_not_composer_text(self) -> None:
        # Regression: the composer's bottom border shared a screen row with a
        # sidebar item named like the submitted draft ("picker-session-3").
        # Row-anchored extraction swallowed the sidebar row into the composer
        # box, so the submit-acceptance predicate could never turn true and
        # the gate timed out at 15s under verify pacing.
        frame = [
            "◆  WORKSPACE              │  /tmp/optimus  auto        ",
            "▾  SESSIONS 1–3/4         │  ✦ offline echo: probe      ",
            "›  PROJECTS               ┊╭───────────────────────────╮",
            "›  PINNED                  ││Ask Optimus anything…      │",
            "▸  picker-session-3       │╰─────────────────────── auto╯",
        ]
        text = self.mod.composer_text(frame)
        self.assertNotIn("picker-session-3", text)
        self.assertIn("Ask Optimus anything", text)

    def test_draft_is_visible_inside_the_box(self) -> None:
        frame = [
            "╭──────────────────────────────╮",
            "│  probe ping                  │",
            "╰──────────────────────────────╯",
        ]
        text = self.mod.composer_text(frame)
        self.assertIn("probe ping", text)

    def test_no_box_returns_empty(self) -> None:
        self.assertEqual(self.mod.composer_text(["no box here", "plain text"]), "")

    def test_torn_top_border_falls_back_to_row_range(self) -> None:
        # A torn capture can lose the ╮ corner; the fallback must still find
        # the draft so the pre-submit wait keeps working.
        frame = [
            "╭─────────────────────────────",
            "│  draft text                 │",
            "╰─────────────────────────────╯",
        ]
        text = self.mod.composer_text(frame)
        self.assertIn("draft text", text)


if __name__ == "__main__":
    unittest.main()
