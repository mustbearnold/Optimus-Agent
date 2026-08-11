#!/usr/bin/env python3
"""Regression tests for the token-efficiency preservation audit.

The audit's contract (approved plan, section 4) is that a compression edit may
tighten prose but must not drop, relocate, or paraphrase any citation, marker,
or normative sentence that existed before. These tests pin down the extraction
sets, the superset diff, and the missing-file failure mode.
"""

from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "token_efficiency_audit",
    ROOT / "scripts" / "tools" / "token_efficiency_audit.py",
)
assert SPEC and SPEC.loader
AUDIT = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = AUDIT
SPEC.loader.exec_module(AUDIT)


class ExtractTests(unittest.TestCase):
    def test_extracts_backticked_paths_and_markdown_targets(self) -> None:
        text = (
            "See `scripts/verify.sh` and [docs](docs/architecture.md).\n"
            "Another `crates/optimus-core/src/lib.rs`."
        )
        payload = AUDIT.extract(text)
        self.assertIn("scripts/verify.sh", payload["backtick_refs"])
        self.assertIn("crates/optimus-core/src/lib.rs", payload["backtick_refs"])
        self.assertEqual(payload["md_links"], ["docs/architecture.md"])

    def test_normative_sentence_is_preserved_verbatim_normalised(self) -> None:
        text = "  The gate   must never   skip silently.\n"
        payload = AUDIT.extract(text)
        self.assertEqual(payload["normative"], ["The gate must never skip silently."])

    def test_plain_non_normative_line_is_not_collected(self) -> None:
        payload = AUDIT.extract("A purely descriptive sentence, nothing here.\n")
        self.assertEqual(payload["normative"], [])


class DiffSetTests(unittest.TestCase):
    def test_after_missing_a_citation_is_reported(self) -> None:
        before = AUDIT.extract("cite `scripts/verify.sh`\nand `docs/x.md`\n")
        after = AUDIT.extract("cite `scripts/verify.sh`\n")
        missing = AUDIT.diff_sets(before, after)
        self.assertEqual(missing, ["backtick_refs: 'docs/x.md'"])

    def test_superset_after_is_clean(self) -> None:
        before = AUDIT.extract("cite `scripts/verify.sh`\n")
        after = AUDIT.extract("cite `scripts/verify.sh`\nand `docs/x.md`\n")
        self.assertEqual(AUDIT.diff_sets(before, after), [])

    def test_compression_that_drops_a_normative_rule_is_reported(self) -> None:
        before = AUDIT.extract("It must never block a managed push.\n")
        after = AUDIT.extract("It must never block.\n")
        self.assertEqual(len(AUDIT.diff_sets(before, after)), 1)


class ReadSourceTests(unittest.TestCase):
    def test_read_source_returns_the_utf8_text(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "before.md"
            path.write_text("must keep this\n", encoding="utf-8")
            self.assertEqual(AUDIT.read_source(str(path)), "must keep this\n")

    def test_missing_file_fails_cleanly_naming_the_path(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            missing = Path(directory) / "does-not-exist.md"
            with self.assertRaises(SystemExit) as caught:
                AUDIT.read_source(str(missing))
            message = str(caught.exception)
            self.assertIn("cannot read", message)
            self.assertIn(str(missing), message)


if __name__ == "__main__":
    unittest.main(verbosity=2)
