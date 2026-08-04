#!/usr/bin/env python3
"""Seeded Ollama-backed artificial humans for the Synthetic User Lab.

The simulator owns private identity, motivation, and state. Optimus receives
only `first_message` and later `next_message` values. This module never calls
Optimus and never grades Optimus; it is deliberately one side of the blind
three-role boundary documented in docs/specifications/synthetic-user-lab.md.
"""

from __future__ import annotations

import json
import random
import re
import urllib.error
import urllib.request
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


SCENARIO_CLASSES = (
    "quick_task",
    "multi_turn_revision",
    "research",
    "recovery",
    "longitudinal",
    "project_journey",
)

DOMAINS = (
    "home and family logistics",
    "community volunteering",
    "small business administration",
    "education and study",
    "creative work",
    "personal computing",
    "local research",
    "clubs and events",
    "health-neutral daily organization",
    "making a useful digital project",
)

COMMUNICATION_STYLES = (
    "rushed fragments",
    "warm but numerically uncertain",
    "precise and terse",
    "anxious and easily overwhelmed",
    "confident but missing one key fact",
    "non-technical and outcome-focused",
    "chatty with corrections after seeing progress",
)

COMPLICATIONS = (
    "a constraint changes after the first answer",
    "the person assumes an unstated ordinary detail is obvious",
    "the first attempt should reveal an ambiguity",
    "the person needs a concrete artifact, not advice",
    "the person returns after a failure and expects continuity",
    "the goal is clear but the path is not",
    "the person notices a practical omission only after seeing the first result",
)

PROJECT_ANCHORS = (
    {
        "description": (
            "a useful offline one-page website for a fictional community event; explicitly request "
            "index.html, styles.css, and README.md"
        ),
        "required_any": ("index.html", "styles.css"),
    },
    {
        "description": (
            "a small-club operations kit with an editable member-rota CSV and a short usage guide; "
            "explicitly request rota.csv and README.md"
        ),
        "required_any": ("rota.csv",),
    },
    {
        "description": (
            "a tiny offline Python command-line tracker with automated tests and a usage guide; "
            "explicitly request tracker.py, test_tracker.py, and README.md"
        ),
        "required_any": ("tracker.py", "test_tracker.py"),
    },
    {
        "description": (
            "a practical event-planning kit; explicitly request checklist.md, budget.csv, and schedule.md"
        ),
        "required_any": ("checklist.md", "budget.csv", "schedule.md"),
    },
    {
        "description": (
            "a self-contained study pack; explicitly request study-guide.md, practice-quiz.md, "
            "and answer-key.md"
        ),
        "required_any": ("study-guide.md", "practice-quiz.md", "answer-key.md"),
    },
)


PLAN_SCHEMA: dict[str, Any] = {
    "type": "object",
    "required": [
        "display_name", "private_profile", "first_message", "approval_policy",
        "min_turns", "max_turns", "rubric",
    ],
    "properties": {
        "display_name": {"type": "string"},
        "private_profile": {
            "type": "object",
            "required": ["life_context", "communication", "latent_goal", "private_constraints"],
            "properties": {
                "life_context": {"type": "string"},
                "communication": {"type": "string"},
                "latent_goal": {"type": "string"},
                "private_constraints": {"type": "array", "items": {"type": "string"}},
            },
        },
        "first_message": {"type": "string"},
        "approval_policy": {"type": "string", "enum": ["deny", "approve_confined"]},
        "min_turns": {"type": "integer", "minimum": 1, "maximum": 8},
        "max_turns": {"type": "integer", "minimum": 1, "maximum": 12},
        "rubric": {
            "type": "object",
            "required": ["max_approvals", "max_tool_calls", "required_terms", "forbidden_terms"],
            "properties": {
                "max_approvals": {"type": "integer", "minimum": 0, "maximum": 30},
                "max_tool_calls": {"type": "integer", "minimum": 0, "maximum": 60},
                "required_terms": {
                    "type": "array", "minItems": 1, "maxItems": 3,
                    "items": {"type": "string"},
                },
                "forbidden_terms": {
                    "type": "array", "maxItems": 2, "items": {"type": "string"},
                },
            },
        },
    },
}

NEXT_SCHEMA: dict[str, Any] = {
    "type": "object",
    "required": ["done", "next_message", "private_state"],
    "properties": {
        "done": {"type": "boolean"},
        "next_message": {"type": "string"},
        "private_state": {"type": "string"},
    },
}


@dataclass
class SimulatorCall:
    purpose: str
    seed: int
    duration_ns: int
    prompt_tokens: int
    output_tokens: int


@dataclass
class OllamaSimulator:
    base_url: str = "http://127.0.0.1:11434"
    model: str = "qwen3:8b"
    timeout: float = 180.0
    calls: list[SimulatorCall] = field(default_factory=list)

    def _chat(
        self, purpose: str, seed: int, messages: list[dict[str, str]], schema: dict[str, Any]
    ) -> dict[str, Any]:
        payload = {
            "model": self.model,
            "messages": messages,
            "stream": False,
            "think": False,
            "format": schema,
            "options": {"seed": seed, "temperature": 0.8, "num_ctx": 16384},
        }
        request = urllib.request.Request(
            f"{self.base_url.rstrip('/')}/api/chat",
            data=json.dumps(payload).encode(),
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        try:
            with urllib.request.urlopen(request, timeout=self.timeout) as response:
                result = json.loads(response.read())
        except (urllib.error.URLError, TimeoutError, json.JSONDecodeError) as error:
            raise RuntimeError(f"local persona call failed: {error}") from error
        try:
            content = json.loads(result["message"]["content"])
        except (KeyError, TypeError, json.JSONDecodeError) as error:
            raise RuntimeError("local persona returned no valid structured message") from error
        self.calls.append(SimulatorCall(
            purpose=purpose,
            seed=seed,
            duration_ns=int(result.get("total_duration", 0)),
            prompt_tokens=int(result.get("prompt_eval_count", 0)),
            output_tokens=int(result.get("eval_count", 0)),
        ))
        return content

    def generate_plan(self, scenario_class: str, seed: int) -> dict[str, Any]:
        if scenario_class not in SCENARIO_CLASSES:
            raise ValueError(f"unknown scenario class: {scenario_class}")
        chooser = random.Random(seed)
        axes = {
            "domain": chooser.choice(DOMAINS),
            "communication": chooser.choice(COMMUNICATION_STYLES),
            "complication": chooser.choice(COMPLICATIONS),
        }
        project_anchor = chooser.choice(PROJECT_ANCHORS) if scenario_class == "project_journey" else None
        if project_anchor:
            axes["project_anchor"] = project_anchor["description"]
        class_instruction = {
            "quick_task": "A bounded useful outcome, with one meaningful follow-up.",
            "multi_turn_revision": "An artifact or plan that changes over several natural corrections.",
            "research": "A source-sensitive real-world question with uncertainty and a useful final synthesis.",
            "recovery": "A task likely to hit one recoverable failure; the person follows up naturally.",
            "longitudinal": "A returning person whose later request depends on a prior session.",
            "project_journey": (
                "A real, useful project with a concrete goal and multiple deliverables. It should take "
                "several turns, but must remain safe inside an isolated workspace and need no purchase, "
                "account creation, secret, external message, or destructive host action."
            ),
        }[scenario_class]
        system = (
            "Create one plausible human for broad assistant capability testing. You are only the human, "
            "never the assistant, evaluator, or developer. Private fields stay hidden from the assistant. "
            "Give the person one concrete task and a finish line that the assistant can actually act on. "
            "The first_message must request that outcome directly and sound like something a person would "
            "genuinely type: no persona name, test language, rubric, role-play label, simulation marker, "
            "generic question about how assistants work, or demand to avoid permissions. Do not over-specify "
            "everything up front; leave one natural detail for a follow-up. Keep all requested effects inside "
            "the isolated workspace. Never require a purchase, account, credential, external message, or host "
            "configuration change. Use one to three required rubric terms that prove the requested outcome, "
            "with a vertical bar between acceptable wording alternatives. Forbidden terms should normally be "
            "empty and may only name an objectively wrong result, never a conversational style preference. "
            f"Scenario class: {scenario_class}. Shape: {class_instruction}"
        )
        if scenario_class == "project_journey":
            system += (
                " For this project journey, directly ask Optimus to create a real artifact or collection "
                "of files in its workspace. Include every fact needed to begin, or explicitly allow sensible "
                "invented content. The person must not promise to bring, upload, or send missing inputs later."
            )
        last_error: ValueError | None = None
        for attempt in range(4):
            attempt_seed = seed + 10_000 * attempt
            raw = self._chat(
                "plan", attempt_seed,
                [
                    {"role": "system", "content": system},
                    {"role": "user", "content": f"Use these variation axes: {json.dumps(axes)}"},
                ],
                PLAN_SCHEMA,
            )
            try:
                plan = validate_plan(raw, scenario_class, seed)
            except ValueError as error:
                last_error = error
                continue
            if project_anchor and not any(
                marker in plan["first_message"].casefold()
                for marker in project_anchor["required_any"]
            ):
                last_error = ValueError("project journey omitted its concrete artifact anchor")
                continue
            plan["generation_attempt"] = attempt + 1
            plan["generation_seed"] = attempt_seed
            if project_anchor:
                plan["project_anchor"] = project_anchor["description"]
            return plan
        raise ValueError(f"local persona could not produce a valid plan: {last_error}")

    def next_turn(
        self,
        plan: dict[str, Any],
        transcript: list[dict[str, str]],
        workspace: dict[str, Any],
        turn_number: int,
        seed: int,
    ) -> dict[str, Any]:
        system = (
            "Continue as the same private human. Respond to what the assistant actually accomplished, not "
            "what an ideal assistant would have done. Ask one natural next question, correction, or request. "
            "Never mention a test, simulator, persona, rubric, hidden goal, or workspace inspection. Do not "
            "do the assistant's work, narrate the assistant's actions as your own, or ask what the assistant "
            "would like you to do. Speak only as the user who wants the outcome. Set done=true only when the "
            "real goal is satisfied; then next_message "
            "must be empty. If visible workspace facts contradict the assistant, remain unsatisfied."
        )
        must_continue = plan["scenario_class"] in {
            "multi_turn_revision", "recovery", "longitudinal", "project_journey",
        }
        if must_continue:
            system += (
                " This scenario specifically exercises a multi-turn journey. Before min_turns, do not set "
                "done; reveal one plausible correction, omitted preference, validation request, or next "
                "project phase at a time. Keep each follow-up grounded in the private goal and actual work."
            )
        schema = NEXT_SCHEMA
        if must_continue and turn_number < plan["min_turns"]:
            schema = {
                "type": "object",
                "required": ["done", "next_message", "private_state"],
                "properties": {
                    "done": {"type": "boolean", "enum": [False]},
                    "next_message": {"type": "string", "minLength": 1},
                    "private_state": {"type": "string"},
                },
            }
        private_context = {
            "private_profile": plan["private_profile"],
            "scenario_class": plan["scenario_class"],
            "turn_number": turn_number,
            "min_turns": plan["min_turns"],
            "max_turns": plan["max_turns"],
            "transcript": transcript,
            "workspace_facts": workspace,
        }
        last_error: ValueError | None = None
        for attempt in range(3):
            raw = self._chat(
                "next_turn", seed + 10_000 * attempt,
                [
                    {"role": "system", "content": system},
                    {"role": "user", "content": json.dumps(private_context, sort_keys=True)},
                ],
                schema,
            )
            try:
                return validate_next_turn(raw, plan, turn_number)
            except ValueError as error:
                last_error = error
        raise ValueError(f"local persona could not produce a valid follow-up: {last_error}")


def _reject_leak(text: str, label: str) -> None:
    lowered = text.casefold()
    markers = ("[sim", "synthetic user", "test harness", "hidden rubric", "as a persona")
    if not text.strip() or any(marker in lowered for marker in markers):
        raise ValueError(f"{label} is empty or leaks simulator state")


def validate_plan(raw: dict[str, Any], scenario_class: str, seed: int) -> dict[str, Any]:
    required = {
        "display_name", "private_profile", "first_message", "approval_policy",
        "min_turns", "max_turns", "rubric",
    }
    if not isinstance(raw, dict) or not required.issubset(raw):
        raise ValueError("local persona plan is incomplete")
    _reject_leak(str(raw["first_message"]), "first message")
    if scenario_class == "project_journey" and re.search(
        r"\b(i(?:'|’)ll|i will) (?:bring|upload|send)|\b(?:bring|upload|send) .{0,30} later\b",
        str(raw["first_message"]), re.IGNORECASE,
    ):
        raise ValueError("project journey depends on unavailable future input")
    if raw["approval_policy"] not in {"deny", "approve_confined"}:
        raise ValueError("local persona chose an unsafe approval policy")
    minimum, maximum = int(raw["min_turns"]), int(raw["max_turns"])
    if not 1 <= minimum <= maximum <= 12:
        raise ValueError("local persona turn bounds are invalid")
    rubric = raw["rubric"]
    rubric_fields = {"max_approvals", "max_tool_calls", "required_terms", "forbidden_terms"}
    if not isinstance(rubric, dict) or set(rubric) != rubric_fields:
        raise ValueError("local persona rubric fields are invalid")
    for key in ("required_terms", "forbidden_terms"):
        if not isinstance(rubric[key], list) or not all(
            isinstance(value, str) and value.strip() for value in rubric[key]
        ):
            raise ValueError(f"local persona rubric {key} is invalid")
    if len(rubric["required_terms"]) > 3 or len(rubric["forbidden_terms"]) > 2:
        raise ValueError("local persona rubric is too broad")
    profile = raw["private_profile"]
    if not isinstance(profile, dict) or not profile:
        raise ValueError("local persona private profile is invalid")
    normalized = dict(raw)
    budget = {
        "quick_task": (0, 6),
        "multi_turn_revision": (1, 12),
        "research": (0, 12),
        "recovery": (1, 12),
        "longitudinal": (1, 12),
        "project_journey": (1, 24),
    }[scenario_class]
    normalized["rubric"] = {
        **rubric,
        "forbidden_terms": [],
        "max_approvals": budget[0] if raw["approval_policy"] == "approve_confined" else 0,
        "max_tool_calls": budget[1],
    }
    normalized.update({
        "id": f"adaptive-{scenario_class}-{seed}",
        "scenario_class": scenario_class,
        "kind": "returning" if scenario_class == "longitudinal" else "fresh",
        "min_turns": minimum,
        "max_turns": maximum,
    })
    return normalized


def validate_next_turn(raw: dict[str, Any], plan: dict[str, Any], turn_number: int) -> dict[str, Any]:
    if not isinstance(raw, dict) or not {"done", "next_message", "private_state"}.issubset(raw):
        raise ValueError("local persona next-turn result is incomplete")
    done = bool(raw["done"])
    must_continue = plan["scenario_class"] in {
        "multi_turn_revision", "recovery", "longitudinal", "project_journey",
    }
    if must_continue and turn_number < plan["min_turns"] and done:
        raise ValueError("multi-turn persona ended before its declared minimum")
    if turn_number >= plan["max_turns"]:
        return {"done": True, "next_message": "", "private_state": str(raw["private_state"])}
    message = str(raw["next_message"])
    if done:
        if message.strip():
            raise ValueError("completed local persona emitted another user message")
    else:
        _reject_leak(message, "next message")
        role_inversions = (
            "would you like me to", "i've created", "i’ve created", "i have created",
            "i can help you", "let me know what you'd like me", "let me know what you’d like me",
        )
        if any(marker in message.casefold() for marker in role_inversions):
            raise ValueError("local persona inverted the user and assistant roles")
    return {"done": done, "next_message": message, "private_state": str(raw["private_state"])}


def workspace_facts(path: Path, max_files: int = 20) -> dict[str, Any]:
    """Return bounded, synthetic-workspace-only facts for the private human."""
    if not path.exists():
        return {"exists": False, "files": []}
    files = []
    for candidate in sorted(item for item in path.rglob("*") if item.is_file())[:max_files]:
        relative = candidate.relative_to(path).as_posix()
        item: dict[str, Any] = {"path": relative, "bytes": candidate.stat().st_size}
        if candidate.stat().st_size <= 65_536 and candidate.suffix.casefold() in {
            ".txt", ".md", ".csv", ".json", ".html", ".css", ".js", ".ts", ".py",
        }:
            try:
                item["excerpt"] = candidate.read_text(encoding="utf-8")[:1200]
            except UnicodeDecodeError:
                pass
        files.append(item)
    return {"exists": True, "files": files, "truncated": len(files) == max_files}
