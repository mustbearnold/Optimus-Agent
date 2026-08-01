#!/usr/bin/env python3
"""Self-test for perf_harness.py.

Runs without cargo artifacts: the cold-start smoke uses target/release/optimus
when present, otherwise a stubbed executable fixture, so a cold checkout stays
green while a built tree exercises the real binary.
"""

from __future__ import annotations

import os
import stat
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import perf_harness as ph  # noqa: E402

RULES = {
    "maximum_latency_ratio": 1.0,
    "maximum_memory_ratio": 1.0,
    "maximum_ttft_ratio": 1.0,
}


def measured_fixture(fingerprint: str, wall_p50: float, rss_p50: float = 1000.0) -> dict:
    sample = {"wall_ms": wall_p50, "ttft_ms": None, "peak_rss_kb": rss_p50, "seed": 1}
    return {
        "schema": ph.SCHEMA,
        "machine_fingerprint": fingerprint,
        "scenarios": [
            {
                "id": "cold-start",
                "status": "measured",
                "samples": [sample],
                "aggregates": {
                    "wall_ms": {"p50": wall_p50, "p95": wall_p50},
                    "peak_rss_kb": {"p50": rss_p50, "p95": rss_p50},
                },
            }
        ],
    }


class TestFingerprint(unittest.TestCase):
    def test_stable_across_calls(self):
        self.assertEqual(ph.machine_fingerprint(), ph.machine_fingerprint())

    def test_derived_from_all_parts(self):
        parts = {"hostname": "a", "cpu_model": "b", "cores": 4, "mem_total_kb": 8}
        base = ph.fingerprint_from_parts(parts)
        for key, changed in (("hostname", "x"), ("cpu_model", "y"), ("cores", 8), ("mem_total_kb", 16)):
            self.assertNotEqual(base, ph.fingerprint_from_parts({**parts, key: changed}), key)
        self.assertEqual(len(base), 16)


class TestPercentile(unittest.TestCase):
    def test_single_value(self):
        self.assertEqual(ph.percentile([7.0], 0.95), 7.0)

    def test_interpolated(self):
        self.assertEqual(ph.percentile([1.0, 2.0, 3.0, 4.0], 0.5), 2.5)
        self.assertEqual(ph.percentile([10.0, 20.0], 0.95), 19.5)


class TestCompare(unittest.TestCase):
    def test_regression_enforced_same_machine_fails(self):
        baseline = measured_fixture("same", 100.0)
        current = measured_fixture("same", 150.0)
        lines, code = ph.compare_results(current, baseline, RULES, enforce=True)
        self.assertEqual(code, 1)
        self.assertTrue(any("REGRESSION" in line for line in lines))

    def test_regression_without_enforce_is_informational(self):
        baseline = measured_fixture("same", 100.0)
        current = measured_fixture("same", 150.0)
        lines, code = ph.compare_results(current, baseline, RULES, enforce=False)
        self.assertEqual(code, 0)
        self.assertTrue(any("REGRESSION" in line for line in lines))

    def test_cross_machine_never_enforces(self):
        baseline = measured_fixture("machine-a", 100.0)
        current = measured_fixture("machine-b", 150.0)
        lines, code = ph.compare_results(current, baseline, RULES, enforce=True)
        self.assertEqual(code, 0)
        self.assertTrue(any("cross-machine" in line for line in lines))

    def test_no_regression_passes_enforced(self):
        baseline = measured_fixture("same", 100.0)
        current = measured_fixture("same", 90.0)
        _, code = ph.compare_results(current, baseline, RULES, enforce=True)
        self.assertEqual(code, 0)

    def test_unmeasured_scenarios_stay_informational(self):
        baseline = measured_fixture("same", 100.0)
        current = {
            "schema": ph.SCHEMA,
            "machine_fingerprint": "same",
            "scenarios": [{"id": "cold-start", "status": "skipped"}],
        }
        lines, code = ph.compare_results(current, baseline, RULES, enforce=True)
        self.assertEqual(code, 0)
        self.assertTrue(any("not comparable" in line for line in lines))


class TestSkipHonesty(unittest.TestCase):
    def test_browser_task_requires_live_and_carries_no_samples(self):
        entry = ph.run_one_scenario("browser-task", None, samples=5, seeds=2)
        self.assertEqual(entry["status"], "skipped")
        self.assertEqual(entry["requires"], "live")
        self.assertEqual(entry["samples"], [])
        self.assertIn("SSRF", entry["skip_reason"])
        self.assertNotIn("aggregates", entry)


class TestSchemaValidation(unittest.TestCase):
    def synthetic_result(self) -> dict:
        skipped = ph.run_one_scenario("browser-task", None, samples=1, seeds=1)
        measured = {
            "id": "cold-start",
            "status": "measured",
            "requires": "offline",
            "ttft_reason": ph.TTFT_REASON,
            "rss_reason": None,
            "samples": [{"wall_ms": 5.0, "ttft_ms": None, "peak_rss_kb": 100, "seed": 1}],
            "aggregates": ph.aggregate(
                [{"wall_ms": 5.0, "ttft_ms": None, "peak_rss_kb": 100, "seed": 1}]
            ),
        }
        return ph.build_result([measured, skipped], samples=1, seeds=1, binary_info={"path": None})

    def test_valid_result_has_no_problems(self):
        self.assertEqual(ph.validate_result(self.synthetic_result()), [])

    def test_measured_without_samples_rejected(self):
        result = self.synthetic_result()
        result["scenarios"][0]["samples"] = []
        self.assertTrue(any("no samples" in p for p in ph.validate_result(result)))

    def test_skipped_with_samples_rejected(self):
        result = self.synthetic_result()
        result["scenarios"][1]["samples"] = [{"wall_ms": 1.0, "seed": 1}]
        self.assertTrue(any("no samples" in p for p in ph.validate_result(result)))

    def test_null_ttft_needs_stated_reason(self):
        result = self.synthetic_result()
        result["scenarios"][0]["ttft_reason"] = None
        self.assertTrue(any("ttft_reason" in p for p in ph.validate_result(result)))

    def test_quick_mode_is_labelled_informational(self):
        note = self.synthetic_result()["protocol"]["note"]
        self.assertIn("below the protocol minimum", note)
        self.assertIn("ADR-0069", note)


class TestColdStartSmoke(unittest.TestCase):
    """End-to-end sample collection through a real process spawn."""

    def setUp(self):
        release = ph.ROOT / "target" / "release" / "optimus"
        if release.is_file() and os.access(release, os.X_OK):
            self.binary = str(release)
            self.using_stub = False
            return
        self.tmp = tempfile.TemporaryDirectory(prefix="perf-stub-")
        stub = Path(self.tmp.name) / "optimus-stub"
        stub.write_text('#!/bin/sh\necho "Optimus Agent 0.0.0-stub"\n', encoding="utf-8")
        stub.chmod(stub.stat().st_mode | stat.S_IXUSR)
        self.binary = str(stub)
        self.using_stub = True

    def tearDown(self):
        if self.using_stub:
            self.tmp.cleanup()

    def test_cold_start_end_to_end(self):
        entry = ph.run_one_scenario("cold-start", self.binary, samples=2, seeds=2)
        self.assertEqual(entry["status"], "measured", entry.get("error"))
        self.assertEqual(len(entry["samples"]), 2)
        for row in entry["samples"]:
            self.assertGreater(row["wall_ms"], 0.0)
            self.assertGreater(row["peak_rss_kb"], 0)
            self.assertIsNone(row["ttft_ms"])
            self.assertIn(row["seed"], ph.seed_list(2))
        self.assertIn("p50", entry["aggregates"]["wall_ms"])
        result = ph.build_result([entry], samples=2, seeds=2, binary_info={"path": self.binary})
        self.assertEqual(ph.validate_result(result), [])
        self.assertEqual(len(result["machine_fingerprint"]), 16)


class TestBinaryResolution(unittest.TestCase):
    def test_missing_binary_reports_build_hint(self):
        binary, info = ph.resolve_binary("/nonexistent/optimus")
        self.assertIsNone(binary)
        self.assertIn("cargo build", info["note"])


if __name__ == "__main__":
    unittest.main(verbosity=1)
