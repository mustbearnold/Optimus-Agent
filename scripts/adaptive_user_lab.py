#!/usr/bin/env python3
"""Run a local adaptive human against OAuth Optimus through the real TUI."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from synthetic_user_lab import (
    ROOT, TMUX, capture, extract_observation, send_text, tmux, wait_for_ready, wait_for_turn,
)
from synthetic_user_lab_eval import evaluate
from synthetic_user_simulator import OllamaSimulator, SCENARIO_CLASSES, workspace_facts


def final_assistant(observation: dict[str, Any]) -> str:
    if not observation["sessions"]:
        raise RuntimeError("Optimus produced no durable session")
    answers = [
        row["content"] for row in observation["sessions"][-1]["messages"]
        if row["role"] == "assistant"
    ]
    if not answers:
        raise RuntimeError("Optimus produced no durable assistant answer")
    return answers[-1]


def public_scenario(plan: dict[str, Any]) -> dict[str, Any]:
    return {"id": plan["id"], "kind": plan["kind"], "rubric": plan["rubric"]}


def run(
    plan: dict[str, Any], simulator: OllamaSimulator, binary: Path, auth_source: Path,
    target_model: str | None, thinking: str, run_dir: Path, timeout: float, seed: int,
) -> dict[str, Any]:
    home = run_dir / "optimus-home"
    frames = run_dir / "frames"
    private = run_dir / "private"
    for directory in (home, frames, private):
        directory.mkdir(parents=True, exist_ok=True)
    (private / "persona-plan.json").write_text(
        json.dumps(plan, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    copied_auth = home / "auth.json"
    shutil.copyfile(auth_source, copied_auth)
    copied_auth.chmod(0o600)
    session = f"optimus-adaptive-{os.getpid()}"
    tmux("kill-session", "-t", session)
    transcript: list[dict[str, str]] = []
    simulator_states: list[dict[str, Any]] = []
    prompt = plan["first_message"]
    expected = 0
    try:
        launched = tmux(
            "new-session", "-d", "-s", session, "-x", "120", "-y", "38",
            f"'{binary}' --home '{home}'",
        )
        if launched.returncode != 0:
            raise RuntimeError(f"tmux could not launch TUI: {launched.stderr}")
        wait_for_ready(session)
        send_text(session, "/provider codex")
        wait_for_ready(session)
        if target_model:
            send_text(session, f"/model {target_model}")
            wait_for_ready(session)
        send_text(session, f"/thinking {thinking}")
        wait_for_ready(session)

        while expected < plan["max_turns"]:
            expected += 1
            send_text(session, prompt)
            wait_for_turn(home, expected, session, plan["approval_policy"], timeout)
            (frames / f"turn-{expected:02d}.txt").write_text(capture(session), encoding="utf-8")
            observation = extract_observation(home, expected)
            answer = final_assistant(observation)
            transcript.extend([
                {"role": "user", "content": prompt},
                {"role": "assistant", "content": answer},
            ])
            next_turn = simulator.next_turn(
                plan, transcript, workspace_facts(home / "workspace"), expected,
                seed + expected,
            )
            simulator_states.append({"turn": expected, **next_turn})
            if next_turn["done"]:
                break
            prompt = next_turn["next_message"]
    finally:
        tmux("send-keys", "-t", session, "C-c")
        time.sleep(0.3)
        tmux("kill-session", "-t", session)
        if copied_auth.exists():
            copied_auth.unlink()

    observation = extract_observation(home, expected)
    workspace = workspace_facts(home / "workspace")
    grade = evaluate(public_scenario(plan), {**observation, "workspace": workspace})
    result = {
        "version": 1,
        "scenario_id": plan["id"],
        "scenario_class": plan["scenario_class"],
        "transcript": transcript,
        "simulator_states": simulator_states,
        "simulator_calls": [call.__dict__ for call in simulator.calls],
        "workspace": workspace,
        "observation": observation,
        "grade": grade,
    }
    (run_dir / "result.json").write_text(
        json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--scenario-class", choices=SCENARIO_CLASSES, default="multi_turn_revision")
    parser.add_argument("--seed", type=int, default=7312026)
    parser.add_argument("--simulator-url", default="http://127.0.0.1:11434")
    parser.add_argument("--simulator-model", default="qwen3:8b")
    parser.add_argument("--binary", type=Path, default=ROOT / "target" / "debug" / "optimus")
    parser.add_argument("--auth-source", type=Path)
    parser.add_argument("--target-model")
    parser.add_argument("--thinking", default="low")
    parser.add_argument("--timeout", type=float, default=240)
    parser.add_argument("--output", type=Path, default=ROOT / "local" / "tmp" / "adaptive-user-lab")
    parser.add_argument("--plan", action="store_true")
    args = parser.parse_args()

    simulator = OllamaSimulator(args.simulator_url, args.simulator_model)
    plan = simulator.generate_plan(args.scenario_class, args.seed)
    if args.plan:
        safe = {key: value for key, value in plan.items() if key != "private_profile"}
        safe["private_profile_sha256"] = hashlib.sha256(
            json.dumps(plan["private_profile"], sort_keys=True).encode()
        ).hexdigest()
        print(json.dumps(safe, indent=2, sort_keys=True))
        return 0

    if not Path(TMUX).is_file():
        raise SystemExit("tmux is required to drive the native TUI")
    if not args.binary.is_file():
        raise SystemExit(f"candidate binary does not exist: {args.binary}")
    if args.auth_source is None or not args.auth_source.is_file():
        raise SystemExit("a run requires --auth-source pointing to Codex auth.json")

    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    run_dir = args.output / f"{stamp}-{args.scenario_class}-seed-{args.seed}"
    run_dir.mkdir(parents=True, exist_ok=False)
    manifest = {
        "version": 1,
        "created_at": datetime.now(timezone.utc).isoformat(),
        "seed": args.seed,
        "scenario_id": plan["id"],
        "scenario_class": args.scenario_class,
        "candidate_sha256": hashlib.sha256(args.binary.read_bytes()).hexdigest(),
        "simulator": {"provider": "ollama-local", "model": args.simulator_model},
        "target": {"provider": "codex-oauth", "model": args.target_model or "provider-default", "thinking": args.thinking},
        "private_profile_sha256": hashlib.sha256(
            json.dumps(plan["private_profile"], sort_keys=True).encode()
        ).hexdigest(),
    }
    (run_dir / "manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    result = run(
        plan, simulator, args.binary, args.auth_source, args.target_model,
        args.thinking, run_dir, args.timeout, args.seed,
    )
    manifest["resolved_candidate_bindings"] = result["observation"]["candidate_bindings"]
    (run_dir / "manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(
        f"ADAPTIVE_USER_LAB_OK={result['grade']['passed']} score={result['grade']['score']} "
        f"turns={result['observation']['expected_turns']} evidence={run_dir}"
    )
    return 0 if result["grade"]["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
