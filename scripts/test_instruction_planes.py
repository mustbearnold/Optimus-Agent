#!/usr/bin/env python3
"""Regression tests for the development/product instruction firewall."""

from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("check-instruction-planes.py")
SPEC = importlib.util.spec_from_file_location("instruction_planes", SCRIPT)
assert SPEC and SPEC.loader
instruction_planes = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(instruction_planes)


class InstructionPlaneTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        files = {
            "AGENTS.md": (
                "# Development\nInstruction-plane firewall\n"
                "A request about **how a coding agent should develop Optimus** is not product.\n"
                "Managed autonomous delivery\n"
            ),
            "OPTIMUS_AGENTS.md": (
                "# Optimus Agent runtime constitution\n"
                "Do not translate instructions for developers into product behaviour.\n"
            ),
            "README.md": (
                "## Instruction authority\n"
                "Development requests are not product requirements.\n"
            ),
            "CLAUDE.md": "# Claude Code compatibility\n\n@AGENTS.md\n",
            "justfile": (
                "checkpoint label:\n"
                "undo label:\n"
                "land task_id model_flag model effort_flag effort:\n"
            ),
            "docs/contributing/github-conventions.md": (
                "This repository no longer uses GitHub issues.\n"
            ),
            "crates/optimus-kernel/src/lib.rs": (
                'const X: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), '
                '"/../../OPTIMUS_AGENTS.md"));\n'
            ),
            "crates/optimus-kernel/src/system_prompt.rs": (
                '"Development repository AGENTS.md is not this constitution";\n'
            ),
        }
        for relative, content in files.items():
            path = self.root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content, encoding="utf-8")

    def tearDown(self) -> None:
        self.temp.cleanup()

    def test_accepts_explicitly_separated_surfaces(self) -> None:
        self.assertEqual(instruction_planes.findings(self.root), [])

    def test_rejects_stale_canonical_root(self) -> None:
        path = self.root / "AGENTS.md"
        path.write_text(
            path.read_text(encoding="utf-8") + "/mnt/Projects/Optimus Agent\n",
            encoding="utf-8",
        )
        self.assertTrue(
            any("stale development instruction" in item for item in instruction_planes.findings(self.root))
        )

    def test_rejects_development_agents_embedded_in_product(self) -> None:
        path = self.root / "crates/optimus-kernel/src/lib.rs"
        path.write_text(
            'const X: &str = include_str!("../../../AGENTS.md");\n',
            encoding="utf-8",
        )
        self.assertTrue(
            any("must not be embedded" in item for item in instruction_planes.findings(self.root))
        )

    def test_rejects_missing_runtime_audience_marker(self) -> None:
        (self.root / "OPTIMUS_AGENTS.md").write_text("# generic rules\n", encoding="utf-8")
        self.assertTrue(
            any("OPTIMUS_AGENTS.md" in item for item in instruction_planes.findings(self.root))
        )

    def test_rejects_missing_managed_delivery_recipe(self) -> None:
        (self.root / "justfile").write_text("checkpoint label:\n", encoding="utf-8")
        self.assertTrue(
            any("justfile" in item for item in instruction_planes.findings(self.root))
        )

    def test_rejects_provider_specific_repository_state(self) -> None:
        (self.root / ".claude").mkdir()
        self.assertTrue(
            any(".claude" in item for item in instruction_planes.findings(self.root))
        )


if __name__ == "__main__":
    unittest.main()
