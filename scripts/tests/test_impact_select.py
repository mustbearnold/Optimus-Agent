#!/usr/bin/env python3
"""Self-tests for the impact selector (program P42).

The selector's only job is to be a *superset* of what would fail. These tests
are written to catch it being a subset, so most of them assert that something
IS selected rather than that something is not. The two that assert exclusion —
that a leaf crate does not drag the workspace in, and that an image is inert —
exist because a selector that always escalates is just `just test` with extra
steps.
"""

from __future__ import annotations

import contextlib
import importlib.util
import io
import json
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
_spec = importlib.util.spec_from_file_location(
    "impact_select", ROOT / "scripts" / "tools" / "impact_select.py"
)
IS = importlib.util.module_from_spec(_spec)
assert _spec.loader is not None
_spec.loader.exec_module(IS)

MEMBERS = IS.workspace_members()
REVERSE = IS.reverse_dependents(MEMBERS)


def plan_for(*paths: str):
    return IS.build_plan(list(paths), members=MEMBERS, reverse=REVERSE)


class EscalationTests(unittest.TestCase):
    """Rule 1 and rule 2: the directions in which the selector must give up."""

    def test_a_path_no_rule_recognises_selects_everything(self) -> None:
        plan = plan_for("some/new/subsystem/thing.rs")
        self.assertTrue(plan.escalated)
        self.assertIn("some/new/subsystem/thing.rs", plan.unclassified)
        self.assertEqual(IS.cargo_arguments(plan), ["--workspace"])

    def test_the_selector_cannot_shrink_itself(self) -> None:
        plan = plan_for("scripts/tools/impact_select.py")
        self.assertTrue(plan.escalated)

    def test_changing_the_gate_selects_everything(self) -> None:
        for path in (
            "justfile",
            "scripts/verify.sh",
            "scripts/gates/check-module-size.py",
            ".github/workflows/ci.yml",
            "Cargo.lock",
            "Cargo.toml",
        ):
            with self.subTest(path=path):
                self.assertTrue(plan_for(path).escalated, path)

    def test_one_unclassified_path_escalates_the_whole_plan(self) -> None:
        # A patch that touches a known crate AND something unrecognised is not
        # half-safe. Mixing must not dilute the escalation.
        plan = plan_for("crates/optimus-policy/src/lib.rs", "mystery.bin")
        self.assertTrue(plan.escalated)

    def test_an_escalated_plan_is_never_reported_as_empty(self) -> None:
        plan = plan_for("justfile")
        self.assertFalse(plan.is_empty())
        self.assertEqual(plan.status(), "escalated")


class NothingIsNotPassingTests(unittest.TestCase):
    """Rule 4."""

    def test_an_empty_patch_selects_nothing_and_says_so(self) -> None:
        plan = plan_for()
        self.assertEqual(plan.status(), "nothing-selected")
        self.assertIn("this is not a pass", IS.render(plan))

    def _exit_code(self, argv: list[str]) -> int:
        # `main` prints the plan; the gate's output belongs to the gate.
        with contextlib.redirect_stdout(io.StringIO()):
            return IS.main(argv)

    def test_require_selection_fails_on_an_empty_plan(self) -> None:
        self.assertEqual(self._exit_code(["--paths", "--require-selection"]), 1)

    def test_require_selection_passes_when_something_is_selected(self) -> None:
        code = self._exit_code(
            ["--paths", "crates/optimus-policy/src/lib.rs", "--require-selection"]
        )
        self.assertEqual(code, 0)

    def test_an_inert_file_does_not_manufacture_a_selection(self) -> None:
        plan = plan_for("docs/images/diagram.png")
        self.assertEqual(plan.status(), "nothing-selected")


class TransitiveImpactTests(unittest.TestCase):
    """Rule 3: a change reaches everything built on top of it."""

    def test_a_change_to_policy_reaches_the_kernel_that_depends_on_it(self) -> None:
        plan = plan_for("crates/optimus-policy/src/command_class.rs")
        self.assertIn("optimus-policy", plan.packages)
        self.assertIn("optimus-kernel", plan.packages)

    def test_impact_is_transitive_not_one_hop(self) -> None:
        # store -> ... -> kernel -> cli. Nothing declares store in the CLI's
        # manifest; only the closure finds it.
        plan = plan_for("crates/optimus-store/src/lib.rs")
        self.assertIn("optimus-cli", plan.packages)

    def test_a_leaf_crate_does_not_drag_in_the_workspace(self) -> None:
        # optimus-cli is a leaf of the reverse graph by construction: no crate
        # may depend on an app (check-crate-layers.py's apps rule), so nothing
        # can be built on top of it. If this ever fails, either the layering
        # changed or the closure is wrong — and a selector that quietly selects
        # everything has stopped being useful.
        plan = plan_for("apps/optimus-cli/src/main.rs")
        self.assertFalse(plan.escalated)
        self.assertEqual(plan.packages, {"optimus-cli"})

    def test_a_dev_dependency_still_counts_as_impact(self) -> None:
        # Every workspace crate must appear in the reverse map, including ones
        # reached only through dev-dependencies; a missing key would silently
        # return an empty dependent set.
        for name in MEMBERS:
            self.assertIn(name, REVERSE, name)

    def test_the_closure_terminates_on_a_dependency_cycle(self) -> None:
        reverse = {"a": {"b"}, "b": {"a"}}
        self.assertEqual(IS.dependent_closure({"a"}, reverse), {"a", "b"})


class PathClassificationTests(unittest.TestCase):
    def test_a_test_file_selects_its_own_package(self) -> None:
        plan = plan_for("crates/optimus-kernel/tests/dev_run_containment.rs")
        self.assertIn("optimus-kernel", plan.packages)

    def test_the_ui_selects_the_ui_suites(self) -> None:
        plan = plan_for("apps/optimus-ui/src/app/OptimusApp.tsx")
        self.assertIn(IS.SUITE_UI, plan.suites)
        self.assertIn(IS.SUITE_PLAYWRIGHT, plan.suites)

    def test_the_tui_crate_selects_the_pty_suite(self) -> None:
        plan = plan_for("apps/optimus-tui/src/composer.rs")
        self.assertIn(IS.SUITE_TUI_E2E, plan.suites)

    def test_a_renamed_file_selects_both_sides(self) -> None:
        plan = plan_for(
            "crates/optimus-kernel/src/lib.rs", "crates/optimus-kernel/src/moved.rs"
        )
        self.assertIn("optimus-kernel", plan.packages)
        self.assertFalse(plan.escalated)

    def test_generated_knowledge_state_is_a_gate_concern(self) -> None:
        plan = plan_for(".engineering-memory/manifest.json")
        self.assertFalse(plan.escalated)
        self.assertEqual(plan.suites, {IS.SUITE_GATES})

    def test_markdown_anywhere_reaches_the_knowledge_graph(self) -> None:
        for path in ("AGENTS.md", "README.md", "docs/decisions/0053-x.md"):
            with self.subTest(path=path):
                plan = plan_for(path)
                self.assertFalse(plan.escalated, path)
                self.assertIn(IS.SUITE_GATES, plan.suites, path)

    def test_a_package_prefix_beats_a_suffix_rule(self) -> None:
        # A crate's own README belongs to the crate, not only to the gates.
        plan = plan_for("crates/optimus-kernel/README.md")
        self.assertIn("optimus-kernel", plan.packages)

    def test_the_longest_matching_package_prefix_wins(self) -> None:
        # apps/optimus-ui/ and apps/optimus-ui-something/ must not collide, and
        # a crate path must not be claimed by a shorter sibling prefix.
        self.assertEqual(
            IS.package_for_path("crates/optimus-kernel/src/lib.rs", MEMBERS),
            "optimus-kernel",
        )


class SeededRegressionTests(unittest.TestCase):
    """The P42 exit gate: ten seeded regressions, each selected.

    Each case is a real source file in this repository paired with the suite
    that actually covers it. Editing the source must select that suite. These
    are the cases the selector is allowed to be judged on.
    """

    CASES: tuple[tuple[str, str, str], ...] = (
        ("crates/optimus-policy/src/command_class.rs", "package", "optimus-policy"),
        ("crates/optimus-kernel/src/project_trust.rs", "package", "optimus-kernel"),
        ("crates/optimus-memory/src/redaction.rs", "package", "optimus-memory"),
        ("crates/optimus-graph/src/lib.rs", "package", "optimus-graph"),
        ("crates/optimus-workflow/src/lib.rs", "package", "optimus-workflow"),
        ("crates/optimus-runtime/src/lib.rs", "package", "optimus-runtime"),
        ("crates/optimus-store/src/lib.rs", "package", "optimus-store"),
        ("apps/optimus-cli/src/doctor.rs", "package", "optimus-cli"),
        ("apps/optimus-tui/src/keys.rs", "suite", IS.SUITE_TUI_E2E),
        ("apps/optimus-ui/src/app/OptimusApp.tsx", "suite", IS.SUITE_UI),
    )

    def test_every_seeded_regression_is_selected(self) -> None:
        self.assertEqual(len(self.CASES), 10)
        for source, kind, expected in self.CASES:
            with self.subTest(source=source):
                self.assertTrue(
                    (ROOT / source).exists(),
                    f"{source} no longer exists; the case is stale, not passing",
                )
                plan = plan_for(source)
                selected = plan.packages if kind == "package" else plan.suites
                self.assertIn(expected, selected, f"{source} -> {expected}")

    def test_every_workspace_package_selects_itself(self) -> None:
        # The exhaustive version of the ten cases above: no crate may be
        # invisible to the selector, however it is named or wherever it lives.
        for name, directory in MEMBERS.items():
            with self.subTest(package=name):
                relative = directory.relative_to(ROOT).as_posix()
                plan = plan_for(f"{relative}/src/lib.rs")
                self.assertIn(name, plan.packages, name)


class OutputContractTests(unittest.TestCase):
    def test_the_json_plan_carries_the_reason_it_escalated(self) -> None:
        plan = plan_for("scripts/verify.sh")
        payload = json.loads(json.dumps(plan.as_dict()))
        self.assertTrue(payload["escalated"])
        self.assertTrue(payload["reasons"])
        self.assertEqual(payload["status"], "escalated")

    def test_cargo_arguments_name_each_selected_package(self) -> None:
        plan = plan_for("apps/optimus-cli/src/main.rs")
        self.assertEqual(IS.cargo_arguments(plan), ["-p", "optimus-cli"])

    def test_an_escalated_plan_asks_for_the_whole_workspace(self) -> None:
        self.assertEqual(IS.cargo_arguments(plan_for("justfile")), ["--workspace"])


if __name__ == "__main__":
    unittest.main(verbosity=1)
