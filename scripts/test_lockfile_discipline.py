#!/usr/bin/env python3
"""Regression tests for the flat-monorepo package-manager discipline gate."""

from __future__ import annotations

import importlib.util
import json
import subprocess
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("check-lockfile-discipline.py")
SPEC = importlib.util.spec_from_file_location("lockfile_discipline", SCRIPT)
assert SPEC and SPEC.loader
lockfile_discipline = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(lockfile_discipline)


class LockfileDisciplineTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        for name in ("Cargo.lock", "bun.lock"):
            (self.root / name).write_text("", encoding="utf-8")
        (self.root / "package.json").write_text(
            json.dumps({"name": "test", "packageManager": "bun@1.3.14"}),
            encoding="utf-8",
        )
        subprocess.run(["git", "init", "-q"], cwd=self.root, check=True)
        subprocess.run(["git", "add", "-A"], cwd=self.root, check=True)

    def tearDown(self) -> None:
        self.temp.cleanup()

    def test_accepts_cargo_and_bun_only(self) -> None:
        self.assertEqual(lockfile_discipline.findings(self.root), [])

    def test_rejects_foreign_lockfile_anywhere(self) -> None:
        nested = self.root / "apps" / "optimus-ui"
        nested.mkdir(parents=True)
        (nested / "package-lock.json").write_text("{}", encoding="utf-8")
        subprocess.run(
            ["git", "add", "-f", "apps/optimus-ui/package-lock.json"],
            cwd=self.root,
            check=True,
        )
        problems = lockfile_discipline.findings(self.root)
        self.assertTrue(any("package-lock.json" in item for item in problems))

    def test_rejects_missing_root_lockfile(self) -> None:
        (self.root / "bun.lock").unlink()
        subprocess.run(["git", "add", "-A"], cwd=self.root, check=True)
        problems = lockfile_discipline.findings(self.root)
        self.assertTrue(any("bun.lock" in item for item in problems))

    def test_rejects_non_bun_package_manager(self) -> None:
        (self.root / "package.json").write_text(
            json.dumps({"name": "test", "packageManager": "pnpm@9.0.0"}),
            encoding="utf-8",
        )
        problems = lockfile_discipline.findings(self.root)
        self.assertTrue(any("packageManager" in item for item in problems))


if __name__ == "__main__":
    unittest.main()
