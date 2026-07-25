#!/usr/bin/env python3
"""Regression tests for Optimus Engineering Memory generation."""

from __future__ import annotations

import contextlib
import importlib.util
import io
import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "engineering_memory", ROOT / "scripts/engineering_memory.py"
)
assert SPEC and SPEC.loader
EM = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(EM)


class EngineeringMemoryTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.metadata = EM.cargo_metadata()

    def test_workspace_packages_are_exact(self) -> None:
        names = {row["name"] for row in EM.package_records(self.metadata)}
        self.assertEqual(
            names,
            {
                "optimus-browser",
                "optimus-cli",
                "optimus-desktop",
                "optimus-eval",
                "optimus-graph",
                "optimus-kernel",
                "optimus-memory",
                "optimus-ops",
                "optimus-packs",
                "optimus-runtime",
                "optimus-skills",
                "optimus-store",
            },
        )

    def test_canonical_tool_catalog_is_reconciled(self) -> None:
        registry = EM.parse_tool_catalog()
        tools = registry["tools"]
        self.assertEqual(len(tools), 22)
        self.assertEqual(len({row["id"] for row in tools}), 22)
        available = {row["id"] for row in tools if row["available"]}
        self.assertEqual(
            available,
            {
                "activate_pack",
                "browser_click",
                "browser_navigate",
                "browser_snapshot",
                "memory_recall",
                "read_file",
                "skill_resolve",
                "terminal",
                "web_search",
                "write_file",
            },
        )
        terminal = next(row for row in tools if row["id"] == "terminal")
        self.assertEqual(terminal["approval"]["status"], "required")
        self.assertEqual(terminal["policy"], "process")
        self.assertEqual(terminal["retry"]["status"], "never")
        self.assertEqual(terminal["cancellation_status"], "terminal_owner")
        self.assertTrue(terminal["observability_contract"]["trace_span_required"])
        read_file = next(row for row in tools if row["id"] == "read_file")
        self.assertEqual(read_file["idempotency"]["status"], "keyed")

        compressed = EM.compress_tool_registry(registry)
        self.assertEqual(compressed.get("storage"), "templated_v2")
        self.assertTrue(compressed.get("templates"))
        expanded = EM.expand_tool_registry(compressed)
        self.assertEqual(
            {row["id"]: row["approval"] for row in expanded["tools"]},
            {row["id"]: row["approval"] for row in tools},
        )

    def test_agent_registry_records_builtin_workspace_writer(self) -> None:
        registry = EM.build_agent_registry()
        self.assertEqual(registry["implemented_specialist_agent_count"], 2)
        self.assertEqual(len(registry["agents"]), 2)
        ids = {row["id"] for row in registry["agents"]}
        self.assertEqual(ids, {"workspace_writer", "workspace_reader"})
        self.assertEqual(
            registry["status"], "implemented_multi_agent_dag_verticals"
        )
        self.assertEqual(registry["contract_substrate"]["status"], "implemented")
        self.assertEqual(
            registry["contract_substrate"]["source"],
            "crates/optimus-kernel/src/agent.rs",
        )

    def test_implemented_workflows_have_terminal_declarations(self) -> None:
        workflows = EM.build_workflow_registry()["workflows"]
        self.assertEqual(
            {row["id"] for row in workflows},
            {
                "kernel-turn",
                "work-graph-job",
                "durable-campaign",
                "write-file-handoff",
                "read-file-handoff",
                "write-then-read-handoff",
                "interval-cron-tick",
                "gateway-inbox-drain",
                "general-workflow-contract",
            },
        )
        required = {
            "id",
            "version",
            "status",
            "owner",
            "trigger",
            "inputs",
            "outputs",
            "stages",
            "dependencies",
            "completion",
            "failure",
            "cancellation",
            "retry",
            "rollback",
            "observability",
            "validated_by",
            "source",
        }
        for workflow in workflows:
            self.assertFalse(required - workflow.keys(), workflow["id"])
            self.assertTrue(workflow["completion"])
            self.assertTrue(workflow["failure"])

        campaign = next(row for row in workflows if row["id"] == "durable-campaign")
        self.assertEqual(campaign["dependencies"], ["work-graph-job", "optimus.db"])
        self.assertIn("schema v4", campaign["validation"])
        self.assertIn("fenced owner leases", campaign["validation"])
        self.assertIn("job-derived", campaign["state_transitions"])
        self.assertFalse(
            any("stores can diverge" in failure for failure in campaign["failure"])
        )

    def test_high_risk_contract_register_covers_current_audit_gaps(self) -> None:
        contracts = EM.build_contract_coverage()["contracts"]
        self.assertEqual(
            {row["id"] for row in contracts},
            {f"C-{index:02d}" for index in range(1, 19)},
        )
        by_id = {row["id"]: row for row in contracts}
        self.assertEqual(by_id["C-04"]["implementation_status"], "implemented")
        self.assertIn(
            "crates/optimus-runtime/tests/path_confinement.rs",
            by_id["C-04"]["validated_by"],
        )
        self.assertEqual(by_id["C-06"]["implementation_status"], "implemented")
        self.assertTrue(by_id["C-06"]["validated_by"])
        for contract_id in ("C-01", "C-02", "C-03", "C-05", "C-15", "C-16", "C-17"):
            self.assertEqual(by_id[contract_id]["implementation_status"], "implemented")
            self.assertTrue(by_id[contract_id]["validated_by"])
        for contract_id in ("C-08", "C-09", "C-10", "C-11", "C-12", "C-13", "C-14", "C-18"):
            self.assertEqual(by_id[contract_id]["implementation_status"], "implemented")
            self.assertTrue(by_id[contract_id]["validated_by"])
        self.assertIn("crates/optimus-eval/src/replay.rs", by_id["C-13"]["sources"])
        self.assertIn("crates/optimus-kernel/src/lib.rs", by_id["C-13"]["sources"])
        self.assertIn(
            "crates/optimus-kernel/tests/kernel_turn.rs",
            by_id["C-13"]["validated_by"],
        )
        self.assertIn(
            "crates/optimus-kernel/tests/session_resume.rs",
            by_id["C-13"]["validated_by"],
        )
        self.assertTrue(by_id["C-18"]["validated_by"])

        workflows = {row["id"]: row for row in EM.build_workflow_registry()["workflows"]}
        self.assertIn("transactionally claim", " ".join(workflows["interval-cron-tick"]["stages"]))
        self.assertIn(
            "SQLite claims/attempts",
            " ".join(workflows["gateway-inbox-drain"]["observability"]),
        )
        self.assertEqual(workflows["kernel-turn"]["cancellation"]["status"], "implemented")
        self.assertIn(
            "stream delivery loss",
            workflows["kernel-turn"]["cancellation"]["contract"],
        )
        self.assertIn(
            "explicit capability-local Stop",
            workflows["kernel-turn"]["cancellation"]["contract"],
        )

    def test_integrity_evaluation_catalog_is_exact_and_executed(self) -> None:
        coverage = EM.build_evaluation_coverage()
        self.assertEqual(
            [case["id"] for case in coverage["integrity_cases"]],
            [
                "sensitivity_denial",
                "smartdeny_approval",
                "route_policy_denial",
                "cooperative_cancellation",
                "stale_completion_fence",
                "gateway_dead_letter",
            ],
        )
        self.assertTrue(all(case["executed_by"] for case in coverage["integrity_cases"]))
        self.assertEqual(
            coverage["integrity_executor"],
            {
                "source": "crates/optimus-eval/src/eval.rs",
                "validated_by": "crates/optimus-eval/tests/integrity_integration.rs",
                "case_count": 6,
                "isolated_runs": True,
                "trace_store": "per_run_integrity-traces.db",
                "typed_evidence": ["terminal_status", "replay", "trace_context"],
                "retry_identity": "fresh_trace_ids_stable_normalized_semantics",
            },
        )
        self.assertEqual(
            coverage["trajectory_executor"],
            {
                "source": "crates/optimus-eval/src/eval.rs",
                "validated_by": "crates/optimus-eval/tests/evaluation_contracts.rs",
                "case_count": 4,
                "typed_evidence": [
                    "assistant_text",
                    "invoked_tools",
                    "terminal_status",
                    "replay",
                    "trace_context",
                ],
            },
        )
        self.assertEqual(
            coverage["priority2_report_executor"],
            {
                "source": "crates/optimus-eval/src/evaluation.rs",
                "validated_by": "crates/optimus-eval/tests/evaluation_contracts.rs",
                "case_count": 10,
                "resource_measurements": "explicit_caller_supplied_per_case",
                "retry_identity": "fresh_run_and_trace_ids_stable_report_bytes",
                "preflight_before_mutation": True,
                "cli": "optimus eval report",
                "cli_validated_by": "apps/optimus-cli/tests/eval_report.rs",
                "json_input_limit_bytes": 1048576,
                "binding_generator": "python scripts/engineering_memory.py binding",
                "binding_context": "compiled_offline_sources_enforced_before_mutation",
            },
        )
        self.assertEqual(coverage["typed_dataset"]["case_count"], 10)
        self.assertEqual(
            coverage["comparison_cli"],
            {
                "command": "optimus eval compare",
                "source": "apps/optimus-cli/src/main.rs",
                "validated_by": "apps/optimus-cli/tests/eval_compare.rs",
                "json_input_limit_bytes": 1048576,
                "mutation": "none_including_home",
                "valid_regressions_exit": "success_with_complete_comparison",
            },
        )
        self.assertTrue(coverage["baseline_comparison"])
        self.assertTrue(coverage["version_binding"])
        self.assertEqual(
            coverage["candidate_bindings"],
            [
                "source_tree_sha256",
                "contract_sha256",
                "tool_catalog_sha256",
                "route_policy_sha256",
                "provider",
                "model",
            ],
        )
        self.assertIn("replay_accuracy", coverage["metrics"])
        self.assertEqual(
            coverage["dimensions"]["trace"],
            "required_case_trace_presence_enforced_before_metrics",
        )

    def test_priority2_candidate_binding_uses_current_canonical_sources(self) -> None:
        binding = EM.build_priority2_candidate_binding()
        repository = EM.build_repository_index(EM.cargo_metadata())
        self.assertEqual(binding["source_tree_sha256"], repository["tree_sha256"])
        self.assertEqual(
            binding["contract_sha256"],
            EM.sha256_file(ROOT / "crates/optimus-eval/src/evaluation.rs"),
        )
        self.assertEqual(
            binding["tool_catalog_sha256"],
            EM.sha256_file(ROOT / "crates/optimus-packs/src/lib.rs"),
        )
        self.assertEqual(
            binding["route_policy_sha256"],
            EM.sha256_file(ROOT / "crates/optimus-kernel/src/routing.rs"),
        )
        self.assertEqual(binding["provider"], "offline")
        self.assertEqual(binding["model"], "offline-scripted")

    def test_priority2_binding_command_failure_keeps_stdout_empty(self) -> None:
        stdout = io.StringIO()
        stderr = io.StringIO()
        with (
            mock.patch.object(
                EM,
                "build_priority2_candidate_binding",
                side_effect=EM.MemoryError("binding unavailable"),
            ),
            contextlib.redirect_stdout(stdout),
            contextlib.redirect_stderr(stderr),
        ):
            code = EM.main(["binding"])
        self.assertEqual(code, 1)
        self.assertEqual(stdout.getvalue(), "")
        self.assertIn("binding unavailable", stderr.getvalue())

    def test_current_docs_do_not_resurrect_adr_0019_superseded_debt(self) -> None:
        current_docs = [
            ROOT / "docs/architecture/system-overview.md",
            ROOT / "docs/maps/security-and-approvals.md",
        ]
        stale_claims = [
            "runtime path confinement is not handle-relative",
            "runtime path checks are not handle-relative",
            "write secret policy is not unified",
            "runtime writes also do not apply",
            "campaign/job handoffs are not atomic",
        ]
        for path in current_docs:
            text = path.read_text(encoding="utf-8").lower()
            for claim in stale_claims:
                self.assertNotIn(claim, text, f"{path}: {claim}")

    def test_current_authority_does_not_resurrect_fixed_repository_debt(self) -> None:
        current_docs = [
            ROOT / "docs/engineering-memory/README.md",
            ROOT / "docs/architecture/system-overview.md",
        ]
        obsolete_claims = [
            "this repository has no `.git` directory",
            "unauthenticated wildcard-cors loopback desktop test api",
            "campaign ownership has no lease",
            "cron and gateway have no claim/lease",
            "event-stream receiver failure stops delivery but does not cancel",
        ]
        combined = "\n".join(path.read_text(encoding="utf-8").lower() for path in current_docs)
        for claim in obsolete_claims:
            self.assertNotIn(claim, combined, claim)

    def test_validation_rejects_resurrected_split_campaign_store(self) -> None:
        workflows = EM.build_workflow_registry()["workflows"]
        contracts = EM.build_contract_coverage()["contracts"]
        self.assertEqual(EM.current_architecture_semantic_errors(workflows, contracts), [])

        altered = [dict(workflow) for workflow in workflows]
        campaign = next(row for row in altered if row["id"] == "durable-campaign")
        campaign["dependencies"] = ["work-graph-job", "campaigns.db"]
        errors = EM.current_architecture_semantic_errors(altered, contracts)
        self.assertIn(
            "durable-campaign must depend on work-graph-job and optimus.db",
            errors,
        )

    def test_important_documents_have_resolving_frontmatter(self) -> None:
        for rel in EM.IMPORTANT_DOCS:
            path = ROOT / rel
            self.assertTrue(path.exists(), rel)
            frontmatter = EM.parse_frontmatter(path)
            self.assertIsNotNone(frontmatter, rel)
            assert frontmatter is not None
            self.assertTrue(EM.expand_patterns(frontmatter["covers"]), rel)
            self.assertIn(frontmatter["status"], {"current", "planned", "historical", "stale"})

    def test_generation_is_deterministic_in_memory(self) -> None:
        first = EM.build_maps(refresh_staleness=True)
        second = EM.build_maps(refresh_staleness=True)
        self.assertEqual(
            {name: EM.canonical_json(value) for name, value in first.items()},
            {name: EM.canonical_json(value) for name, value in second.items()},
        )

    def test_generated_identity_is_independent_of_ambient_git_state(self) -> None:
        repository = EM.build_repository_index(EM.cargo_metadata())
        self.assertNotIn("git", repository)
        self.assertEqual(repository["verification_basis"], "sha256_tree")
        indexed_roots = sorted({row["path"].split("/", 1)[0] for row in repository["files"]})
        self.assertEqual(repository["root_entries"], indexed_roots)
        for domain, present in repository["top_level_domains"].items():
            self.assertEqual(
                present,
                any(
                    row["path"] == domain or row["path"].startswith(f"{domain}/")
                    for row in repository["files"]
                ),
            )

        staleness = EM.build_knowledge_staleness(refresh=True)
        self.assertTrue(staleness["documents"])
        self.assertEqual(staleness.get("storage"), "hash_only_v2")
        self.assertTrue(
            all(
                document["verification_basis"] == "sha256_tree"
                and "covered_files" not in document
                and "covered_file_count" in document
                for document in staleness["documents"]
            )
        )

    def test_change_impact_is_pattern_indexed_not_path_expanded(self) -> None:
        impact = EM.build_change_impact()
        self.assertIn("pattern_to_knowledge", impact)
        self.assertNotIn("source_to_knowledge", impact)
        self.assertTrue(impact["pattern_to_knowledge"])
        sample = next(iter(impact["documents"]))
        self.assertIn("owns", sample)
        self.assertIn("resolved_test_count", sample)
        self.assertNotIn("resolved_tests", sample)

        hits = EM.impact_for_paths(["crates/optimus-packs/src/lib.rs"], impact)
        self.assertIn("crates/optimus-packs/src/lib.rs", hits)
        self.assertTrue(hits["crates/optimus-packs/src/lib.rs"])

    def test_system_overview_no_longer_owns_entire_kernel_tree(self) -> None:
        overview = EM.parse_frontmatter(ROOT / "docs/architecture/system-overview.md")
        assert overview is not None
        owns = EM.ownership_patterns(overview)
        self.assertIn("crates/optimus-kernel/src/lib.rs", owns)
        self.assertFalse(any(item.endswith("/**") and "optimus-kernel" in item for item in owns))
        watches = EM.watch_patterns(overview)
        self.assertTrue(any("crates/*/src/**" == item or item.endswith("src/**") for item in watches))

        impact = EM.build_change_impact()
        leaf = "crates/optimus-kernel/src/openai_compat.rs"
        hits = EM.impact_for_paths([leaf], impact).get(leaf, [])
        overview_relations = {
            row["relation"]
            for row in hits
            if row["document"] == "docs/architecture/system-overview.md"
        }
        self.assertIn("watches", overview_relations)
        self.assertNotIn("owns", overview_relations)

    def test_context_lens_stays_within_budget(self) -> None:
        pack = EM.build_context_pack(
            budget_tokens=3000,
            paths=["crates/optimus-kernel/src/execution.rs"],
        )
        self.assertTrue(pack["ok"])
        self.assertLessEqual(pack["used_tokens"], 3000)
        self.assertIn("EM_CONTEXT v2", pack["text"])
        self.assertIn("crates/optimus-kernel/src/execution.rs", pack["text"])

    def test_canonical_json_is_compact(self) -> None:
        payload = {"b": 1, "a": [2, 3]}
        rendered = EM.canonical_json(payload)
        self.assertEqual(rendered, '{"a":[2,3],"b":1}\n')

    def test_file_record_hash_cache_round_trip(self) -> None:
        with tempfile.TemporaryDirectory(
            prefix="engineering-memory-cache-", dir=ROOT
        ) as directory:
            path = Path(directory) / "cached.txt"
            path.write_text("alpha\n", encoding="utf-8")
            first = EM.file_record(path)
            second = EM.file_record(path)
            self.assertEqual(first, second)
            path.write_text("beta\n", encoding="utf-8")
            third = EM.file_record(path)
            self.assertNotEqual(first["sha256"], third["sha256"])

    def test_repository_files_excludes_linked_worktree_git_pointer(self) -> None:
        fake_root = Path("C:/repo")
        with (
            mock.patch.object(EM, "ROOT", fake_root),
            mock.patch.object(
                EM.os,
                "walk",
                return_value=[(str(fake_root), [], [".git", "Cargo.toml"])],
            ),
        ):
            EM.repository_files.cache_clear()
            try:
                self.assertEqual(
                    [path.name for path in EM.repository_files()],
                    ["Cargo.toml"],
                )
            finally:
                EM.repository_files.cache_clear()

    def test_repository_files_excludes_tsbuildinfo_artifacts(self) -> None:
        fake_root = Path("/tmp/repo")
        with (
            mock.patch.object(EM, "ROOT", fake_root),
            mock.patch.object(
                EM.os,
                "walk",
                return_value=[
                    (
                        str(fake_root),
                        [],
                        ["Cargo.toml", "tsconfig.tsbuildinfo", "foo.pyc"],
                    )
                ],
            ),
        ):
            EM.repository_files.cache_clear()
            try:
                self.assertEqual(
                    [path.name for path in EM.repository_files()],
                    ["Cargo.toml"],
                )
            finally:
                EM.repository_files.cache_clear()

    def test_file_records_canonicalize_text_eol_and_preserve_binary_bytes(self) -> None:
        with tempfile.TemporaryDirectory(
            prefix="engineering-memory-test-", dir=ROOT
        ) as directory:
            text_path = Path(directory) / "fixture.html"
            text_path.write_bytes(b"a\r\nb\r\n")
            text_record = EM.file_record(text_path)
            self.assertEqual(text_record["sha256"], EM.sha256_bytes(b"a\nb\n"))
            self.assertEqual(text_record["bytes"], 4)
            self.assertEqual(text_record["lines"], 2)

            binary_path = Path(directory) / "fixture.bin"
            binary_path.write_bytes(b"\xff\r\n")
            binary_record = EM.file_record(binary_path)
            self.assertEqual(binary_record["sha256"], EM.sha256_bytes(b"\xff\r\n"))
            self.assertEqual(binary_record["bytes"], 3)
            self.assertIsNone(binary_record["lines"])

    def test_checked_in_generated_files_have_markers(self) -> None:
        for name in EM.GENERATED_NAMES:
            path = EM.MEMORY_DIR / name
            self.assertTrue(path.exists(), name)
            value = json.loads(path.read_text(encoding="utf-8"))
            self.assertEqual(value["generated_by"], EM.GENERATOR)
            self.assertIs(value["do_not_edit"], True)

    def test_update_skill_has_valid_minimal_frontmatter(self) -> None:
        path = ROOT / "skills/update-engineering-memory/SKILL.md"
        frontmatter = EM.parse_frontmatter(path)
        self.assertIsNotNone(frontmatter)
        assert frontmatter is not None
        self.assertEqual(frontmatter.get("name"), "update-engineering-memory")
        self.assertTrue(str(frontmatter.get("description", "")).startswith("Use after"))


if __name__ == "__main__":
    unittest.main(verbosity=2)
