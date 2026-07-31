#!/usr/bin/env python3
"""Regression tests for the executable repository component authority."""

from __future__ import annotations

import copy
import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import repository_ontology as ontology


def component(component_id: str, path: str, **overrides: object) -> dict[str, object]:
    value: dict[str, object] = {
        "id": component_id,
        "path": path,
        "storage_plane": "repository",
        "concern": "development-tooling",
        "distribution": "repository-only",
        "lifecycle": "primary",
        "repository_role": "required",
        "retention": "essential",
        "summary": f"Bounded purpose for {component_id} in a disposable repository fixture.",
        "not": ["A different component"],
        "safe_to_delete": False,
    }
    value.update(overrides)
    return value


class RepositoryOntologyTests(unittest.TestCase):
    def test_longest_prefix_selects_nested_rollback_component(self) -> None:
        payload = {
            "schema_version": 1,
            "components": [
                component("root", "."),
                component("desktop", "apps/desktop"),
                component(
                    "wry", "apps/desktop/ui", parent="desktop", lifecycle="rollback-only",
                    removal_when="replacement passes", review_by="2099-01-01",
                ),
            ],
        }
        found = ontology.resolve_component("apps/desktop/ui/index.html", payload)
        self.assertEqual(found["id"], "wry")

    def test_duplicate_path_is_rejected(self) -> None:
        payload = {
            "schema_version": 1,
            "components": [component("one", "."), component("two", ".")],
        }
        with tempfile.TemporaryDirectory() as directory, mock.patch.object(
            ontology, "git_files", return_value=[]
        ), mock.patch.object(ontology, "workspace_packages", return_value={}), mock.patch.object(
            ontology, "npm_packages", return_value={}
        ):
            with self.assertRaisesRegex(ontology.OntologyError, "duplicate component path"):
                ontology.validate_database(payload, Path(directory))

    def test_generated_output_cannot_point_into_repository(self) -> None:
        payload = {
            "schema_version": 1,
            "components": [component("root", ".", outputs_to=["local/results"])],
        }
        with tempfile.TemporaryDirectory() as directory, mock.patch.object(
            ontology, "git_files", return_value=[]
        ), mock.patch.object(ontology, "workspace_packages", return_value={}), mock.patch.object(
            ontology, "npm_packages", return_value={}
        ):
            with self.assertRaisesRegex(ontology.OntologyError, "must leave Repository"):
                ontology.validate_database(payload, Path(directory))

    def test_unclassified_new_top_level_root_fails_closed(self) -> None:
        payload = {"schema_version": 1, "components": [component("root", ".")]}
        with tempfile.TemporaryDirectory() as directory, mock.patch.object(
            ontology, "git_files", return_value=["surprise/file.txt"]
        ), mock.patch.object(ontology, "workspace_packages", return_value={}), mock.patch.object(
            ontology, "npm_packages", return_value={}
        ):
            with self.assertRaisesRegex(ontology.OntologyError, "unclassified top-level"):
                ontology.validate_database(payload, Path(directory))

    def test_manifest_identity_disagreement_is_rejected(self) -> None:
        payload = {
            "schema_version": 1,
            "components": [component("root", "."), component("crate", "crates/example", package="wrong")],
        }
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "crates" / "example").mkdir(parents=True)
            with mock.patch.object(ontology, "git_files", return_value=[]), mock.patch.object(
                ontology, "workspace_packages", return_value={"crates/example": "actual"}
            ), mock.patch.object(ontology, "npm_packages", return_value={}):
                with self.assertRaisesRegex(ontology.OntologyError, "does not match manifest"):
                    ontology.validate_database(payload, root)

    def test_generated_wiki_is_deterministic(self) -> None:
        payload = {"schema_version": 1, "components": [component("root", ".")]}
        self.assertEqual(ontology.wiki(copy.deepcopy(payload)), ontology.wiki(payload))

    def test_benchmark_fixture_is_valid_json(self) -> None:
        suite = json.loads(ontology.BENCHMARK.read_text(encoding="utf-8"))
        self.assertGreaterEqual(len(suite["questions"]), 10)


if __name__ == "__main__":
    unittest.main(verbosity=2)
