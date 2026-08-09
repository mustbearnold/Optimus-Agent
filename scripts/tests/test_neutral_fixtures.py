#!/usr/bin/env python3


"""Unit checks for scripts/gates/check-neutral-fixtures.py."""


from __future__ import annotations


import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "gates" / "check-neutral-fixtures.py"


def load_module():
    spec = importlib.util.spec_from_file_location("check_neutral_fixtures", SCRIPT)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


class NeutralFixturesGateTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.mod = load_module()

    def _write_app_source(self, root: Path, relative: str, text: str) -> Path:
        path = root / "apps" / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
        return path

    def test_neutral_root_is_allowed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self._write_app_source(
                root,
                "fixture.ts",
                "const bridge = '/projects/optimus-agent/resolver';\n",
            )
            self.assertEqual(self.mod.scan_tree(root), [])

    def test_machine_specific_home_path_is_detected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self._write_app_source(
                root,
                "fixture.ts",
                "const root = '/home/alice/Projects/optimus/resolver';\n",
            )
            findings = self.mod.scan_tree(root)
            self.assertEqual(len(findings), 1, findings)
            self.assertIn("fixture.ts:1", findings[0])

    def test_skipped_directories_are_not_scanned(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self._write_app_source(
                root,
                "node_modules/pkg/index.js",
                "const root = '/home/alice/Projects/optimus/resolver';\n",
            )
            self.assertEqual(self.mod.scan_tree(root), [])

    def test_non_matching_extension_is_ignored(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self._write_app_source(
                root,
                "fixture.txt",
                "const root = '/home/alice/Projects/optimus/resolver';\n",
            )
            self.assertEqual(self.mod.scan_tree(root), [])


if __name__ == "__main__":
    unittest.main()
