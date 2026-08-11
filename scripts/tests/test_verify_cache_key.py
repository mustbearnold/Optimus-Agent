#!/usr/bin/env python3


"""Pin the verify content-addressed cache key (scripts/verify.sh cache-key).

The key must be a pure function of tree content plus environment
fingerprint: deterministic, sensitive to every tracked and
untracked-not-ignored file, insensitive to ignored files, and sensitive
to the environment inputs that change what a `verify all` run compiles
and which suites it runs.
"""

from __future__ import annotations

import os
import pathlib
import re
import shutil
import subprocess
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
VERIFY = ROOT / "scripts" / "verify.sh"
KEY_RE = re.compile(r"^[0-9a-f]{64}$")


class CacheKeyTest(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.tmp = pathlib.Path(self._tmp.name)
        (self.tmp / "scripts").mkdir()
        shutil.copy(VERIFY, self.tmp / "scripts" / "verify.sh")
        subprocess.run(
            ["git", "init", "-q", "-b", "main", self.tmp], check=True
        )
        self.write("README", "hello\n")
        self.index("README")

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def write(self, rel: str, content: str) -> None:
        path = self.tmp / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")

    def index(self, rel: str) -> None:
        subprocess.run(["git", "-C", str(self.tmp), "add", rel], check=True)

    def key(self, env_extra: dict[str, str] | None = None) -> str:
        env = os.environ.copy()
        if env_extra:
            env.update(env_extra)
        proc = subprocess.run(
            ["/bin/bash", str(self.tmp / "scripts" / "verify.sh"), "cache-key"],
            capture_output=True,
            text=True,
            env=env,
        )
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertTrue(KEY_RE.match(proc.stdout.strip()), proc.stdout)
        return proc.stdout.strip()

    def test_key_is_deterministic(self) -> None:
        self.assertEqual(self.key(), self.key())

    def test_tracked_content_change_invalidates(self) -> None:
        before = self.key()
        self.write("README", "world\n")
        self.assertNotEqual(self.key(), before)

    def test_untracked_not_ignored_file_invalidates(self) -> None:
        before = self.key()
        self.write("scratch.txt", "new\n")
        self.assertNotEqual(self.key(), before)

    def test_ignored_file_does_not_invalidate(self) -> None:
        before = self.key()
        self.write(".gitignore", "ignored.txt\n")
        self.index(".gitignore")
        with_ignore = self.key()
        self.assertNotEqual(with_ignore, before)
        self.write("ignored.txt", "junk\n")
        self.assertEqual(self.key(), with_ignore)

    def test_node_modules_presence_invalidates(self) -> None:
        # The fingerprint probes the ROOT node_modules dir: its presence
        # changes which suites `verify all` runs. An empty ignored dir is
        # invisible to the tree part (git never lists directories), so the
        # key change isolates the environment probe.
        self.write(".gitignore", "node_modules/\n")
        self.index(".gitignore")
        with_ignore = self.key()
        (self.tmp / "node_modules").mkdir()
        with_dir = self.key()
        self.assertNotEqual(with_dir, with_ignore)
        (self.tmp / "node_modules" / "x").write_text("junk\n", encoding="utf-8")
        self.assertEqual(self.key(), with_dir)


if __name__ == "__main__":
    unittest.main()
