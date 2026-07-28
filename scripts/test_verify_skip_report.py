#!/usr/bin/env python3
"""Unit tests for verify_skip_report rendering (no shell, no gates)."""

from __future__ import annotations

import os
import tempfile
import unittest

import verify_skip_report as vsr


class ParseTest(unittest.TestCase):
    def test_reads_name_and_reason(self) -> None:
        raw = 'electron e2e\tnpm ci in apps/optimus-electron\n'
        self.assertEqual(vsr.parse(raw), [('electron e2e', 'npm ci in apps/optimus-electron')])

    def test_preserves_order(self) -> None:
        raw = 'tui e2e\ttmux not installed\nplaywright\tnpx playwright install chromium\n'
        self.assertEqual([name for name, _ in vsr.parse(raw)], ['tui e2e', 'playwright'])

    def test_collapses_a_gate_named_twice(self) -> None:
        # `electron e2e` reaches `skip` from four separate branches in
        # verify.sh; naming it twice would read as two distinct holes.
        raw = (
            'electron e2e\tnpm ci in apps/optimus-electron\n'
            'electron e2e\tno display and no xvfb-run\n'
        )
        self.assertEqual(len(vsr.parse(raw)), 1)

    def test_ignores_blank_lines(self) -> None:
        self.assertEqual(vsr.parse('\n\n  \n'), [])

    def test_tolerates_a_missing_reason(self) -> None:
        self.assertEqual(vsr.parse('clippy\n'), [('clippy', '')])


class RenderTest(unittest.TestCase):
    def test_nothing_skipped_renders_nothing(self) -> None:
        # The whole point: a fully-run push must not gain a new paragraph.
        self.assertEqual(vsr.render([]), '')

    def test_never_says_clean(self) -> None:
        block = vsr.render([('electron e2e', 'npm ci in apps/optimus-electron')])
        self.assertNotIn('clean', block.lower())

    def test_names_every_gate_and_its_reason(self) -> None:
        block = vsr.render([
            ('electron e2e', 'npm ci in apps/optimus-electron'),
            ('tui e2e', 'tmux not installed'),
        ])
        for text in ('electron e2e', 'npm ci in apps/optimus-electron', 'tui e2e', 'tmux not installed'):
            self.assertIn(text, block)

    def test_counts_agree_with_the_list(self) -> None:
        self.assertIn('1 gate did not run', vsr.render([('tui e2e', 'tmux not installed')]))
        self.assertIn(
            '2 gates did not run',
            vsr.render([('tui e2e', 'tmux'), ('playwright', 'chromium')]),
        )

    def test_points_at_the_flag_ci_uses(self) -> None:
        # Without this the reader knows something is missing but not how to
        # reproduce CI's stricter answer locally.
        self.assertIn('OPTIMUS_VERIFY_FORBID_SKIPS=1', vsr.render([('tui e2e', 'tmux')]))


class MainTest(unittest.TestCase):
    def test_missing_file_is_silence_not_an_error(self) -> None:
        # verify.sh writes the file only when it has skips to report, so absence
        # is the ordinary case and must not fail a push.
        self.assertEqual(vsr.main(['x', '/nonexistent/skip-report']), 0)

    def test_empty_file_is_silence(self) -> None:
        handle, path = tempfile.mkstemp()
        os.close(handle)
        try:
            self.assertEqual(vsr.main(['x', path]), 0)
        finally:
            os.unlink(path)

    def test_wrong_arity_is_a_usage_error(self) -> None:
        self.assertEqual(vsr.main(['x']), 2)


if __name__ == '__main__':
    unittest.main()
