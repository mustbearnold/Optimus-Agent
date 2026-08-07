#!/usr/bin/env python3
"""Synthetic desktop task harness: drive the installed Optimus Desktop App.

The evaluation loop that issue #126 mandates: launch the installed app with
the WebKit remote inspector enabled and an isolated ``--home``, discover the
page target over the inspector's HTTP server, evaluate the native DOM,
submit prompts through the real composer (provider pinned to the
deterministic offline echo), capture the inspector console stream, and bind
durable traces from ``sessions.db`` / ``execution.db`` read-only.

Wire protocol (WebKitGTK remote inspector, empirically pinned):

  * ``WEBKIT_INSPECTOR_HTTP_SERVER=127.0.0.1:<port>`` on the installed Tauri
    binary serves an HTTP inspector. ``GET /`` returns the target list HTML;
    the debuggable page is the ``/socket/<conn>/<page>/WebPage`` path.
  * The socket speaks WebKit's multiplexed inspector protocol: the backend
    emits ``Target.targetCreated`` (grab the ``type: page`` target id),
    accepts ``Target.setPauseOnStart``, and proxies page-level commands
    through ``Target.sendMessageToTarget`` with responses arriving as
    ``Target.dispatchMessageFromTarget`` events wrapping the inner JSON.
  * ``Runtime.evaluate`` / ``Runtime.enable`` / ``Console.enable`` work on
    the page target; console traffic and uncaught exceptions arrive as inner
    ``Runtime.consoleAPICalled`` / ``Runtime.exceptionThrown`` events.

The composer provider is pinned through the real settings popover (click the
``Model and run settings`` trigger, set Provider=Offline, Model=
offline-scripted) so every run is deterministic: the host answers each prompt
with ``offline echo: <message>`` and records execution manifests with zero
tool calls and zero approvals.

Design rules:

  * Evidence ladder first: the transcript is read back from the native DOM,
    never from the HTTP development shell. SQLite binds the durable layer.
  * The installed binary is the primary target; repo builds are fallbacks.
  * Missing binary / missing ``websockets`` / missing display hardware
    produce a ``DESKTOP_TASK_HARNESS_SKIP: <reason>`` marker and exit 0 —
    the established self-skip pattern (see ``ui_layout_audit_webkit.py``).
  * Everything else that goes wrong is a failure with evidence, never a skip.
"""

from __future__ import annotations

import asyncio
import json
import os
import re
import shutil
import signal
import sqlite3
import subprocess
import sys
import tempfile
import time
import urllib.request
from pathlib import Path
from typing import Any

from desktop_task_evidence import Evidence  # noqa: E402
from desktop_task_atspi import atspi_available, choose_channel, submit_prompt  # noqa: E402
import websockets

ROOT = Path(__file__).resolve().parents[2]
INSTALLED_TAURI = Path.home() / ".local/share/optimus-agent/bin/optimus-agent-tauri"
REPO_TAURI = ROOT / "target" / "debug" / "optimus-agent"
REPO_DESKTOP = ROOT / "target" / "debug" / "optimus-desktop"

# -- DOM contracts (React workbench, native surface) -------------------------

COMPOSER_TEXTAREA = 'textarea[aria-label="Message Optimus"]'
SEND_BUTTON = 'button[aria-label="Send message"]'
SETTINGS_TRIGGER = 'button[aria-label="Model and run settings"]'
NEW_THREAD_BUTTON = 'button[aria-label="New thread"]'

JS_COMPOSER_READY = (
    "JSON.stringify({ta: !!document.querySelector('" + COMPOSER_TEXTAREA +
    "'), send: !!document.querySelector('" + SEND_BUTTON + "')})"
)

JS_TRANSCRIPT = (
    "JSON.stringify(Array.from(document.querySelectorAll('.message')).map(m => "
    "({role: m.classList.contains('message-assistant') ? 'assistant' : 'user',"
    " text: m.querySelector('.message-body')?.textContent.trim() || ''})))"
)

JS_STATUS_MODEL = "JSON.stringify(document.querySelector('.composer-settings-model')?.textContent)"

JS_SUBMIT = """(() => {
  const ta = document.querySelector(%(ta)s);
  if (!ta) return 'no textarea';
  const setter = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set;
  setter.call(ta, %(prompt)s);
  ta.dispatchEvent(new Event('input', { bubbles: true }));
  const btn = document.querySelector(%(send)s);
  if (!btn) return 'set-but-no-send';
  if (btn.disabled) return 'send-disabled';
  btn.click();
  return 'submitted';
})()"""

JS_PIN_OFFLINE = """(() => {
  const trigger = document.querySelector(%(trigger)s);
  if (!trigger) return 'no-settings-trigger';
  trigger.click();
  return 'clicked';
})()"""

JS_PIN_OFFLINE_SET = """(() => {
  const popover = document.querySelector('div.composer-settings-popover');
  if (!popover) return 'no-popover';
  const selects = Array.from(popover.querySelectorAll('select'));
  if (selects.length < 3) return 'wrong-select-count:' + selects.length;
  const setSelect = (sel, value) => {
    const setter = Object.getOwnPropertyDescriptor(window.HTMLSelectElement.prototype, 'value').set;
    setter.call(sel, value);
    sel.dispatchEvent(new Event('change', { bubbles: true }));
  };
  setSelect(selects[0], 'offline');
  setSelect(selects[1], 'offline-scripted');
  const label = document.querySelector('.composer-settings-model');
  return 'pinned provider=' + selects[0].value + ' model=' + selects[1].value +
         ' label=' + (label ? label.textContent : '?');
})()"""

JS_CLOSE_SETTINGS = """(() => {
  const trigger = document.querySelector(%(trigger)s);
  if (!trigger) return 'no-trigger';
  if (trigger.getAttribute('aria-expanded') === 'true') trigger.click();
  return 'closed';
})()"""

JS_NEW_THREAD = """(() => {
  const button = document.querySelector(%(button)s);
  if (!button) return 'no-new-thread-button';
  button.click();
  return 'clicked';
})()"""

JS_SESSION_EMPTY = (
    "JSON.stringify({messages: document.querySelectorAll('.message').length,"
    " empty: !!document.querySelector('.work-empty')})"
)

JS_OPEN_SESSION = """(() => {
  const wanted = %(prefix)s;
  const rows = Array.from(document.querySelectorAll('button.session-select'));
  const row = rows.find(r => (r.querySelector('.session-title')?.textContent || '').startsWith(wanted));
  if (!row) return 'no-session-row';
  row.click();
  return 'opened';
})()"""


# -- environment -------------------------------------------------------------

def resolve_binary(explicit: Path | None) -> Path | None:
    candidates = [explicit, INSTALLED_TAURI, REPO_TAURI, REPO_DESKTOP]
    for candidate in candidates:
        if candidate and candidate.is_file() and os.access(candidate, os.X_OK):
            return candidate
    return None


def free_port() -> int:
    import socket

    for port in range(9230, 9300):
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
            try:
                sock.bind(("127.0.0.1", port))
                return port
            except OSError:
                continue
    raise RuntimeError("no free inspector port in 9230..9299")


# -- WebKit inspector client -------------------------------------------------

class InspectorError(RuntimeError):
    pass


class Inspector:
    """Multiplexed WebKit remote-inspector client for one page target."""

    def __init__(self, port: int):
        self.port = port
        self.ws: Any = None
        self.target_id: str | None = None
        self._next_id = 1000
        self._pending: dict[int, asyncio.Future] = {}
        self.events: list[dict[str, Any]] = []

    @staticmethod
    def _fetch(url: str, timeout: float = 4) -> str:
        with urllib.request.urlopen(url, timeout=timeout) as resp:
            return resp.read().decode(errors="replace")

    def discover_socket_path(self) -> str:
        html = self._fetch(f"http://127.0.0.1:{self.port}/")
        if "Inspectable targets" not in html:
            raise InspectorError("inspector server did not report targets")
        match = re.search(r"/socket/[^'\" ]+", html)
        if not match:
            raise InspectorError("target list carried no socket path")
        return match.group(0)

    async def connect(self) -> None:
        path = await asyncio.to_thread(self.discover_socket_path)
        self.ws = await websockets.connect(
            f"ws://127.0.0.1:{self.port}{path}", max_size=2**26, open_timeout=10
        )
        asyncio.create_task(self._reader())
        deadline = time.monotonic() + 15
        while not self.target_id and time.monotonic() < deadline:
            await asyncio.sleep(0.2)
        if not self.target_id:
            raise InspectorError("no page target appeared on the inspector socket")
        await self._send("Target.setPauseOnStart", {"pauseOnStart": False})

    async def _reader(self) -> None:
        try:
            async for raw in self.ws:
                message = json.loads(raw)
                method = message.get("method")
                if method == "Target.targetCreated":
                    info = message["params"]["targetInfo"]
                    if info.get("type") == "page":
                        self.target_id = info["targetId"]
                elif method == "Target.dispatchMessageFromTarget":
                    inner = json.loads(message["params"]["message"])
                    if "id" in inner:
                        future = self._pending.pop(inner["id"], None)
                        if future and not future.done():
                            future.set_result(inner)
                    else:
                        self.events.append(inner)
                elif method:
                    self.events.append(message)
                else:
                    future = self._pending.pop(message.get("id"), None)
                    if future and not future.done():
                        future.set_result(message)
        except Exception:  # noqa: BLE001 — socket teardown during close()
            return

    async def _send(self, method: str, params: dict, timeout: float = 20) -> dict:
        request_id = self._next_id
        self._next_id += 1
        future: asyncio.Future = asyncio.get_event_loop().create_future()
        self._pending[request_id] = future
        await self.ws.send(json.dumps({"id": request_id, "method": method, "params": params}))
        return await asyncio.wait_for(future, timeout=timeout)

    async def page(self, method: str, params: dict, timeout: float = 20) -> dict:
        inner_id = self._next_id
        self._next_id += 1
        future: asyncio.Future = asyncio.get_event_loop().create_future()
        self._pending[inner_id] = future
        inner = json.dumps({"id": inner_id, "method": method, "params": params})
        await self._send(
            "Target.sendMessageToTarget", {"targetId": self.target_id, "message": inner}
        )
        return await asyncio.wait_for(future, timeout=timeout)

    async def evaluate(self, expression: str, timeout: float = 20) -> Any:
        result = await self.page(
            "Runtime.evaluate",
            {"expression": expression, "returnByValue": True},
            timeout=timeout,
        )
        if "error" in result:
            raise InspectorError(f"Runtime.evaluate error: {result['error']}")
        value = result.get("result", {}).get("result", {})
        if value.get("type") == "undefined":
            return None
        return value.get("value")

    async def close(self) -> None:
        if self.ws:
            try:
                await self.ws.close()
            except Exception:  # noqa: BLE001
                pass


# -- app session -------------------------------------------------------------

class AppSession:
    """Owns the Xvfb display + app process for one isolated task run."""

    def __init__(self, binary: Path, home: Path, inspector_port: int):
        self.binary = binary
        self.home = home
        self.inspector_port = inspector_port
        self.display = os.environ.get("DISPLAY", "")
        self.xvfb: subprocess.Popen | None = None
        self.app: subprocess.Popen | None = None
        self.log_path = home.parent / "app.log"

    def start(self) -> None:
        env = dict(os.environ)
        if not self.display:
            if not shutil.which("Xvfb"):
                raise RuntimeError("no DISPLAY and Xvfb is not installed")
            self.display = f":{free_port() % 100 + 40}"
            self.xvfb = subprocess.Popen(
                ["Xvfb", self.display, "-screen", "0", "1440x900x24"],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
        env.update({
            "DISPLAY": self.display,
            "WEBKIT_INSPECTOR_HTTP_SERVER": f"127.0.0.1:{self.inspector_port}",
            "GDK_BACKEND": "x11",
            "WINIT_UNIX_BACKEND": "x11",
            "WEBKIT_DISABLE_COMPOSITING_MODE": "1",
        })
        with self.log_path.open("wb") as log:
            self.app = subprocess.Popen(
                [str(self.binary), "--home", str(self.home)],
                stdout=log,
                stderr=subprocess.STDOUT,
                env=env,
            )

    def stop(self) -> None:
        if self.app and self.app.poll() is None:
            self.app.send_signal(signal.SIGTERM)
            try:
                self.app.wait(timeout=8)
            except subprocess.TimeoutExpired:
                self.app.kill()
                self.app.wait(timeout=5)
        if self.xvfb and self.xvfb.poll() is None:
            self.xvfb.terminate()
            try:
                self.xvfb.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.xvfb.kill()
                self.xvfb.wait(timeout=5)


# -- durable trace extraction (read-only) ------------------------------------

def ro_connect(path: Path) -> sqlite3.Connection:
    return sqlite3.connect(f"file:{path}?mode=ro", uri=True)


def extract_traces(home: Path) -> dict[str, Any]:
    """Read-only projection of sessions.db + execution.db (no credentials)."""
    traces: dict[str, Any] = {"sessions": [], "turns": [], "execution": {}}
    sessions_db = home / "sessions.db"
    if sessions_db.exists():
        with ro_connect(sessions_db) as conn:
            for session_id, title, messages_json in conn.execute(
                "SELECT id, title, messages_json FROM sessions ORDER BY created_at, id"
            ):
                messages = json.loads(messages_json)
                traces["sessions"].append({
                    "id": session_id,
                    "title": title,
                    "messages": [
                        {"role": row.get("role", ""), "content": row.get("content", "")}
                        for row in messages
                        if row.get("role") not in (None, "system")
                    ],
                })
            traces["turns"] = [
                {
                    "id": row[0],
                    "session_id": row[1],
                    "status": row[2],
                    "error_code": row[3],
                }
                for row in conn.execute(
                    "SELECT id, session_id, status, error_code FROM session_turns "
                    "ORDER BY created_at, id"
                )
            ]
    execution_db = home / "execution.db"
    if execution_db.exists():
        with ro_connect(execution_db) as conn:
            traces["execution"] = {
                "manifests": conn.execute("SELECT count(*) FROM execution_manifests").fetchone()[0],
                "tool_calls": conn.execute("SELECT count(*) FROM execution_tool_calls").fetchone()[0],
                "approvals": conn.execute("SELECT count(*) FROM execution_chat_approvals").fetchone()[0],
                "timing_events": conn.execute("SELECT count(*) FROM execution_timing_events").fetchone()[0],
                "durations_ms": [
                    row[0]
                    for row in conn.execute(
                        "SELECT COALESCE(duration_ms, 0) FROM execution_manifests "
                        "ORDER BY created_unix, id"
                    )
                ],
                "bindings": [
                    {
                        "provider": row[0],
                        "model": row[1],
                        "autonomy_profile": row[2],
                    }
                    for row in conn.execute(
                        "SELECT DISTINCT provider, model, autonomy_profile "
                        "FROM execution_manifests ORDER BY provider, model"
                    )
                ],
            }
    return traces


# -- grading (pure; unit-tested) ---------------------------------------------

def _failure(failures: list[str], label: str, detail: Any) -> None:
    failures.append(f"{label}: {detail}")


def grade_task(expect: dict[str, Any], observation: dict[str, Any]) -> dict[str, Any]:
    """Grade an observation against a task contract. Pure and deterministic."""
    failures: list[str] = []
    db = observation.get("db", {})
    dom = observation.get("dom", {})
    turns = db.get("turns", [])
    sessions = db.get("sessions", [])
    execution = db.get("execution", {})

    if len(turns) != expect.get("turns", 0):
        _failure(failures, "turn count", f"expected {expect.get('turns')}, got {len(turns)}")
    bad_turns = [t for t in turns if t.get("status") != "succeeded"]
    if bad_turns:
        _failure(failures, "turn status", f"non-succeeded turns: {bad_turns}")
    if len(sessions) != expect.get("sessions", 0):
        _failure(failures, "session count", f"expected {expect.get('sessions')}, got {len(sessions)}")

    if expect.get("echo"):
        transcript = dom.get("transcript", [])
        assistant_text = " ".join(
            m.get("text", "") for m in transcript if m.get("role") == "assistant"
        )
        for prompt in expect.get("prompts", []):
            if prompt not in assistant_text:
                _failure(failures, "echo", f"assistant text missing prompt {prompt!r}")
            if not any(m.get("text") == prompt for m in transcript if m.get("role") == "user"):
                _failure(failures, "user message", f"prompt {prompt!r} missing from DOM transcript")

    if expect.get("bindings"):
        actual_bindings = {
            (b.get("provider"), b.get("model")) for b in execution.get("bindings", [])
        }
        expected_bindings = {tuple(b) for b in expect["bindings"]}
        if actual_bindings != expected_bindings:
            _failure(failures, "bindings", f"expected {expected_bindings}, got {actual_bindings}")

    if execution.get("tool_calls") != expect.get("tool_calls", 0):
        _failure(failures, "tool calls", execution.get("tool_calls"))
    if execution.get("approvals") != expect.get("approvals", 0):
        _failure(failures, "approvals", execution.get("approvals"))
    if expect.get("min_timing_events") is not None:
        timing_events = execution.get("timing_events", 0)
        if timing_events < expect["min_timing_events"]:
            _failure(failures, "timing events", f"expected >= {expect['min_timing_events']}, got {timing_events}")
    expected_manifests = expect.get("manifests")
    if expected_manifests is not None and execution.get("manifests") != expected_manifests:
        _failure(failures, "manifests", f"expected {expected_manifests}, got {execution.get('manifests')}")

    if expect.get("new_session_empty") and not dom.get("new_session_empty"):
        _failure(failures, "new session empty", "workbench did not show an empty session")

    if expect.get("isolation"):
        # The last session is the one created after the earlier sessions; its
        # transcript must not leak the earlier sessions' prompts/nonces.
        if sessions:
            last_session_text = " ".join(
                m.get("content", "") for m in sessions[-1].get("messages", [])
            )
            for foreign in expect.get("foreign_prompts", []):
                if foreign in last_session_text:
                    _failure(failures, "isolation", f"prompt {foreign!r} leaked into the new session")
        else:
            _failure(failures, "isolation", "no sessions recorded to check")

    if expect.get("reopen"):
        if not dom.get("reopened_transcript"):
            _failure(failures, "reopen", "transcript did not restore after session reopen")
        else:
            # Only the reopened session's own prompts must be restored (the
            # other sessions' prompts were never typed into it).
            for prompt in expect.get("reopen_prompts", expect.get("prompts", [])):
                if prompt not in " ".join(dom["reopened_transcript"]):
                    _failure(failures, "reopen content", f"prompt {prompt!r} not restored")

    console_events = observation.get("console", {}).get("events", [])
    exceptions = [
        ev for ev in console_events if ev.get("method") == "Runtime.exceptionThrown"
    ]
    if expect.get("max_exceptions", 0) is not None and len(exceptions) > expect.get("max_exceptions", 0):
        _failure(failures, "console exceptions", f"{len(exceptions)} uncaught exceptions")

    return {
        "passed": not failures,
        "failures": failures,
        "console_event_count": len(console_events),
        "console_exceptions": len(exceptions),
    }


# -- task execution ----------------------------------------------------------

async def wait_for(
    predicate, description: str, timeout: float, interval: float = 1.0
) -> Any:
    deadline = time.monotonic() + timeout
    last = None
    while time.monotonic() < deadline:
        last = await predicate()
        if last:
            return last
        await asyncio.sleep(interval)
    raise TimeoutError(f"timed out waiting for {description} (last={last!r})")


async def run_task(
    binary: Path,
    task: dict[str, Any],
    output_dir: Path,
    timeout: float = 240,
    enforce_ocr: bool = True,
    ocr_model: str = "qwen3.5:9b",
    ocr_endpoint: str = "http://127.0.0.1:11434",
) -> dict[str, Any]:
    """Run one task contract against a fresh app instance and home.

    Capture is ALWAYS enforced: every settled turn gets a screenshot, and a
    failed capture fails the task. When `enforce_ocr`, the final capture is
    OCR-verified with the qwen vision model and must contain the last
    prompt's nonce before the task may pass.
    """
    session_prompts_formatted: list[list[str]] = []
    for session_templates in task["sessions"]:
        session_prompts_formatted.append(
            [
                template.format(nonce=f"T{len(session_prompts_formatted) + 1}-{secrets_hex(4).upper()}")
                for template in session_templates
            ]
        )
    prompts = [p for session in session_prompts_formatted for p in session]

    home = Path(tempfile.mkdtemp(prefix=f"optimus-task-{task['id']}-"))
    evidence: dict[str, Any] = {
        "task_id": task["id"],
        "difficulty": task.get("difficulty", ""),
        "prompts": prompts,
        "started_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "dom": {},
        "console": {"events": []},
        "db": {},
        "captures": [],
    }
    # Input channel probe (spec-014 R13): hosts with the AT stack submit
    # through AT-SPI; everyone else keeps the deterministic DOM channel.
    # The decision is recorded per prompt in the evidence.
    atspi_ok, atspi_reason = atspi_available()
    evidence["atspi"] = {"available": atspi_ok, "reason": atspi_reason, "prompts": []}
    evidence_obj = Evidence(output_dir, ocr_model, ocr_endpoint)
    inspector: Inspector | None = None
    session = AppSession(binary, home, free_port())
    try:
        session.start()
        evidence_obj.display = session.display  # capture against the session's Xvfb/Wayland display
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
        await inspector.evaluate(JS_PIN_OFFLINE % {"trigger": json.dumps(SETTINGS_TRIGGER)})
        await asyncio.sleep(0.6)
        pinned = await inspector.evaluate(
            JS_PIN_OFFLINE_SET % {"trigger": json.dumps(SETTINGS_TRIGGER)}
        )
        if "pinned provider=offline" not in str(pinned):
            raise RuntimeError(f"could not pin offline provider: {pinned}")
        evidence["dom"]["provider_pin"] = pinned
        await inspector.evaluate(JS_CLOSE_SETTINGS % {"trigger": json.dumps(SETTINGS_TRIGGER)})
        model = await inspector.evaluate(JS_STATUS_MODEL)
        if "offline-scripted" not in str(model):
            raise RuntimeError(f"provider pin did not stick: model label {model!r}")

        expected_turns = 0
        for session_index, session_prompts in enumerate(session_prompts_formatted):
            if session_index:
                await inspector.evaluate(JS_NEW_THREAD % {"button": json.dumps(NEW_THREAD_BUTTON)})
                empty = await wait_for(
                    lambda: inspector.evaluate(JS_SESSION_EMPTY),
                    "empty new session",
                    timeout=30,
                    interval=0.5,
                )
                evidence["dom"]["new_session_empty"] = "empty" in str(empty)
            for prompt in session_prompts:
                expected_turns += 1
                channel = choose_channel(atspi_ok)
                submitted = None
                entry: dict[str, Any] = {"turn": expected_turns, "channel": channel}
                if atspi_ok:
                    atspi_ev = await asyncio.to_thread(submit_prompt, prompt)
                    entry.update(atspi_ev)
                    if atspi_ev["ok"]:
                        submitted = "submitted"
                if submitted is None:
                    submitted = await inspector.evaluate(
                        JS_SUBMIT
                        % {
                            "ta": json.dumps(COMPOSER_TEXTAREA),
                            "send": json.dumps(SEND_BUTTON),
                            "prompt": json.dumps(prompt),
                        }
                    )
                    if submitted == "submitted":
                        entry["ok"] = True
                        entry["detail"] = "dom submit via inspector"
                    elif atspi_ok:
                        entry["fell_back_to_dom"] = True
                evidence["atspi"]["prompts"].append(entry)
                if submitted != "submitted":
                    raise RuntimeError(f"composer submission failed: {submitted}")
                await wait_for(
                    lambda: self_transcript_count(inspector, expected_turns),
                    f"turn {expected_turns} in transcript",
                    timeout=timeout,
                    interval=1.0,
                )
                await wait_for(
                    lambda: self_turn_settled(home, expected_turns),
                    f"turn {expected_turns} settled in db",
                    timeout=timeout,
                    interval=0.5,
                )
                # Enforced capture: every settled turn MUST have a screenshot.
                evidence["captures"].append({
                    "label": f"turn-{expected_turns:02d}",
                    "path": str(evidence_obj.capture(f"turn-{expected_turns:02d}")),
                })

        transcript = await inspector.evaluate(JS_TRANSCRIPT)
        evidence["dom"]["transcript"] = json.loads(transcript) if isinstance(transcript, str) else []

        if task.get("reopen_first_session"):
            first_prefix = prompts[0][:24]
            opened = await inspector.evaluate(JS_OPEN_SESSION % {"prefix": json.dumps(first_prefix)})
            evidence["dom"]["reopen_click"] = opened
            reopened = await wait_for(
                lambda: self_reopened(inspector, prompts[0]),
                "first session transcript restored",
                timeout=60,
                interval=1.0,
            )
            evidence["dom"]["reopened_transcript"] = reopened

        # Enforced OCR gate: the final capture must contain the last nonce
        # (which the offline echo reply carries) before the task may pass.
        if enforce_ocr and evidence["captures"]:
            last_capture = Path(evidence["captures"][-1]["path"])
            nonce = re.search(r"T\d+-[0-9A-F]+", prompts[-1])
            required = [nonce.group(0)] if nonce else [prompts[-1]]
            record = await asyncio.to_thread(evidence_obj.ocr, last_capture, required)
            evidence["ocr"] = record
            if record["missing_required_terms"]:
                raise RuntimeError(
                    f"final capture OCR missing required terms: {record['missing_required_terms']}"
                )

        await asyncio.sleep(1)
        evidence["db"] = await asyncio.to_thread(extract_traces, home)
        evidence["console"]["events"] = [ev for ev in inspector.events]
        grade = grade_task(
            {**task["expect"], "prompts": prompts},
            {
                "dom": evidence["dom"],
                "db": evidence["db"],
                "console": evidence["console"],
            },
        )
        evidence["grade"] = grade
        evidence["ended_at"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
        (output_dir / "evidence.json").write_text(
            json.dumps(evidence, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
        return evidence
    finally:
        if inspector:
            await inspector.close()
        session.stop()
        if not os.environ.get("DESKTOP_TASK_HARNESS_KEEP_HOME"):
            shutil.rmtree(home, ignore_errors=True)


def secrets_hex(length: int) -> str:
    import secrets

    return secrets.token_hex(length)


async def self_transcript_count(inspector: Inspector, expected: int) -> bool:
    try:
        raw = await inspector.evaluate(JS_TRANSCRIPT)
    except InspectorError:
        return False
    try:
        messages = json.loads(raw) if isinstance(raw, str) else []
        return sum(1 for m in messages if m.get("role") == "assistant") >= expected
    except (TypeError, json.JSONDecodeError):
        return False


async def self_turn_settled(home: Path, expected: int) -> bool:
    try:
        traces = await asyncio.to_thread(extract_traces, home)
    except Exception:  # noqa: BLE001 — db may be mid-write
        return False
    turns = traces["turns"]
    if len(turns) < expected:
        return False
    return turns[expected - 1]["status"] == "succeeded"


async def self_reopened(inspector: Inspector, prompt: str) -> list[Any]:
    try:
        raw = await inspector.evaluate(JS_TRANSCRIPT)
    except InspectorError:
        return []
    try:
        messages = json.loads(raw) if isinstance(raw, str) else []
    except (TypeError, json.JSONDecodeError):
        return []
    texts = [m.get("text", "") for m in messages]
    if any(prompt in text for text in texts):
        return texts
    return []


def main() -> int:
    import argparse

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--task", type=Path, help="task contract JSON file")
    parser.add_argument("--binary", type=Path, help="candidate binary override")
    parser.add_argument("--output", type=Path, default=ROOT / "Development" / "tmp" / "desktop-task-harness")
    parser.add_argument("--timeout", type=float, default=240)
    args = parser.parse_args()

    binary = resolve_binary(args.binary)
    if binary is None:
        print(
            "DESKTOP_TASK_HARNESS_SKIP: no installed or repo candidate binary "
            "(run scripts/rebuild-install-relaunch.sh or cargo build -p optimus-agent)"
        )
        return 0
    try:
        import websockets  # noqa: F401
    except ImportError:
        print("DESKTOP_TASK_HARNESS_SKIP: python websockets module missing")
        return 0

    if args.task is None or not args.task.is_file():
        print("DESKTOP_TASK_HARNESS_FAIL: --task JSON contract required", file=sys.stderr)
        return 1
    task = json.loads(args.task.read_text(encoding="utf-8"))
    stamp = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())
    run_dir = args.output / f"{stamp}-{task['id']}"
    run_dir.mkdir(parents=True, exist_ok=True)
    try:
        evidence = asyncio.run(run_task(binary, task, run_dir, args.timeout))
    except (TimeoutError, RuntimeError, InspectorError) as error:
        print(f"DESKTOP_TASK_HARNESS_FAIL {task['id']}: {error}", file=sys.stderr)
        return 1
    grade = evidence["grade"]
    if grade["passed"]:
        print(f"DESKTOP_TASK_OK task={task['id']} console_events={grade['console_event_count']} evidence={run_dir}")
        return 0
    print(f"DESKTOP_TASK_FAIL task={task['id']}", file=sys.stderr)
    for failure in grade["failures"]:
        print(f"  {failure}", file=sys.stderr)
    print(f"  evidence: {run_dir / 'evidence.json'}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
