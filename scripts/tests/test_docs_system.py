#!/usr/bin/env python3


"""Regression tests for the executable documentation control plane."""


from __future__ import annotations


import pathlib, sys
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / "tools"))
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

    def test_a_superseded_document_can_be_named_out_of_the_lock(self) -> None:
        # ADR-0063 §5 locks every current or planned document, so one that goes
        # historical has to be able to leave. Before this was fixed `refresh`
        # only ever wrote entries, so `validate_lock` reported the retired id as
        # `extra` forever and the gate could never go green again. Retiring is
        # still an explicit review act, and a document that is merely stale — a
        # current one — cannot be pruned to dodge its binding.
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            lock = root / "verification-lock.json"
            lock.write_text(
                json.dumps(
                    {
                        "schema_version": 1,
                        "documents": {
                            "retired": {"path": "retired.md"},
                            "kept": {"path": "kept.md"},
                        },
                    }
                ),
                encoding="utf-8",
            )
            historical = docs.Document(
                root / "retired.md", "retired.md",
                {"doc_id": "retired", "status": "historical"},
                "Retired", ("Retired",), "11" * 32,
            )
            current = docs.Document(
                root / "kept.md", "kept.md",
                {"doc_id": "kept", "status": "current"},
                "Kept", ("Kept",), "22" * 32,
            )
            old_root, old_lock = docs.ROOT, docs.LOCK
            docs.ROOT, docs.LOCK = root, lock
            try:
                docs.refresh([historical, current], ["retired", "kept"])
                held = json.loads(lock.read_text(encoding="utf-8"))["documents"]
                self.assertEqual(sorted(held), ["kept"])
                # The retired id is gone, so naming it again is a typo, not a
                # second retirement.
                with self.assertRaisesRegex(docs.DocsError, "unknown or non-source-bound"):
                    docs.refresh([historical, current], ["retired"])
                docs.validate_lock([historical, current])
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

    def test_generated_artifacts_are_excluded_from_binding_digests(self) -> None:
        # repository_ontology.py writes docs/repository-components.json and
        # its human view (specs/011-developer-tooling/repository-components.md)
        # on EVERY docs-generate. Before they joined the binding exclusions,
        # any document whose globs covered them re-staled the lock on every
        # regeneration (refresh → generate → stale again), and docs-check
        # failed until someone refreshed the affected ids a second time.
        json_db = (docs.DOCS / "repository-components.json").resolve()
        human_view = (docs.SPECS / "011-developer-tooling" / "repository-components.md").resolve()
        self.assertIn(json_db, docs.GENERATED)
        self.assertIn(json_db, docs.BINDING_EXCLUDED)
        # The human view IS a cataloged document (frontmatter + routes), so
        # it must stay in the document set — only bindings exclude it.
        self.assertNotIn(human_view, docs.GENERATED)
        self.assertIn(human_view, docs.BINDING_EXCLUDED)
        for generated in (docs.CATALOG_JSON, docs.CATALOG_MD, docs.LOCK, docs.COMPONENTS_MD):
            self.assertIn(generated.resolve(), docs.BINDING_EXCLUDED)

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

    def test_a_deleted_but_still_indexed_file_is_not_a_binding(self) -> None:
        # `git ls-files --cached` reports index entries, and managed delivery
        # never stages, so a file deleted in the worktree stays a candidate all
        # the way through the land gate. Before this was fixed, deleting any
        # glob-bound source file made `docs-check` — and therefore `just land`,
        # which runs verify.sh in the worktree — die on FileNotFoundError
        # instead of reporting the document whose bindings had changed.
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            survivor = root / "crates" / "kept" / "Cargo.toml"
            survivor.parent.mkdir(parents=True)
            survivor.write_text("[package]\n", encoding="utf-8")
            document = docs.Document(
                Path("status.md"),
                "docs/current/status.md",
                {
                    "doc_id": "current-status",
                    "status": "current",
                    "knowledge_type": "current-state",
                    "covers": ["crates/**"],
                },
                "Status",
                ("Status",),
                "cd" * 32,
            )
            with (
                mock.patch.object(docs, "ROOT", root),
                mock.patch.object(
                    docs,
                    "candidate_repository_files",
                    return_value=("crates/gone/Cargo.toml", "crates/kept/Cargo.toml"),
                ),
            ):
                self.assertEqual(docs.glob_files("crates/**"), [survivor])
                digest, resolved = docs.binding_digest(document)
        self.assertEqual(resolved, 1)
        self.assertEqual(len(digest), 64)

    def test_dead_frontmatter_binding_is_rejected(self) -> None:
        # Regression (2026-08-05): the SDD layout retired ~110 docs while 27
        # ADRs still named their old paths in owns/covers/depends_on/
        # validated_by. Historical records never enter change-impact, so the
        # dead bindings passed every gate silently. validate_bindings makes
        # the ADR-0062 precedent machine-enforced.
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            survivor = root / "specs" / "002-host-ipc" / "spec.md"
            survivor.parent.mkdir(parents=True)
            survivor.write_text("---\n---\n", encoding="utf-8")
            alive = docs.Document(
                Path("alive.md"),
                "docs/decisions/0001-alive.md",
                {
                    "doc_id": "decisions-0001-alive",
                    "status": "historical",
                    "depends_on": ["specs/002-host-ipc/spec.md"],
                },
                "Alive",
                ("Alive",),
                "ab" * 32,
            )
            dead_doc = docs.Document(
                Path("dead.md"),
                "docs/decisions/0002-dead.md",
                {
                    "doc_id": "decisions-0002-dead",
                    "status": "historical",
                    "depends_on": ["docs/plans/retired-program.md"],
                },
                "Dead",
                ("Dead",),
                "cd" * 32,
            )
            with (
                mock.patch.object(docs, "ROOT", root),
                mock.patch.object(
                    docs,
                    "candidate_repository_files",
                    return_value=("specs/002-host-ipc/spec.md",),
                ),
            ):
                docs.validate_bindings([alive])
                with self.assertRaises(docs.DocsError) as caught:
                    docs.validate_bindings([alive, dead_doc])
                message = str(caught.exception)
                self.assertIn("docs/decisions/0002-dead.md", message)
                self.assertIn("docs/plans/retired-program.md", message)


if __name__ == "__main__":
    unittest.main()
