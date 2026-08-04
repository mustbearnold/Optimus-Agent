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

    def test_code_after_a_test_module_is_still_counted(self) -> None:
        """The bug this metric was rewritten for.

        Truncating at the first `#[cfg(test)]` measures "lines before the first
        test module", which is a different number that usually agrees. When it
        disagrees it under-reports: `lib.rs` once read 912 while holding ~1200
        production lines, because a test module two-thirds down hid the rest.
        A gate on that number rewards moving a test module up the file.
        """
        path = self._write(
            "fn a() {}\n#[cfg(test)]\nmod t {\n"
            + "    // test\n" * 500
            + "}\nfn b() {}\nfn c() {}\n"
        )
        try:
            self.assertEqual(CMS.production_lines(path), 3)
        finally:
            path.unlink()

    def test_module_declarations_do_not_count(self) -> None:
        """Splitting a module must not push its declaring file over the ratchet.

        `mod x;` is a registry line, not logic. Counting it means complying with
        the law in one file can break the law in another — the file that
        declares the new module gains a line for doing the right thing.
        """
        path = self._write(
            "mod a;\npub mod b;\npub(crate) mod c;\nfn real() {}\nmod d { fn x() {} }\n"
        )
        try:
            # `mod d { … }` has a body, so it is real code and still counts.
            self.assertEqual(CMS.production_lines(path), 2)
        finally:
            path.unlink()

    def test_a_brace_in_a_literal_does_not_desynchronise_the_skip(self) -> None:
        path = self._write(
            'fn a() {}\n#[cfg(test)]\nmod t {\n    const S: &str = "}";\n'
            "    // {\n}\nfn b() {}\n"
        )
        try:
            self.assertEqual(CMS.production_lines(path), 2)
        finally:
            path.unlink()

    def test_a_lifetime_is_not_read_as_a_char_literal(self) -> None:
        """`'a` has no closing quote.

        Scanned as a char literal it swallows the rest of the file looking for
        one, taking every brace after it and the whole count with it.
        """
        path = self._write(
            "fn a<'x>(v: &'x str) -> &'x str { v }\n#[cfg(test)]\nmod t {\n}\nfn b() {}\n"
        )
        try:
            self.assertEqual(CMS.production_lines(path), 2)
        finally:
            path.unlink()

    def test_conditional_test_attributes_are_recognised(self) -> None:
        path = self._write(
            'fn a() {}\n#[cfg(all(test, target_os = "linux"))]\nmod t {\n}\nfn b() {}\n'
        )
        try:
            self.assertEqual(CMS.production_lines(path), 2)
        finally:
            path.unlink()

    def test_a_feature_named_test_is_production_code(self) -> None:
        path = self._write('#[cfg(feature = "test")]\nfn a() {}\nfn b() {}\n')
        try:
            self.assertEqual(CMS.production_lines(path), 3)
        finally:
            path.unlink()

    def test_a_single_line_test_item_does_not_swallow_the_rest_of_the_file(
        self,
    ) -> None:
        """`mod t {}` opens and closes without ever raising the brace depth.

        Waiting for the depth to come back down therefore waits forever, and
        every line after it is counted as test code — the file reads as far
        smaller than it is, which is the failure mode this whole metric exists
        to remove.
        """
        path = self._write("#[cfg(test)]\nmod t {}\nfn a() {}\nfn b() {}\nfn c() {}\n")
        try:
            self.assertEqual(CMS.production_lines(path), 3)
        finally:
            path.unlink()

    def test_a_cfg_test_item_without_a_block_ends_at_its_statement(self) -> None:
        path = self._write("#[cfg(test)]\nuse std::fmt;\nfn a() {}\nfn b() {}\n")
        try:
            self.assertEqual(CMS.production_lines(path), 2)
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
            ROOT / "docs" / "architecture.md"
        ).read_text(encoding="utf-8")
        self.assertIn("no module > ~800 LOC", blueprint)
        self.assertEqual(CMS.LIMIT, 800)


if __name__ == "__main__":
    unittest.main(verbosity=2)
