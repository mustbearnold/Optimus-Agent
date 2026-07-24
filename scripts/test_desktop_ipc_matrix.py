#!/usr/bin/env python3
"""Unit checks for scripts/check-desktop-ipc-matrix.py against the live tree."""

from __future__ import annotations

import importlib.util
import subprocess
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "check-desktop-ipc-matrix.py"


def load_matrix():
    spec = importlib.util.spec_from_file_location("desktop_ipc_matrix", SCRIPT)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class DesktopIpcMatrixTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.mod = load_matrix()

    def test_live_tree_matrix_passes(self) -> None:
        completed = subprocess.run(
            [sys.executable, str(SCRIPT)],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(
            completed.returncode,
            0,
            msg=completed.stdout + "\n" + completed.stderr,
        )
        self.assertIn("DESKTOP_IPC_MATRIX_OK", completed.stdout)

    def test_critical_paths_include_approval_and_project_scope(self) -> None:
        critical = self.mod.CRITICAL_INVOKE_METHODS
        self.assertIn("chat_approval_resolve", critical)
        self.assertIn("project_scopes_authorize", critical)
        self.assertIn("get_session", critical)
        self.assertNotIn("project_root_stage_native", critical)

    def test_parsers_return_sorted_unique_critical_subset(self) -> None:
        rust = self.mod.parse_rust_registry(
            ROOT / "apps/optimus-desktop/src/ipc/router.rs"
        )
        electron = self.mod.parse_electron_allowlist(
            ROOT / "apps/optimus-electron/main.cjs"
        )
        react = self.mod.parse_react_desktop_methods(
            ROOT / "apps/optimus-ui/src/ipc/contracts.ts"
        )
        self.assertTrue(set(self.mod.CRITICAL_INVOKE_METHODS).issubset(set(rust)))
        self.assertEqual(set(electron), set(react))
        self.assertTrue(set(electron).issubset(set(rust)))


if __name__ == "__main__":
    unittest.main()
