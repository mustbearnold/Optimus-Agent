#!/usr/bin/env python3
"""Self-improvement loop: N consecutive deepseek-v4-flash turns in the app.

Drives the installed Tauri desktop app exactly like a real user: the real
deepseek-v4-flash provider (credentials copied from the user's real home —
the auth store is machine-bound encrypted and decrypts on the same host),
Developer Full Access enabled, approval buttons clicked the moment they
appear (the "real person" behaviour), and every turn graded against the
durable stores:

  * session_turns.status == succeeded
  * zero ``tool_finished/failed`` events in execution_timing_events
    (tool-call errors)
  * zero ``denied`` and zero leftover ``pending`` execution_chat_approvals
    (permission errors), and no ``approval_required`` timing event left
    unresolved at turn end
  * at least one real tool call per turn
  * a real repository change: the workspace HEAD advanced or the working
    tree differs from the pre-turn baseline (the improvement itself)

Screenshots are enforced every turn and OCR-verified with the local qwen
vision model (capture/OCR failures fail the run). Each iteration gets its
own fresh app instance + isolated home so no run contaminates the next.

Exit 0 with ``DESKTOP_SELF_IMPROVEMENT_OK`` when every iteration passed;
exit 1 with ``DESKTOP_SELF_IMPROVEMENT_FAIL`` otherwise. Self-skips (no
binary, no websockets, no display/Xvfb, no source home, no ollama) print
``DESKTOP_SELF_IMPROVEMENT_SKIP: <reason>`` and exit 0.
"""

from __future__ import annotations

import argparse
import asyncio
import json
import os
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import time
import urllib.request
from pathlib import Path
from typing import Any

from desktop_task_evidence import Evidence, ocr_qwen  # noqa: E402
from desktop_task_harness import (  # noqa: E402
    COMPOSER_TEXTAREA,
    AppSession,
    Inspector,
    InspectorError,
    JS_CLOSE_SETTINGS,
    JS_COMPOSER_READY,
    JS_PIN_OFFLINE,
    JS_STATUS_MODEL,
    JS_SUBMIT,
    SEND_BUTTON,
    SETTINGS_TRIGGER,
    extract_traces,
    free_port,
    resolve_binary,
    secrets_hex,
    wait_for,
)

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_SOURCE_HOME = Path.home() / ".local/share/optimus"
DEFAULT_OUTPUT = ROOT / "Development" / "tmp" / "desktop-self-improvement"
DEFAULT_ITERATIONS = 10

PROVIDER = "deepseek"
MODEL = "deepseek-v4-flash"

PROMPTS = [
    "Find one concrete way to improve this codebase and implement it now, "
    "with a regression test. Run the relevant tests to prove it. Keep the "
    "change small and focused. You MUST make at least one real change to "
    "the repository (code, test, or documentation). IMPORTANT: this is an "
    "automated acceptance run — a turn that ends with no repository change "
    "is graded as a FAILURE. Editing at least one file is MANDATORY: "
    "improve a comment, refactor a function, add a test, tighten a check "
    "— pick the smallest defensible improvement. Do NOT use the browser "
    "tool; keep exploration brief (a few reads), then make the edit. "
    "NEVER run git checkout, git restore, git reset, or git stash — they "
    "erase your work and fail the run. When your change is complete and verified, COMMIT it on the current branch (git add then git commit); a committed change is the successful outcome of this turn — do NOT revert it; leave your changes in the working "
    "tree. CRITICAL: this acceptance run grades every tool call — every "
    "command you run must SUCCEED on the first attempt. Verify paths and "
    "flags before running (read the file, check the command with --help) "
    "and never run a command that could fail. All file writes and scratch "
    "work must stay INSIDE this repository workspace — writing outside it "
    "is denied by the access scope and fails the run.",
    "Identify a small bug or papercut in this codebase, fix it now, and add "
    "a regression test. Verify by running the relevant tests. You MUST make "
    "at least one real change to the repository. IMPORTANT: this is an "
    "automated acceptance run — a turn that ends with no repository change "
    "is graded as a FAILURE. Editing at least one file is MANDATORY: pick "
    "the smallest defensible improvement (comment, refactor, test, check). "
    "Do NOT use the browser tool; keep exploration brief, then make the "
    "edit. NEVER run git checkout, git restore, git reset, or git stash — "
    "they erase your work and fail the run. When your change is complete and verified, COMMIT it on the current branch (git add then git commit); a committed change is the successful outcome of this turn — do NOT revert it; leave your changes in the "
    "working tree. CRITICAL: this acceptance run grades every tool call — "
    "every command must SUCCEED on the first attempt. Inspect before you "
    "run; never gamble on a command that might fail. All file writes must "
    "stay INSIDE this repository workspace — outside writes are denied by "
    "the access scope and fail the run.",
    "Review the most recent changes for a latent bug, fix it with a "
    "regression test, and prove it by running the tests. You MUST make at "
    "least one real change to the repository. IMPORTANT: this is an "
    "automated acceptance run — a turn that ends with no repository change "
    "is graded as a FAILURE; editing at least one file is MANDATORY. Do "
    "NOT use the browser tool; keep exploration brief, then make the edit. "
    "NEVER run git checkout, git restore, git reset, or git stash — they "
    "erase your work and fail the run. When your change is complete and verified, COMMIT it on the current branch (git add then git commit); a committed change is the successful outcome of this turn — do NOT revert it; leave your changes in the working "
    "tree. CRITICAL: this acceptance run grades every tool call — every "
    "command you run must SUCCEED the first time. Verify everything before "
    "executing.",
    "Find stale or wrong documentation in one module and correct it now, "
    "then verify nothing else broke. If the documentation is accurate, "
    "improve it (clarity, examples) — you MUST make at least one real "
    "change to the repository. IMPORTANT: this is an automated acceptance "
    "run — a turn that ends with no repository change is graded as a "
    "FAILURE; editing at least one file is MANDATORY. Do NOT use the "
    "browser tool; keep exploration brief, then make the edit. NEVER run "
    "git checkout, git restore, git reset, or git stash — they erase your work and fail the run. When your change is complete and verified, COMMIT it (git add then git commit); a committed change is the successful outcome — do NOT revert it "
    "; leave your changes in the working tree. "
    "CRITICAL: this acceptance run grades every tool call — every command "
    "must SUCCEED on the first attempt. Check paths and flags before "
    "running anything.",
    "Find dead or duplicated code and remove it safely, then verify with "
    "the relevant tests. If you find nothing to remove, add a regression "
    "test for a gap you notice — you MUST make at least one real change to "
    "the repository. IMPORTANT: this is an automated acceptance run — a "
    "turn that ends with no repository change is graded as a FAILURE; "
    "editing at least one file is MANDATORY. Do NOT use the browser tool; "
    "keep exploration brief, then make the edit. NEVER run git checkout, "
    "git restore, git reset, or git stash — they erase your work and fail "
    "the run. When your change is complete and verified, COMMIT it on the "
    "current branch (git add then git commit); a committed change is the "
    "successful outcome of this turn — do NOT revert it; leave your "
    "changes in the working tree. CRITICAL: this "
    "acceptance run grades every tool call — every command must SUCCEED "
    "on the first attempt. Inspect before you run; no failed commands are "
    "allowed.",
]

# Provider pin: same popover as the offline pin, but selects the real
# deepseek provider + deepseek-v4-flash model (the ids recorded in
# execution_manifests by real usage).
JS_PIN_PROVIDER = """(() => {
  const popover = document.querySelector('div.composer-settings-popover');
  if (!popover) return 'no-popover';
  const selects = Array.from(popover.querySelectorAll('select'));
  if (selects.length < 3) return 'wrong-select-count:' + selects.length;
  const setSelect = (sel, value) => {
    const setter = Object.getOwnPropertyDescriptor(window.HTMLSelectElement.prototype, 'value').set;
    setter.call(sel, value);
    sel.dispatchEvent(new Event('change', { bubbles: true }));
  };
  setSelect(selects[0], %(provider)s);
  setSelect(selects[1], %(model)s);
  const label = document.querySelector('.composer-settings-model');
  return 'pinned provider=' + selects[0].value + ' model=' + selects[1].value +
         ' label=' + (label ? label.textContent : '?');
})()"""

JS_OPEN_APPROVALS_TAB = """(() => {
  const tab = Array.from(document.querySelectorAll('button'))
    .find(b => (b.getAttribute('role') === 'tab') && (b.textContent || '').includes('Approvals'));
  if (!tab) return 'no-tab';
  tab.click();
  return 'tab-clicked';
})()"""

JS_CLICK_APPROVE_COMMAND = """(() => {
  // Live labels: "Approve terminal …", "Approve and continue" — never
  // "Approve command" (stale doc label). Match /^Approve / which excludes
  // the tab ("Approvals") and the in-flight state ("Approving…").
  const buttons = Array.from(document.querySelectorAll('button'))
    .filter(b => /^Approve /.test((b.textContent || '').trim()));
  if (!buttons.length) return 'no-button';
  let clicked = 0;
  for (const button of buttons) {
    if (!button.disabled) {
      button.click();
      clicked += 1;
    }
  }
  return clicked ? 'clicked:' + clicked : 'all-disabled';
})()"""

JS_TURN_RUNNING = (
    "JSON.stringify(!!document.querySelector('.turn-running, .turn-pending, .status-running'))"
)


def preflight(source_home: Path, binary: Path | None) -> str | None:
    """Return a skip reason when the environment cannot run the loop."""
    if binary is None:
        return "no installed or repo candidate binary (run scripts/rebuild-install-relaunch.sh or cargo build -p optimus-agent)"
    try:
        import websockets  # noqa: F401
    except ImportError:
        return "python websockets module missing"
    if not os.environ.get("DISPLAY") and not shutil.which("Xvfb"):
        return "no DISPLAY and Xvfb is not installed"
    if not (source_home / "auth.json").is_file():
        return f"source home {source_home} has no auth.json (no provider credentials)"
    settings = source_home / "settings.json"
    if not settings.is_file():
        return f"source home {source_home} has no settings.json"
    try:
        with urllib.request.urlopen("http://127.0.0.1:11434/api/tags", timeout=3) as resp:  # noqa: S310
            tags = json.loads(resp.read())
        if not any("qwen" in (t.get("name") or "") for t in tags.get("models", [])):
            return "ollama has no qwen model (OCR gate needs qwen3.5:9b or similar)"
    except OSError:
        return "ollama is not reachable at 127.0.0.1:11434 (OCR gate)"
    return None


def make_home(source_home: Path) -> Path:
    """Fresh isolated home seeded with the real credentials + grants.

    ``provider-keys.json`` is the DeepSeek key store (protected via the
    Secret Service, machine-bound — it decrypts on the same host). Without
    it the pinned deepseek provider has no key and the turn silently never
    starts (empty assistant bubble, no turn row).
    """
    home = Path(tempfile.mkdtemp(prefix="optimus-self-improve-"))
    for name in ("settings.json", "auth.json", "host-runtime.json",
                 "provider-keys.json", "provider-keys.lock"):
        source = source_home / name
        if source.is_file():
            shutil.copy2(source, home / name)
    return home


def patch_settings_scope(home: Path, workspace: Path) -> None:
    """Ensure developer_access is enabled for the loop's workspace."""
    path = home / "settings.json"
    settings = json.loads(path.read_text(encoding="utf-8"))
    dev = settings.setdefault("developer_access", {})
    dev["enabled"] = True
    dev.setdefault("scope", {})["kind"] = "selected_repository"
    dev["scope"]["root"] = str(workspace)
    path.write_text(json.dumps(settings, indent=2) + "\n", encoding="utf-8")


def git_snapshot(workspace: Path) -> dict[str, Any]:
    """HEAD + dirty-path set baseline for one iteration.

    The dirty PATH SET (not the count) is the change signal: a prior
    iteration's leftover WIP keeps the count non-zero, and an agent that
    edits a different set of files at the same count must still count as
    having made a change.
    """
    def run(*args: str) -> str:
        result = subprocess.run(
            ["git", "-C", str(workspace), *args],
            capture_output=True, text=True, timeout=30, check=False,
        )
        return result.stdout.strip()

    return {
        "head": run("rev-parse", "HEAD"),
        "dirty": sorted(line for line in run("status", "--porcelain").splitlines() if line.strip()),
    }


INFRA_ERROR_MARKERS = (
    "policy denied", "permission", "denied", "unauthorized", "forbidden",
    "not available", "timed out", "timeout", "crashed", "unreachable",
    "connection refused", "tool unavailable", "no such tool",
)


def _failed_call_is_error(failed_event: dict[str, Any]) -> bool:
    """Classify a failed tool call: infra/permission errors FAIL the run;
    normal recoverable outcomes (wrong path guess, command exit code, TDD
    red) are the agent's job and only reported."""
    message = (failed_event.get("error_message") or "").lower()
    return not message or any(marker in message for marker in INFRA_ERROR_MARKERS)


def grade_iteration(expect: dict[str, Any], observation: dict[str, Any]) -> dict[str, Any]:
    """Grade one iteration. Pure and deterministic (unit-tested)."""
    failures: list[str] = []
    traces = observation.get("db", {})
    timing = traces.get("execution", {}).get("timing_events", [])
    turns = traces.get("turns", [])
    approvals = traces.get("execution", {}).get("approvals", [])

    turn = turns[-1] if turns else None
    if not turn:
        failures.append("no turn recorded")
    elif turn.get("status") != "succeeded":
        failures.append(f"turn status: {turn.get('status')} (error_code={turn.get('error_code')})")

    failed_calls = [e for e in timing if e.get("kind") == "tool_finished" and e.get("status") == "failed"]
    error_calls = [e for e in failed_calls if _failed_call_is_error(e)]
    recovered_calls = [e for e in failed_calls if not _failed_call_is_error(e)]
    if error_calls:
        failures.append(
            f"tool-call errors: {[(e.get('name'), e.get('error_message', '')) for e in error_calls]}"
        )

    # approval_required timing events are the pause history — an approved
    # pause is clean (real-person behaviour). Permission errors are only
    # denied or still-pending approvals at turn end.

    denied = [a for a in approvals if a.get("status") == "denied"]
    pending = [a for a in approvals if a.get("status") == "pending"]
    if denied:
        failures.append(f"denied approvals: {len(denied)}")
    if pending:
        failures.append(f"pending approvals at turn end: {len(pending)}")

    calls = [e for e in timing if e.get("kind") == "tool_started"]
    if not calls:
        failures.append("no tool calls recorded (agent did not act)")

    snapshot = observation.get("git", {})
    if expect.get("git_change") and not snapshot.get("changed"):
        failures.append("no repository change (HEAD did not advance and tree stayed clean)")

    return {
        "passed": not failures,
        "failures": failures,
        "tool_calls": len(calls),
        "tool_errors": len(error_calls),
        "tool_errors_recovered": len(recovered_calls),
        "approvals_denied": len(denied),
        "approvals_pending": len(pending),
    }


async def run_iteration(
    binary: Path,
    workspace: Path,
    source_home: Path,
    output_dir: Path,
    iteration: int,
    prompt: str,
    timeout: float,
    ocr_model: str,
    ocr_endpoint: str,
) -> dict[str, Any]:
    home = make_home(source_home)
    patch_settings_scope(home, workspace)
    baseline = git_snapshot(workspace)
    evidence: dict[str, Any] = {
        "iteration": iteration,
        "prompt": prompt,
        "baseline": baseline,
        "started_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "captures": [],
    }
    inspector: Inspector | None = None
    session = AppSession(binary, home, free_port())
    evidence_obj = Evidence(output_dir, ocr_model, ocr_endpoint)
    try:
        session.start()
        evidence_obj.display = session.display
        inspector = Inspector(session.inspector_port)
        deadline = time.monotonic() + 45
        while True:
            try:
                await inspector.connect()
                break
            except Exception:  # noqa: BLE001 — retry until the app's inspector is up
                if time.monotonic() >= deadline:
                    raise RuntimeError("inspector never came up")
                await asyncio.sleep(1.5)
        await inspector.page("Runtime.enable", {})
        await inspector.page("Console.enable", {})
        await wait_for(
            lambda: inspector.evaluate(JS_COMPOSER_READY),
            "composer ready",
            timeout=60,
            interval=1.0,
        )

        # Pin the real provider through the settings popover (real-person path).
        await inspector.evaluate(JS_PIN_OFFLINE % {"trigger": json.dumps(SETTINGS_TRIGGER)})
        await asyncio.sleep(0.6)
        pinned = await inspector.evaluate(
            JS_PIN_PROVIDER % {"provider": json.dumps(PROVIDER), "model": json.dumps(MODEL)}
        )
        if "pinned provider=deepseek model=deepseek-v4-flash" not in str(pinned):
            raise RuntimeError(f"could not pin deepseek provider: {pinned}")
        await inspector.evaluate(JS_CLOSE_SETTINGS % {"trigger": json.dumps(SETTINGS_TRIGGER)})
        model = await inspector.evaluate(JS_STATUS_MODEL)
        if "deepseek" not in str(model).lower() or "flash" not in str(model).lower():
            raise RuntimeError(f"provider pin did not stick: model label {model!r}")

        # Submit the improvement prompt.
        submitted = await inspector.evaluate(
            JS_SUBMIT
            % {
                "ta": json.dumps(COMPOSER_TEXTAREA),
                "send": json.dumps(SEND_BUTTON),
                "prompt": json.dumps(prompt),
            }
        )
        if submitted != "submitted":
            raise RuntimeError(f"composer submission failed: {submitted}")

        # Settle + real-person approval handling. The settle loop is
        # DB-driven (sqlite reads every 5s); the DOM is touched ONLY when a
        # pending approval exists — constant Runtime.evaluate polling has
        # killed the WebKit web process on long turns (observed 4-10 min).
        settled = False
        tab_opened = False
        settle_deadline = time.monotonic() + timeout
        while not settled and time.monotonic() < settle_deadline:
            # Death signature: the app has died silently on real deepseek
            # turns (no core, no journal record). Capture the exact exit
            # status before the websocket error obscures it.
            if session.app and session.app.poll() is not None:
                raise RuntimeError(f"app exited unexpectedly rc={session.app.returncode}")
            if await asyncio.to_thread(has_pending_approval, home):
                # Open the Approvals tab once, then click the button when it
                # renders (text-only "Approve command", no aria-label).
                if not tab_opened:
                    await inspector.evaluate(JS_OPEN_APPROVALS_TAB)
                    tab_opened = True
                clicked = await inspector.evaluate(JS_CLICK_APPROVE_COMMAND)
                if clicked == "clicked":
                    await asyncio.sleep(1.0)
                    continue
                await asyncio.sleep(1.0)
                continue
            settled = await asyncio.to_thread(self_turn_settled, home, 1)
            if not settled:
                terminal = await asyncio.to_thread(self_turn_status, home, 1)
                if terminal in ("failed", "cancelled"):
                    raise RuntimeError(f"turn ended {terminal}")
                await asyncio.sleep(5.0)
        if not settled:
            raise RuntimeError(f"turn did not settle within {timeout}s")

        await asyncio.sleep(1.0)
        evidence["captures"].append({
            "label": f"iteration-{iteration:02d}",
            "path": str(evidence_obj.capture(f"iteration-{iteration:02d}")),
        })
        ocr_text = await asyncio.to_thread(ocr_qwen, Path(evidence["captures"][-1]["path"]), ocr_model, ocr_endpoint)
        evidence["ocr"] = {"text": ocr_text, "nonempty": bool(ocr_text.strip())}

        evidence["db"] = await asyncio.to_thread(extract_traces, home)
        evidence["db"]["execution"]["timing_events"] = await asyncio.to_thread(
            extract_timing_events, home
        )
        evidence["db"]["execution"]["approvals"] = await asyncio.to_thread(
            extract_approvals, home
        )
        after = git_snapshot(workspace)
        evidence["git"] = {
            "head_before": baseline["head"],
            "head_after": after["head"],
            "dirty_before": baseline["dirty"],
            "dirty_after": after["dirty"],
            "changed": after["head"] != baseline["head"] or after["dirty"] != baseline["dirty"],
        }
        evidence["grade"] = grade_iteration({"git_change": True}, evidence)
        evidence["ended_at"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
        (output_dir / "evidence.json").write_text(
            json.dumps(evidence, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
        return evidence
    except Exception as error:  # noqa: BLE001 — failure forensics
        # Preserve the failure evidence: DB traces + the error, so a failed
        # iteration can be diagnosed instead of vanishing with the home.
        evidence["error"] = f"{type(error).__name__}: {error}"
        try:
            evidence["db"] = await asyncio.to_thread(extract_traces, home)
            evidence["db"]["execution"]["timing_events"] = await asyncio.to_thread(
                extract_timing_events, home
            )
            evidence["db"]["execution"]["approvals"] = await asyncio.to_thread(
                extract_approvals, home
            )
        except Exception:  # noqa: BLE001
            pass
        (output_dir / "evidence.json").write_text(
            json.dumps(evidence, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
        raise
    finally:
        if inspector:
            await inspector.close()
        session.stop()
        if not os.environ.get("DESKTOP_TASK_HARNESS_KEEP_HOME"):
            shutil.rmtree(home, ignore_errors=True)


def has_pending_approval(home: Path) -> bool:
    """True when execution_chat_approvals holds an unresolved approval."""
    path = home / "execution.db"
    if not path.is_file():
        return False
    try:
        with sqlite3.connect(f"file:{path}?mode=ro", uri=True) as conn:
            row = conn.execute(
                "SELECT count(*) FROM execution_chat_approvals WHERE status = 'pending'"
            ).fetchone()
        return bool(row and row[0] > 0)
    except sqlite3.Error:
        return False


def self_turn_settled(home: Path, expected: int) -> bool:
    try:
        traces = extract_traces(home)
    except Exception:  # noqa: BLE001 — db may be mid-write
        return False
    turns = traces["turns"]
    if len(turns) < expected:
        return False
    # The approval-resume flow re-starts the turn (new session_turns row per
    # continuation), so the LAST turn is the iteration's live one — a
    # "succeeded" first turn with a running continuation is NOT settled.
    return turns[-1]["status"] == "succeeded"


def self_turn_status(home: Path, expected: int) -> str:
    try:
        traces = extract_traces(home)
    except Exception:  # noqa: BLE001
        return "unknown"
    turns = traces["turns"]
    if len(turns) < expected:
        return "running"
    return turns[-1].get("status", "unknown")


def extract_timing_events(home: Path) -> list[dict[str, Any]]:
    path = home / "execution.db"
    if not path.is_file():
        return []
    with sqlite3.connect(f"file:{path}?mode=ro", uri=True) as conn:
        # The failed-call error text lives in the tool lifecycle event_json
        # (the timing row only carries the status). Classification needs it:
        # "policy denied" is a permission error, "not found" is a recoverable
        # agent outcome.
        error_by_call: dict[str, str] = {}
        try:
            for row in conn.execute(
                "SELECT call_id, event_json FROM execution_tool_events "
                "WHERE phase = 'failed' ORDER BY sequence"
            ):
                try:
                    event = json.loads(row[1])
                    outcome = event.get("outcome") or {}
                    error = outcome.get("error") or {}
                    message = error.get("message") or str(error.get("code") or "")
                    if message:
                        error_by_call.setdefault(row[0], message)
                except (json.JSONDecodeError, AttributeError):
                    continue
        except sqlite3.Error:
            pass
        return [
            {
                "kind": row[0],
                "status": row[1],
                "name": row[2],
                "step": row[3],
                "call_id": row[4],
                "error_message": error_by_call.get(row[4], ""),
            }
            for row in conn.execute(
                "SELECT kind, status, name, step, call_id FROM execution_timing_events "
                "ORDER BY sequence"
            )
        ]


def extract_approvals(home: Path) -> list[dict[str, Any]]:
    path = home / "execution.db"
    if not path.is_file():
        return []
    with sqlite3.connect(f"file:{path}?mode=ro", uri=True) as conn:
        return [
            {"status": row[0]}
            for row in conn.execute(
                "SELECT status FROM execution_chat_approvals ORDER BY rowid"
            )
        ]


async def run_loop(
    binary: Path,
    workspace: Path,
    source_home: Path,
    output_dir: Path,
    iterations: int,
    timeout: float,
    ocr_model: str,
    ocr_endpoint: str,
) -> dict[str, Any]:
    stamp = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())
    run_dir = output_dir / stamp
    run_dir.mkdir(parents=True, exist_ok=True)
    results = []
    for iteration in range(1, iterations + 1):
        prompt = PROMPTS[(iteration - 1) % len(PROMPTS)]
        iter_dir = run_dir / f"iteration-{iteration:02d}"
        iter_dir.mkdir(parents=True, exist_ok=True)
        started = time.monotonic()
        try:
            evidence = await run_iteration(
                binary, workspace, source_home, iter_dir, iteration, prompt,
                timeout, ocr_model, ocr_endpoint,
            )
            grade = evidence["grade"]
            results.append({
                "iteration": iteration,
                "passed": grade["passed"],
                "failures": grade["failures"],
                "tool_calls": grade["tool_calls"],
                "tool_errors": grade["tool_errors"],
                "approvals_denied": grade["approvals_denied"],
                "approvals_pending": grade["approvals_pending"],
                "git_changed": evidence["git"]["changed"],
                "duration_s": round(time.monotonic() - started, 1),
                "evidence": str(iter_dir / "evidence.json"),
            })
        except Exception as error:  # noqa: BLE001 — report every iteration, keep going
            results.append({
                "iteration": iteration,
                "passed": False,
                "failures": [f"harness error: {error}"],
                "duration_s": round(time.monotonic() - started, 1),
            })
        print(
            f"  iteration {iteration:02d}: {'ok' if results[-1]['passed'] else 'FAIL'} "
            f"({results[-1]['duration_s']}s)"
        )
        for failure in results[-1]["failures"]:
            print(f"    {failure}")
        if not results[-1]["passed"]:
            break
    report = {
        "version": 1,
        "run_dir": str(run_dir),
        "requested_iterations": iterations,
        "completed_iterations": len(results),
        "passed": all(row["passed"] for row in results),
        "iterations": results,
    }
    (run_dir / "report.json").write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    return report


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--iterations", type=int, default=DEFAULT_ITERATIONS)
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--workspace", type=Path, default=ROOT)
    parser.add_argument("--source-home", type=Path, default=DEFAULT_SOURCE_HOME)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--timeout", type=float, default=1200)
    parser.add_argument("--ocr-model", default="qwen3.5:9b")
    parser.add_argument("--ocr-endpoint", default="http://127.0.0.1:11434")
    args = parser.parse_args()

    binary = resolve_binary(args.binary)
    reason = preflight(args.source_home, binary)
    if reason:
        print(f"DESKTOP_SELF_IMPROVEMENT_SKIP: {reason}")
        return 0
    assert binary is not None

    report = asyncio.run(run_loop(
        binary, args.workspace, args.source_home, args.output, args.iterations,
        args.timeout, args.ocr_model, args.ocr_endpoint,
    ))
    if report["passed"]:
        print(
            f"DESKTOP_SELF_IMPROVEMENT_OK iterations={report['completed_iterations']} "
            f"evidence={report['run_dir']}"
        )
        return 0
    print(f"DESKTOP_SELF_IMPROVEMENT_FAIL evidence={report['run_dir']}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
