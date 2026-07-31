#!/usr/bin/env python3
"""Regression tests for the executable documentation control plane."""

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import docs_system as docs


def entry(doc_id: str, *, authority: str = "supporting", plane: str = "current") -> dict[str, object]:
    return {
        "id": doc_id,
        "path": f"docs/{doc_id}.md",
        "title": doc_id,
        "type": "reference",
        "plane": plane,
        "status": "current",
        "authority": authority,
        "summary": f"Useful documentation about {doc_id} for Optimus Agent.",
        "reviewed_on": "2026-07-31",
        "review_by": "2026-10-31",
        "headings": [],
        "content_sha256": "00" * 32,
    }


class DocsSystemTests(unittest.TestCase):
    def test_duplicate_headings_receive_github_style_suffixes(self) -> None:
        anchors = docs.heading_anchors("# Title\n## Same\n## Same\n")
        self.assertEqual(anchors, {"title", "same", "same-1"})

    def test_missing_local_heading_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source.md"
            target = root / "target.md"
            source.write_text("# Source\n[bad](target.md#absent)\n", encoding="utf-8")
            target.write_text("# Target\n## Present\n", encoding="utf-8")
            document = docs.Document(
                source, "source.md", {}, "Source", ("Source",), "00" * 32
            )
            old_root = docs.ROOT
            docs.ROOT = root
            try:
                with self.assertRaisesRegex(docs.DocsError, "missing heading anchor"):
                    docs.validate_links([document])
            finally:
                docs.ROOT = old_root

    def test_canonical_document_needs_exclusive_authority_route(self) -> None:
        entries = [entry("primary", authority="canonical"), entry("orphan", authority="canonical")]
        routes = {
            "required_routes": ["topic"],
            "routes": [{"id": "topic", "primary": ["primary"], "supporting": [], "keywords": ["topic"]}],
        }
        with self.assertRaisesRegex(docs.DocsError, "without an authority route: orphan"):
            docs.validate_routes(entries, routes)

    def test_strongest_authority_route_wins_over_generic_overlap(self) -> None:
        payload = {
            "routes": [
                {"id": "product.status", "primary": ["status"], "supporting": [], "keywords": ["current product"]},
                {"id": "product.roadmap", "primary": ["roadmap"], "supporting": [], "keywords": ["current product roadmap next"]},
            ],
            "documents": [
                entry("status", authority="canonical"),
                entry("roadmap", authority="canonical"),
            ],
        }
        found = docs.search(payload, "What is the current product roadmap and what comes next?")
        self.assertEqual(found[0]["id"], "roadmap")

    def test_validation_never_refreshes_stale_source_binding_lock(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "owned.txt"
            source.write_text("new truth", encoding="utf-8")
            lock = root / "verification-lock.json"
            lock.write_text(json.dumps({"schema_version": 1, "documents": {}}), encoding="utf-8")
            document_path = root / "map.md"
            document_path.write_text("# Map\n", encoding="utf-8")
            document = docs.Document(
                document_path,
                "map.md",
                {
                    "doc_id": "map",
                    "status": "current",
                    "knowledge_type": "map",
                    "covers": ["owned.txt"],
                },
                "Map",
                ("Map",),
                "00" * 32,
            )
            old_root, old_lock = docs.ROOT, docs.LOCK
            docs.ROOT, docs.LOCK = root, lock
            try:
                before = lock.read_text(encoding="utf-8")
                with self.assertRaisesRegex(docs.DocsError, "lock is stale"):
                    docs.validate_lock([document])
                self.assertEqual(lock.read_text(encoding="utf-8"), before)
            finally:
                docs.ROOT, docs.LOCK = old_root, old_lock

    def test_all_current_prose_is_held_by_content_hash(self) -> None:
        document = docs.Document(
            Path("guide.md"),
            "docs/guide.md",
            {"doc_id": "guide", "status": "current"},
            "Guide",
            ("Guide",),
            "ab" * 32,
        )
        locked = docs.expected_lock([document])["guide"]
        self.assertEqual(locked["document_sha256"], "ab" * 32)
        self.assertIsNone(locked["binding_sha256"])
        self.assertEqual(locked["resolved_files"], 0)

    def test_binding_globs_exclude_files_outside_git_candidate_universe(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            tracked = root / "apps" / "source.ts"
            ignored = root / "apps" / "dist" / "bundle.js"
            tracked.parent.mkdir(parents=True)
            ignored.parent.mkdir(parents=True)
            tracked.write_text("source", encoding="utf-8")
            ignored.write_text("build output", encoding="utf-8")
            with (
                mock.patch.object(docs, "ROOT", root),
                mock.patch.object(
                    docs,
                    "candidate_repository_files",
                    return_value=("apps/source.ts",),
                ),
            ):
                self.assertEqual(docs.glob_files("apps/**"), [tracked])


if __name__ == "__main__":
    unittest.main()
