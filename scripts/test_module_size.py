#!/usr/bin/env python3
"""Self-tests for the module-size ratchet gate."""

from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
_spec = importlib.util.spec_from_file_location(
    "check_module_size", ROOT / "scripts" / "check-module-size.py"
)
CMS = importlib.util.module_from_spec(_spec)
assert _spec.loader is not None
_spec.loader.exec_module(CMS)


class ProductionLinesTests(unittest.TestCase):
    def _write(self, body: str) -> Path:
        handle = tempfile.NamedTemporaryFile(
            "w", suffix=".rs", delete=False, encoding="utf-8"
        )
        handle.write(body)
        handle.close()
        return Path(handle.name)

    def test_counts_every_line_without_an_inline_test_module(self) -> None:
        path = self._write("fn a() {}\nfn b() {}\nfn c() {}\n")
        try:
            self.assertEqual(CMS.production_lines(path), 3)
        finally:
            path.unlink()

    def test_inline_test_module_is_not_counted(self) -> None:
        """A file's own tests must not count against it.

        Otherwise the gate penalises good inline coverage, which inverts the
        rule's purpose.
        """
        path = self._write(
            "fn a() {}\nfn b() {}\n#[cfg(test)]\nmod tests {\n"
            + "    // test\n" * 500
            + "}\n"
        )
        try:
            self.assertEqual(CMS.production_lines(path), 2)
        finally:
            path.unlink()

    def test_indented_cfg_test_attribute_is_recognised(self) -> None:
        path = self._write("fn a() {}\n    #[cfg(test)]\n    mod t {}\n")
        try:
            self.assertEqual(CMS.production_lines(path), 1)
        finally:
            path.unlink()


class BaselineTests(unittest.TestCase):
    def test_baseline_matches_the_current_tree(self) -> None:
        """The committed baseline must be exactly the over-limit set.

        A stale baseline silently grandfathers files that were already split,
        or omits ones that grew — either way the ratchet stops ratcheting.
        """
        sizes = CMS.measure()
        expected = {
            name: size for name, size in sizes.items() if size > CMS.LIMIT
        }
        self.assertEqual(CMS.load_baseline(), expected)

    def test_no_baselined_file_is_at_or_under_the_limit(self) -> None:
        for name, size in CMS.load_baseline().items():
            self.assertGreater(
                size, CMS.LIMIT, f"{name} is at or under the limit; retire it"
            )

    def test_baseline_entries_all_exist(self) -> None:
        for name in CMS.load_baseline():
            self.assertTrue((ROOT / name).exists(), f"{name} is baselined but gone")

    def test_limit_matches_the_documented_law(self) -> None:
        blueprint = (
            ROOT / "docs" / "architecture" / "optimus-exceeds-hermes.md"
        ).read_text(encoding="utf-8")
        self.assertIn("no module > ~800 LOC", blueprint)
        self.assertEqual(CMS.LIMIT, 800)


if __name__ == "__main__":
    unittest.main(verbosity=2)
