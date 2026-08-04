#!/usr/bin/env python3
"""Independent, deterministic grader for Synthetic User Lab observations.

This module never receives a simulator's private persona profile. It scores
only the public rubric and durable observations extracted after Optimus exits.
That separation prevents the same model that invented a user from declaring
its own conversation successful.
"""

from __future__ import annotations

import json
from typing import Any


def evaluate(public: dict[str, Any], observation: dict[str, Any]) -> dict[str, Any]:
    """Return a stable 0-100 score and exact, machine-readable findings."""
    rubric = public["rubric"]
    turns = observation["turns"]
    terminal = [row for row in turns if row["status"] != "running"]
    succeeded = [row for row in terminal if row["status"] == "succeeded"]
    expected = observation["expected_turns"]
    completion_ratio = len(succeeded) / expected if expected else 0.0

    final_messages = [
        message["content"]
        for message in observation["sessions"][-1]["messages"]
        if message["role"] == "assistant"
    ] if observation["sessions"] else []
    final_answer = final_messages[-1].casefold() if final_messages else ""
    workspace_evidence = json.dumps(
        observation.get("workspace", {}), sort_keys=True
    ).casefold()

    def matches(term: str) -> bool:
        return any(
            option.strip().casefold() in final_answer
            or option.strip().casefold() in workspace_evidence
            for option in term.split("|")
        )

    missing = [term for term in rubric["required_terms"] if not matches(term)]
    forbidden = [term for term in rubric["forbidden_terms"] if matches(term)]
    approvals_over = max(0, observation["approvals"] - rubric["max_approvals"])
    tools_over = max(0, observation["tool_calls"] - rubric["max_tool_calls"])
    errors = len([row for row in terminal if row["status"] in {"failed", "cancelled"}])

    dimensions = {
        "turn_completion": round(40 * completion_ratio),
        "task_evidence": 25 if not missing and not forbidden else max(0, 25 - 8 * len(missing) - 8 * len(forbidden)),
        "approval_friction": max(0, 15 - 5 * approvals_over),
        "tool_efficiency": max(0, 10 - 2 * tools_over),
        "terminal_integrity": 10 if errors == 0 and len(terminal) == expected else 0,
    }
    findings: list[dict[str, Any]] = []
    for term in missing:
        findings.append({"code": "required_term_missing", "term": term})
    for term in forbidden:
        findings.append({"code": "forbidden_term_present", "term": term})
    if approvals_over:
        findings.append({"code": "approval_budget_exceeded", "over_by": approvals_over})
    if tools_over:
        findings.append({"code": "tool_budget_exceeded", "over_by": tools_over})
    if errors:
        findings.append({"code": "terminal_turn_failures", "count": errors})
    if len(terminal) != expected:
        findings.append({"code": "turn_count_mismatch", "expected": expected, "observed": len(terminal)})

    score = sum(dimensions.values())
    return {
        "version": 1,
        "scenario_id": public["id"],
        "score": score,
        "passed": score >= 80 and not findings,
        "dimensions": dimensions,
        "findings": findings,
    }
