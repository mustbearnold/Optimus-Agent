#!/usr/bin/env python3

"""Unit checks for scripts/gates/check-surface-contract.py (spec-015 A5)."""

from __future__ import annotations

import importlib.util
import pathlib
import subprocess
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "gates" / "check-surface-contract.py"


def load_gate():
    spec = importlib.util.spec_from_file_location("surface_contract", SCRIPT)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class SurfaceContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.mod = load_gate()

    def test_live_tree_contract_passes(self) -> None:
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
        self.assertIn("SURFACE_CONTRACT_OK", completed.stdout)

    def test_live_tree_dump_is_current(self) -> None:
        derived = self.mod.derive()
        schema = self.mod.load_schema()
        self.assertEqual(
            self.mod.load_dump(),
            self.mod.dump_payload(derived, schema),
            "registry dump is stale; run just surface-contract-dump",
        )

    def test_wire_set_formula(self) -> None:
        derived = self.mod.derive()
        # The formula: registry − non-wire − superseded + trio + protocol.
        expected = (
            derived["registry"]
            - derived["non_wire"]
            - derived["superseded"]
            | derived["trio"]
            | derived["protocol"]
        )
        self.assertEqual(derived["wire"], expected)
        # Shell-gated is a bucket inside the wire set, never in the union.
        self.assertTrue(derived["shell_gated"] <= derived["wire"])
        self.assertTrue(derived["shell_gated"] <= derived["registry"])

    def test_critical_minus_superseded_is_on_the_union(self) -> None:
        union = set(self.mod.parse_react_desktop_methods(self.mod.CONTRACTS_TS))
        missing = (
            self.mod.CRITICAL_INVOKE_METHODS
            - set(self.mod.parse_string_const(self.mod.CONTRACT_RS, "SUPERSEDED_CHAT_FAMILY"))
            - union
        )
        self.assertEqual(missing, set())

    def test_union_stays_inside_the_wire_surface(self) -> None:
        derived = self.mod.derive()
        union = set(self.mod.parse_react_desktop_methods(self.mod.CONTRACTS_TS))
        self.assertFalse(union - derived["wire"] - derived["shell_gated"])
        self.assertFalse(union & derived["superseded"])
        self.assertFalse(union & derived["non_wire"])
        self.assertFalse(union & derived["server_origin"])
        self.assertFalse(union & derived["shell_gated"], "staging is shell-kind only")

    def test_schema_pins_the_wire_surface(self) -> None:
        derived = self.mod.derive()
        schema = self.mod.load_schema()
        self.assertEqual(schema["methods"], derived["wire"])
        self.assertEqual(schema["events"], self.mod.EVENT_VOCABULARY)
        self.assertEqual(schema["protocol_version"], 1)

    def test_legacy_member_lives_only_in_the_shim_bucket(self) -> None:
        union = set(self.mod.parse_react_desktop_methods(self.mod.CONTRACTS_TS))
        self.assertNotIn("chat_approval_resolve", union)
        for name in self.mod.LEGACY_TRANSPORTS:
            text = (self.mod.UI_IPC_DIR / name).read_text(encoding="utf-8")
            self.assertIn("legacyInvoke", text)
        # The trio member is NOT the legacy member.
        self.assertIn("chat_approval_resolve_start", union)


if __name__ == "__main__":
    unittest.main()
