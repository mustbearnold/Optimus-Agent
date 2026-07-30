#!/usr/bin/env python3
"""Tests for the fail-closed Optimus/Hermes version gate."""

from __future__ import annotations

import datetime as dt
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import optimus_version as versioning


class VersioningHelpersTest(unittest.TestCase):
    def test_short_and_long_options_never_share_an_id(self) -> None:
        self.assertEqual(versioning.option_id("-q"), "short-q")
        self.assertEqual(versioning.option_id("--q"), "long-q")
        self.assertNotEqual(versioning.option_id("-q"), versioning.option_id("--q"))

    def test_normalized_feature_collisions_are_preserved(self) -> None:
        rows, warnings = versioning.deduplicate_features(
            [
                versioning.feature("cli.option.chat.short-q", "cli-option", "chat -q", "first"),
                versioning.feature("cli.option.chat.short-q", "cli-option", "query -q", "second"),
            ]
        )
        self.assertEqual(len(rows), 2)
        self.assertEqual(warnings, [])
        self.assertTrue(any("variant-" in row["id"] for row in rows))

    def test_release_rejects_numeric_match_without_verified_claim(self) -> None:
        result = versioning.Evaluation(
            product_version="0.19.0",
            target_version="0.19.0",
            claim_status="unverified",
        )
        errors = versioning.release_errors(result)
        self.assertTrue(any("without a verified parity claim" in error for error in errors))

    def test_release_rejects_numeric_match_hidden_by_semver_suffixes(self) -> None:
        for product_version in ("0.19.0+optimus.1", "0.19.0-rc.1"):
            with self.subTest(product_version=product_version):
                result = versioning.Evaluation(
                    product_version=product_version,
                    target_version="0.19.0",
                    claim_status="unverified",
                    blockers=["features incomplete"],
                )
                errors = versioning.release_errors(result)
                self.assertTrue(
                    any("numeric Hermes version match" in error for error in errors),
                    errors,
                )
                self.assertTrue(
                    any("without a verified parity claim" in error for error in errors),
                    errors,
                )

    def test_independent_development_version_is_not_falsely_blocked(self) -> None:
        result = versioning.Evaluation(
            product_version="0.1.0",
            target_version="0.19.0",
            claim_status="unverified",
            blockers=["features incomplete"],
        )
        self.assertEqual(versioning.release_errors(result), [])


class PerformanceGateTest(unittest.TestCase):
    NOW = dt.datetime(2026, 7, 23, 12, 0, tzinfo=versioning.UTC)
    BASELINE_HASH = "a" * 64

    def manifest(self) -> dict:
        return {
            "hermes_target": {"version": "0.19.0", "upstream_revision": "8967e73e"},
            "performance_rules": {
                "minimum_paired_samples_per_scenario": 4,
                "minimum_distinct_seeds_per_scenario": 2,
                "evidence_max_age_days": 30,
                "require_same_machine": True,
                "require_same_model": True,
                "require_same_provider": True,
                "require_same_tool_permissions": True,
                "require_paired_order_randomization": True,
                "maximum_latency_ratio": 1.0,
                "maximum_ttft_ratio": 1.0,
                "maximum_cost_per_success_ratio": 1.0,
                "maximum_memory_ratio": 1.0,
                "minimum_success_rate_delta": 0.0,
                "minimum_quality_score_delta": 0.0,
                "required_scenarios": [
                    {
                        "id": "agent-turn",
                        "metrics": ["wall_ms", "ttft_ms", "cost_usd", "rss_mb"],
                    }
                ],
            },
        }

    def report(self, optimus_wall_ms: list[float]) -> dict:
        runs = []
        for index, wall_ms in enumerate(optimus_wall_ms):
            runs.append(
                {
                    "case_id": f"case-{index}",
                    "seed": index % 2,
                    "execution_order": "hermes-first" if index % 2 == 0 else "optimus-first",
                    "hermes": {
                        "success": True,
                        "quality_score": 1.0,
                        "wall_ms": 100.0,
                        "ttft_ms": 50.0,
                        "cost_usd": 0.01,
                        "rss_mb": 100.0,
                    },
                    "optimus": {
                        "success": True,
                        "quality_score": 1.0,
                        "wall_ms": wall_ms,
                        "ttft_ms": 45.0,
                        "cost_usd": 0.009,
                        "rss_mb": 95.0,
                    },
                }
            )
        return {
            "schema_version": 1,
            "hermes_target_version": "0.19.0",
            "baseline_sha256": self.BASELINE_HASH,
            "generated_at": "2026-07-23T11:00:00Z",
            "optimus_revision": "b" * 40,
            "hermes_revision": "8967e73e",
            "comparison_protocol": {
                "same_machine": True,
                "same_model": True,
                "same_provider": True,
                "same_tool_permissions": True,
                "paired_order_randomized": True,
                "machine_fingerprint": "test-machine",
                "model": "test-model",
                "provider": "test-provider",
                "dataset_sha256": "c" * 64,
                "grader_sha256": "d" * 64,
                "benchmark_harness_sha256": "e" * 64,
                "hermes_binary_sha256": "f" * 64,
                "optimus_binary_sha256": "1" * 64,
            },
            "scenarios": [{"id": "agent-turn", "runs": runs}],
        }

    def evaluate_report(self, report: dict) -> versioning.Evaluation:
        result = versioning.Evaluation()
        versioning.evaluate_performance(
            report,
            self.manifest(),
            self.BASELINE_HASH,
            self.NOW,
            result,
        )
        return result

    def test_equal_or_faster_p50_p95_passes(self) -> None:
        result = self.evaluate_report(self.report([90.0, 95.0, 90.0, 95.0]))
        self.assertEqual(result.errors, [])
        self.assertEqual(result.blockers, [])
        self.assertEqual(result.performance_passed, 1)

    def test_slower_p95_blocks_parity(self) -> None:
        result = self.evaluate_report(self.report([90.0, 90.0, 90.0, 200.0]))
        self.assertEqual(result.errors, [])
        self.assertTrue(any("wall_ms p95 ratio" in blocker for blocker in result.blockers))
        self.assertEqual(result.performance_passed, 0)

    def test_missing_protocol_proof_blocks_parity(self) -> None:
        report = self.report([90.0, 90.0, 90.0, 90.0])
        report["comparison_protocol"]["same_model"] = False
        result = self.evaluate_report(report)
        self.assertTrue(any("same_model" in blocker for blocker in result.blockers))

    def test_missing_benchmark_provenance_blocks_parity(self) -> None:
        report = self.report([90.0, 90.0, 90.0, 90.0])
        report["comparison_protocol"]["grader_sha256"] = None
        result = self.evaluate_report(report)
        self.assertTrue(any("grader_sha256" in blocker for blocker in result.blockers))


class RepositoryContractTest(unittest.TestCase):
    def test_git_queries_scope_a_worktree_override_without_process_env(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp) / "linked-worktree-shape"
            subprocess.run(["git", "init", "--quiet", root], check=True)
            (root / "candidate.txt").write_text("candidate\n", encoding="utf-8")
            subprocess.run(
                ["git", "-C", root, "config", "core.bare", "true"],
                check=True,
            )

            inherited_hook_env = {
                "GIT_DIR": str(Path(temp) / "outer.git"),
                "GIT_WORK_TREE": str(Path(temp) / "outer-worktree"),
                "GIT_INDEX_FILE": str(Path(temp) / "outer-index"),
            }
            with mock.patch.dict(os.environ, inherited_hook_env):
                self.assertEqual(
                    versioning.git_output(root, "status", "--porcelain"),
                    "?? candidate.txt",
                )

    def test_checked_in_version_system_is_well_formed_and_unverified(self) -> None:
        root = Path(__file__).resolve().parents[1]
        result = versioning.evaluate(root, now=dt.datetime.now(versioning.UTC))
        self.assertEqual(result.errors, [])
        self.assertEqual(result.product_version, "0.1.0")
        self.assertEqual(result.target_version, "0.19.0")
        self.assertEqual(result.claim_status, "unverified")
        self.assertEqual(result.feature_total, 2063)
        self.assertEqual(result.feature_verified, 0)
        self.assertFalse(result.ready)


if __name__ == "__main__":
    unittest.main()
