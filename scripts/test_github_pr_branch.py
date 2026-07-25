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


if __name__ == "__main__":
    unittest.main()
