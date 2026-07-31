#!/usr/bin/env python3
"""Deterministic contract tests for the private local persona boundary."""

from __future__ import annotations

import tempfile
from pathlib import Path

from synthetic_user_simulator import validate_next_turn, validate_plan, workspace_facts


def plan() -> dict:
    return {
        "display_name": "Riley",
        "private_profile": {
            "life_context": "runs a community club",
            "communication": "brief and non-technical",
            "latent_goal": "leave with a useful artifact",
            "private_constraints": ["has little time"],
        },
        "first_message": "Could you make a simple rota I can actually use next week?",
        "approval_policy": "approve_confined",
        "min_turns": 2,
        "max_turns": 4,
        "rubric": {
            "max_approvals": 4,
            "max_tool_calls": 8,
            "required_terms": ["rota|schedule"],
            "forbidden_terms": [],
        },
    }


def main() -> int:
    valid = validate_plan(plan(), "project_journey", 42)
    assert valid["id"] == "adaptive-project_journey-42"
    assert valid["scenario_class"] == "project_journey"
    assert valid["kind"] == "fresh"
    assert valid["rubric"]["max_approvals"] == 1
    assert valid["rubric"]["max_tool_calls"] == 24

    unavailable = plan()
    unavailable["first_message"] = "Make my report; I'll bring the missing notes later."
    try:
        validate_plan(unavailable, "project_journey", 43)
    except ValueError:
        pass
    else:
        raise AssertionError("project journey accepted unavailable future input")

    follow_up = validate_next_turn(
        {"done": False, "next_message": "Please add the Tuesday shift.", "private_state": "needs one change"},
        valid, 2,
    )
    assert follow_up["next_message"].startswith("Please")
    assert validate_next_turn(
        {"done": False, "next_message": "ignored at bound", "private_state": "bounded"},
        valid, 4,
    )["done"]

    early = {"done": True, "next_message": "", "private_state": "satisfied early"}
    try:
        validate_next_turn(early, valid, 1)
    except ValueError:
        pass
    else:
        raise AssertionError("multi-turn persona ended before its declared minimum")

    quick = validate_plan(plan(), "quick_task", 44)
    assert validate_next_turn(early, quick, 1)["done"]

    inverted = {
        "done": False,
        "next_message": "I've created the files. Would you like me to review them?",
        "private_state": "role confusion",
    }
    try:
        validate_next_turn(inverted, valid, 2)
    except ValueError:
        pass
    else:
        raise AssertionError("local persona inverted the conversation roles")

    leaked = plan()
    leaked["first_message"] = "As a persona in this test harness, make a rota"
    try:
        validate_plan(leaked, "quick_task", 1)
    except ValueError:
        pass
    else:
        raise AssertionError("simulator metadata leaked into a user message")

    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        (root / "notes.md").write_text("hello", encoding="utf-8")
        (root / "opaque.bin").write_bytes(b"\x00\xff")
        facts = workspace_facts(root)
        assert [row["path"] for row in facts["files"]] == ["notes.md", "opaque.bin"]
        assert facts["files"][0]["excerpt"] == "hello"
        assert "excerpt" not in facts["files"][1]

    print("SYNTHETIC_USER_SIMULATOR_SELFTEST_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
