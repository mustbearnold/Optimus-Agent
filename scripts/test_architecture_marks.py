#!/usr/bin/env python3
"""Unit checks for scripts/check-architecture-marks.py."""

from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "check-architecture-marks.py"


def load_module():
    spec = importlib.util.spec_from_file_location("check_architecture_marks", SCRIPT)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    # dataclasses + from __future__ annotations need the module registered first
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


class ArchitectureMarksGateTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.mod = load_module()

    def test_live_tree_passes_or_reports_honest_gap(self) -> None:
        """Live tree: all current S+++ marks must satisfy the map.

        During P17 work Release may still be A until marks land; the live script
        should exit 0 only when claims and paths align. If Release is still A,
        the script still exits 0 (no false S+++). After R4 lands S+++, it still
        exits 0 because verification + paths exist.
        """
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
        self.assertIn("architecture marks check OK", completed.stdout)

    def test_splus_without_phase_done_fails(self) -> None:
        fake_marks = """
## Current grades (test)

| Mark | Grade | Notes |
|---|:---:|---|
| Release / parity gating | **S+++** | greenwash |
| Doc / claim hygiene | **A-** | ok |

## Program phases

| Phase | Focus | Marks moved | Status |
|---|---|---|---|
| P16 | Doc | Doc | **done** |
| P17 | Release | Release | pending |
"""
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            # Required paths missing → multiple findings.
            findings = self.mod.check(
                fake_marks,
                root=root,
                require_program_mentions=False,
            )
        self.assertTrue(any("P17" in f and "pending" in f for f in findings), findings)
        self.assertTrue(any("required path" in f or "requires existing" in f for f in findings), findings)

    def test_splus_with_done_phase_and_paths_passes(self) -> None:
        fake_marks = """
## Current grades (test)

| Mark | Grade | Notes |
|---|:---:|---|
| Doc / claim hygiene | **S+++** | ok |

## Program phases

| Phase | Focus | Marks moved | Status |
|---|---|---|---|
| P16 | Doc / claim hygiene pass | Doc A-→**S+++** | **done** |
"""
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            em = root / "scripts" / "engineering_memory.py"
            em.parent.mkdir(parents=True)
            em.write_text("# stub\n", encoding="utf-8")
            ver = root / "docs" / "architecture" / "s-plus-plus-plus-p16-verification.md"
            ver.parent.mkdir(parents=True)
            ver.write_text("# ok\n", encoding="utf-8")
            findings = self.mod.check(
                fake_marks,
                root=root,
                require_program_mentions=False,
            )
        self.assertEqual(findings, [])

    def test_non_splus_does_not_require_phase(self) -> None:
        fake_marks = """
## Current grades (test)

| Mark | Grade | Notes |
|---|:---:|---|
| Durability / crash safety | **A+** | residual |

## Program phases

| Phase | Focus | Marks moved | Status |
|---|---|---|---|
| P18 | Durability | Durability | pending |
"""
        findings = self.mod.check(
            fake_marks,
            root=ROOT,
            require_program_mentions=False,
        )
        self.assertEqual(findings, [])

    def test_not_done_status_is_not_done(self) -> None:
        """Negated status must not greenwash S+++ (substring 'done')."""
        for status in ("not done", "undone", "**not done**"):
            with self.subTest(status=status):
                fake_marks = f"""
## Current grades (test)

| Mark | Grade | Notes |
|---|:---:|---|
| Release / parity gating | **S+++** | greenwash |

## Program phases

| Phase | Focus | Marks moved | Status |
|---|---|---|---|
| P17 | Release | Release | {status} |
"""
                with tempfile.TemporaryDirectory() as tmp:
                    root = Path(tmp)
                    for rel in (
                        "scripts/optimus_version.py",
                        "scripts/check-parity-ledger.py",
                        "scripts/check-architecture-marks.py",
                        "docs/architecture/release-and-parity-gates.md",
                        "docs/architecture/s-plus-plus-plus-p17-verification.md",
                    ):
                        path = root / rel
                        path.parent.mkdir(parents=True, exist_ok=True)
                        path.write_text("stub\n", encoding="utf-8")
                    findings = self.mod.check(
                        fake_marks,
                        root=root,
                        require_program_mentions=False,
                    )
                self.assertTrue(
                    any(
                        "P17" in f and "status is" in f and "'done'" not in f
                        for f in findings
                    ),
                    findings,
                )

    def test_unbolded_splus_is_detected(self) -> None:
        fake_marks = """
## Current grades (test)

| Mark | Grade | Notes |
|---|:---:|---|
| Release / parity gating | S+++ | unbolded |

## Program phases

| Phase | Focus | Marks moved | Status |
|---|---|---|---|
| P17 | Release | Release | pending |
"""
        findings = self.mod.check(
            fake_marks,
            root=ROOT,
            require_program_mentions=False,
        )
        self.assertTrue(any("P17" in f for f in findings), findings)

    def test_missing_verification_only_fails(self) -> None:
        fake_marks = """
## Current grades (test)

| Mark | Grade | Notes |
|---|:---:|---|
| Release / parity gating | **S+++** | missing ver |

## Program phases

| Phase | Focus | Marks moved | Status |
|---|---|---|---|
| P17 | Release | Release A→**S+++** | **done** |
"""
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for rel in (
                "scripts/optimus_version.py",
                "scripts/check-parity-ledger.py",
                "scripts/check-architecture-marks.py",
                "docs/architecture/release-and-parity-gates.md",
            ):
                path = root / rel
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("stub\n", encoding="utf-8")
            findings = self.mod.check(
                fake_marks,
                root=root,
                require_program_mentions=False,
            )
        self.assertTrue(any("verification" in f for f in findings), findings)

    def test_unregistered_mark_splus_fails(self) -> None:
        fake_marks = """
## Current grades (test)

| Mark | Grade | Notes |
|---|:---:|---|
| Totally new mark | **S+++** | unregistered |

## Program phases

| Phase | Focus | Marks moved | Status |
|---|---|---|---|
| P99 | x | x | **done** |
"""
        findings = self.mod.check(
            fake_marks,
            root=ROOT,
            require_program_mentions=False,
        )
        self.assertTrue(any("no requirement map" in f for f in findings), findings)


if __name__ == "__main__":
    unittest.main()
