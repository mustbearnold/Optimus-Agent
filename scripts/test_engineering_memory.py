#!/usr/bin/env python3
"""Regression tests for Optimus Engineering Memory generation."""

from __future__ import annotations

import importlib.util
import json
import unittest
from pathlib import Path

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
                "optimus-cli",
                "optimus-desktop",
                "optimus-graph",
                "optimus-kernel",
                "optimus-memory",
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

    def test_agent_registry_does_not_invent_specialists(self) -> None:
        registry = EM.build_agent_registry()
        self.assertEqual(registry["agents"], [])
        self.assertEqual(registry["implemented_specialist_agent_count"], 0)
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
        self.assertIn("crates/optimus-kernel/src/replay.rs", by_id["C-13"]["sources"])
        self.assertTrue(by_id["C-18"]["validated_by"])

        workflows = {row["id"]: row for row in EM.build_workflow_registry()["workflows"]}
        self.assertIn("transactionally claim", " ".join(workflows["interval-cron-tick"]["stages"]))
        self.assertIn(
            "SQLite claims/attempts",
            " ".join(workflows["gateway-inbox-drain"]["observability"]),
        )
        self.assertEqual(workflows["kernel-turn"]["cancellation"]["status"], "implemented")

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
        self.assertEqual(coverage["typed_dataset"]["case_count"], 10)
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
