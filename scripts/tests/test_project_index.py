#!/usr/bin/env python3


"""Regression tests for the deterministic root folder index (index.md)."""


from __future__ import annotations


import pathlib, sys
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / "tools"))
import json
import subprocess
import tempfile
import unittest
from pathlib import Path

import project_index


def make_repo(base: Path) -> Path:
    subprocess.run(["git", "init", "-q"], cwd=base, check=True)
    return base


def write(repo: Path, relative: str, text: str = "content") -> None:
    path = repo / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def stage_all(repo: Path) -> None:
    subprocess.run(["git", "add", "-A"], cwd=repo, check=True)


def fixture_repo(base: Path) -> Path:
    repo = make_repo(base)
    write(repo, "README.md", "# fixture")
    write(repo, "crates/foo/Cargo.toml", (
        '[package]\nname = "foo"\nversion = "0.1.0"\n'
        'description = "Typed fixture crate for index tests."\n'
    ))
    write(repo, "crates/foo/src/lib.rs", "")
    write(repo, "docs/a.md", "# doc")
    return repo


class ProjectIndexTests(unittest.TestCase):
    def setUp(self) -> None:
        self._temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self._temporary.cleanup)
        self.repo = fixture_repo(Path(self._temporary.name))
        stage_all(self.repo)

    def read_index(self) -> str:
        return (self.repo / "index.md").read_text(encoding="utf-8")

    def test_generate_lists_every_folder(self) -> None:
        project_index.generate(self.repo)
        content = self.read_index()
        for folder in ("crates/", "foo/", "src/", "docs/"):
            self.assertIn(folder, content)
        self.assertIn("**4 folders**", content)
        self.assertIn("**5 files**", content)

    def test_generate_lists_itself_from_the_first_generation(self) -> None:
        first = project_index.generate(self.repo)
        self.assertIn("index.md", first)
        second = project_index.generate(self.repo)
        self.assertEqual(first, second)

    def test_check_detects_drift_and_recovers(self) -> None:
        project_index.generate(self.repo)
        self.assertEqual(project_index.check(self.repo)["folders"], 4)
        write(self.repo, "specs/001-feature/spec.md", "# spec")
        stage_all(self.repo)
        with self.assertRaises(project_index.ProjectIndexError):
            project_index.check(self.repo)
        project_index.generate(self.repo)
        result = project_index.check(self.repo)
        self.assertEqual(result["folders"], 6)
        self.assertIn("specs/", self.read_index())

    def test_gitignored_directories_are_excluded(self) -> None:
        write(self.repo, ".gitignore", "build/\n")
        write(self.repo, "build/junk.txt", "junk")
        stage_all(self.repo)
        project_index.generate(self.repo)
        self.assertNotIn("build/", self.read_index())

    def test_component_summary_annotation(self) -> None:
        write(self.repo, "docs/repository-components.json", json.dumps({
            "schema_version": 1,
            "components": [
                {"path": "crates", "summary": "Governed fixture summary for crates."},
            ],
        }))
        stage_all(self.repo)
        project_index.generate(self.repo)
        self.assertIn("Governed fixture summary for crates.", self.read_index())

    def test_manifest_description_annotation(self) -> None:
        project_index.generate(self.repo)
        self.assertIn("Typed fixture crate for index tests.", self.read_index())

    def test_check_missing_index_fails(self) -> None:
        with self.assertRaises(project_index.ProjectIndexError):
            project_index.check(self.repo)


if __name__ == "__main__":
    unittest.main()
