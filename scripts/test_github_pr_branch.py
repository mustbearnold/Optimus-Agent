#!/usr/bin/env python3
"""Unit tests for github_pr_branch pure helpers (no network, no git)."""

from __future__ import annotations

import unittest

import github_pr_branch as gpb


class StripAnsiTest(unittest.TestCase):
    def test_strips_sgr_sequences(self) -> None:
        colored = '\x1b[1;37m{\x1b[m\n  \x1b[1;34m"number"\x1b[m\x1b[1;37m:\x1b[m 21\n}'
        self.assertEqual(gpb._strip_ansi(colored), '{\n  "number": 21\n}')

    def test_noop_on_clean_json(self) -> None:
        raw = '{"number":21,"headRefName":"wip/foo"}'
        self.assertEqual(gpb._strip_ansi(raw), raw)


class slugifyTest(unittest.TestCase):
    def test_emoji_first_conventional_commit_title(self) -> None:
        self.assertEqual(
            gpb.slugify("📝 docs: mandate multi-plane artifact naming for coding agents"),
            "mandate-multi-plane-artifact-naming-for-coding-agents",
        )

    def test_strips_pr_number_prefix(self) -> None:
        self.assertEqual(gpb.slugify("pr/21-artifact-naming-planes"), "artifact-naming-planes")

    def test_strips_wip_prefix(self) -> None:
        self.assertEqual(gpb.slugify("wip/p12-command-fs-envelope"), "p12-command-fs-envelope")

    def test_empty_becomes_work(self) -> None:
        self.assertEqual(gpb.slugify("!!!"), "work")

    def test_length_cap(self) -> None:
        long = "a" * 80
        self.assertEqual(len(gpb.slugify(long)), 60)


class DefaultSlugFromBranchTest(unittest.TestCase):
    def test_pr_local_name(self) -> None:
        self.assertEqual(
            gpb.default_slug_from_branch("pr/21-artifact-naming-planes"),
            "artifact-naming-planes",
        )

    def test_wip_branch(self) -> None:
        self.assertEqual(
            gpb.default_slug_from_branch("wip/p12-command-fs-envelope"),
            "p12-command-fs-envelope",
        )


class LabelValidationTest(unittest.TestCase):
    """Regression coverage: labelling used to be optional, so it depended on
    memory. Nine consecutive PRs (#97–#116) opened unlabelled on 2026-07-28 and
    nothing caught it. `open` now refuses before it pushes."""

    def test_catalog_parses_labels_yml(self) -> None:
        names = gpb.known_labels()
        self.assertIn("✨ type:feat", names)
        self.assertIn("💻 area:cli", names)

    def test_namespace_extraction(self) -> None:
        self.assertEqual(gpb.namespace_of("✨ type:feat"), "type")
        self.assertEqual(gpb.namespace_of("💻 area:cli"), "area")
        self.assertIsNone(gpb.namespace_of("nonsense"))

    def test_no_labels_is_refused(self) -> None:
        with self.assertRaises(SystemExit) as caught:
            gpb.validate_labels([])
        self.assertIn("type", str(caught.exception))
        self.assertIn("area", str(caught.exception))

    def test_partial_labels_are_refused(self) -> None:
        with self.assertRaises(SystemExit) as caught:
            gpb.validate_labels(["✨ type:feat"])
        self.assertIn("area", str(caught.exception))

    def test_unknown_label_is_refused(self) -> None:
        with self.assertRaises(SystemExit) as caught:
            gpb.validate_labels(["🚀 type:teleport"])
        self.assertIn("unknown label", str(caught.exception))

    def test_type_and_area_is_accepted(self) -> None:
        gpb.validate_labels(["✨ type:feat", "💻 area:cli"])

    def test_extra_namespaces_are_fine(self) -> None:
        gpb.validate_labels(["🔧 type:fix", "🎨 area:ui", "▪️ size:S"])

    def test_missing_namespaces_reports_gaps_without_raising(self) -> None:
        # The audit reports on items that already exist, so it describes rather
        # than refuses. validate_labels() is the one that fails closed.
        self.assertEqual(gpb.missing_namespaces([]), ["type", "area"])
        self.assertEqual(gpb.missing_namespaces(["✨ type:feat"]), ["area"])
        self.assertEqual(gpb.missing_namespaces(["✨ type:feat", "💻 area:cli"]), [])


if __name__ == "__main__":
    unittest.main()
