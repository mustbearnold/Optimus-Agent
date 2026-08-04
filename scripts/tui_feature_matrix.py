#!/usr/bin/env python3
"""Exhaustive deterministic feature matrix for the native terminal workbench.

This is the broad companion to ``tui_e2e.py`` (one critical journey), the
Rust ``TestBackend``/portable-pty tests (state and protocol contracts), and the
Playwright terminal-cell oracle (geometry).  It drives the real Optimus binary
through tmux and checks every advertised command plus keyboard, mouse, sidebar,
streaming, scrolling, persistence, project, and exit paths without credentials,
network access, or token spend.

The layered test method was refreshed on 2026-08-02 against the official
Ratatui TestBackend recipe, Crossterm event documentation, and tmux control
documentation.  Those references are printed into managed verification output
so a future audit can tell which test architecture was reviewed.
"""

from __future__ import annotations

import argparse
import http.server
import json
import os
import re
import shlex
import subprocess
import tempfile
import threading
import time
from pathlib import Path
from typing import Callable


ROOT = Path(__file__).resolve().parents[1]
METHOD_SOURCES = (
    "https://ratatui.rs/recipes/testing/snapshots/",
    "https://docs.rs/crossterm/latest/crossterm/event/index.html",
    "https://github.com/tmux/tmux/wiki/Control-Mode",
)
BRAILLE = set("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏")


class ApprovalFixtureHandler(http.server.BaseHTTPRequestHandler):
    """A tiny local Chat Completions provider for the approval journey.

    The first and third requests deliberately ask for a real ``write_file``
    call. The following request supplies the continuation answer. This keeps
    the matrix on the actual provider, kernel, PTY, and TUI boundaries instead
    of manufacturing a pending binding inside the renderer.
    """

    requests = 0

    def log_message(self, _format: str, *_args: object) -> None:
        return

    def do_POST(self) -> None:  # noqa: N802 - stdlib handler hook
        length = int(self.headers.get("Content-Length", "0"))
        self.rfile.read(length)
        type(self).requests += 1
        request_number = type(self).requests
        if request_number in (1, 3):
            filename = (
                "approval-proof.txt" if request_number == 1 else "denial-proof.txt"
            )
            message = {
                "role": "assistant",
                "content": None,
                "tool_calls": [
                    {
                        "id": f"approval-call-{request_number}",
                        "type": "function",
                        "function": {
                            "name": "write_file",
                            "arguments": json.dumps(
                                {"path": filename, "contents": "approved"}
                            ),
                        },
                    }
                ],
            }
        else:
            outcome = "approved" if request_number == 2 else "denied"
            message = {
                "role": "assistant",
                "content": f"The {outcome} continuation returned.",
            }
        body = json.dumps(
            {"choices": [{"index": 0, "message": message, "finish_reason": "stop"}]}
        ).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


class ApprovalContinuationFixtureHandler(http.server.BaseHTTPRequestHandler):
    """A provider that parks twice, once in the original turn and once in its continuation."""

    requests = 0

    def log_message(self, _format: str, *_args: object) -> None:
        return

    def do_POST(self) -> None:  # noqa: N802 - stdlib handler hook
        length = int(self.headers.get("Content-Length", "0"))
        self.rfile.read(length)
        type(self).requests += 1
        request_number = type(self).requests
        if request_number in (1, 2):
            filename = (
                "first-approval-proof.txt"
                if request_number == 1
                else "second-approval-proof.txt"
            )
            message = {
                "role": "assistant",
                "content": None,
                "tool_calls": [
                    {
                        "id": "call_1" if request_number == 1 else "call_2",
                        "type": "function",
                        "function": {
                            "name": "write_file",
                            "arguments": json.dumps(
                                {"path": filename, "contents": filename}
                            ),
                        },
                    }
                ],
            }
        else:
            message = {
                "role": "assistant",
                "content": "both approval continuations completed",
            }
        body = json.dumps(
            {"choices": [{"index": 0, "message": message, "finish_reason": "stop"}]}
        ).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


class CompletedToolFixtureHandler(http.server.BaseHTTPRequestHandler):
    """A local provider whose read-only tool turn is safe to reopen.

    The first response lists the workspace; the second answers from that
    result. The journey is intentionally completed before the process exits,
    so relaunch coverage can distinguish a polished lifecycle row from the
    model protocol envelope kept in the durable conversation.
    """

    requests = 0

    def log_message(self, _format: str, *_args: object) -> None:
        return

    def do_POST(self) -> None:  # noqa: N802 - stdlib handler hook
        length = int(self.headers.get("Content-Length", "0"))
        self.rfile.read(length)
        type(self).requests += 1
        if type(self).requests == 1:
            message = {
                "role": "assistant",
                "content": None,
                "tool_calls": [
                    {
                        "id": "list-call-1",
                        "type": "function",
                        "function": {
                            "name": "list_dir",
                            "arguments": json.dumps({"path": "."}),
                        },
                    }
                ],
            }
        else:
            message = {
                "role": "assistant",
                "content": "The directory listing came back clean.",
            }
        body = json.dumps(
            {"choices": [{"index": 0, "message": message, "finish_reason": "stop"}]}
        ).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


class ReusedCallFixtureHandler(http.server.BaseHTTPRequestHandler):
    """A provider fixture that deliberately reuses ``call_1`` on two turns."""

    requests = 0

    def log_message(self, _format: str, *_args: object) -> None:
        return

    def do_POST(self) -> None:  # noqa: N802 - stdlib handler hook
        length = int(self.headers.get("Content-Length", "0"))
        self.rfile.read(length)
        type(self).requests += 1
        request_number = type(self).requests
        if request_number in (1, 3):
            path = "alpha.txt" if request_number == 1 else "beta.txt"
            message = {
                "role": "assistant",
                "content": None,
                "tool_calls": [
                    {
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "read_file",
                            "arguments": json.dumps({"path": path}),
                        },
                    }
                ],
            }
        else:
            answer = "The alpha file was read." if request_number == 2 else "The beta file was read."
            message = {"role": "assistant", "content": answer}
        body = json.dumps(
            {"choices": [{"index": 0, "message": message, "finish_reason": "stop"}]}
        ).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


class PendingRepeatedCallFixtureHandler(http.server.BaseHTTPRequestHandler):
    """A completed call is followed by a pending call with the same provider id."""

    requests = 0

    def log_message(self, _format: str, *_args: object) -> None:
        return

    def do_POST(self) -> None:  # noqa: N802 - stdlib handler hook
        length = int(self.headers.get("Content-Length", "0"))
        self.rfile.read(length)
        type(self).requests += 1
        request_number = type(self).requests
        if request_number == 1:
            message = {
                "role": "assistant",
                "content": None,
                "tool_calls": [
                    {
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "read_file",
                            "arguments": json.dumps({"path": "alpha.txt"}),
                        },
                    }
                ],
            }
        elif request_number == 2:
            message = {
                "role": "assistant",
                "content": "The completed read alpha.",
            }
        elif request_number == 3:
            message = {
                "role": "assistant",
                "content": None,
                "tool_calls": [
                    {
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "write_file",
                            "arguments": json.dumps(
                                {"path": "pending-proof.txt", "contents": "pending"}
                            ),
                        },
                    }
                ],
            }
        else:
            message = {
                "role": "assistant",
                "content": "The pending proof was approved after switching sessions.",
            }
        body = json.dumps(
            {"choices": [{"index": 0, "message": message, "finish_reason": "stop"}]}
        ).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


class FeatureFailure(RuntimeError):
    """A named user-visible contract failed."""


def tmux(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["tmux", *args], capture_output=True, text=True, check=False
    )


def normalized(lines: list[str]) -> str:
    return re.sub(r"\s+", " ", " ".join(lines)).strip()


def main_text(lines: list[str]) -> str:
    """Visible main-workbench text, excluding titles repeated in the sidebar."""

    divider = next((line.find("│") for line in lines if "│" in line), -1)
    if divider < 0:
        return normalized(lines)
    return normalized([line[divider + 1 :] for line in lines])


def composer_text(lines: list[str]) -> str:
    top = next((index for index, line in enumerate(lines) if "╭" in line), -1)
    if top < 0:
        return ""
    bottom = next(
        (index for index in range(top + 1, len(lines)) if "╰" in lines[index]),
        len(lines),
    )
    return normalized(lines[top : bottom + 1])


def busy_visible(lines: list[str]) -> bool:
    """The busy state, identified by the status-rail marker, not a phase label.

    The kernel replaces the initial "working" label with the first "model step
    N" status milliseconds after submit, so waiting for that literal races a
    sub-millisecond window and only passes on slow hosts.  The ◌ marker and
    the absence of "turn · ready" hold for the entire turn.
    """
    text = normalized(lines)
    return "◌" in text and "turn · ready" not in text


def seed_projects(home: Path, count: int = 4) -> None:
    """Create valid already-authorized scopes, as a desktop picker would.

    The matrix is testing the TUI's project browser, not the desktop's native
    folder picker.  Its fixture therefore starts at the TUI's public persisted
    input: an authority document containing canonical roots.
    """

    projects: dict[str, object] = {}
    for index in range(1, count + 1):
        root = home.parent / f"project-{index}"
        root.mkdir(parents=True, exist_ok=True)
        canonical = str(root.resolve())
        project_id = f"project-{index}"
        projects[project_id] = {
            "project_id": project_id,
            "roots": [canonical],
            "primary_root": canonical,
            "updated_unix": int(time.time()) + index,
        }
    path = home / "project-authority.json"
    path.write_text(
        json.dumps(
            {
                "version": 1,
                "projects": projects,
                "staged_roots": [],
            }
        ),
        encoding="utf-8",
    )
    path.chmod(0o600)


class Audit:
    def __init__(self) -> None:
        self.checks = 0
        self.cases = 0

    def check(
        self,
        condition: bool,
        label: str,
        case: "Case | None" = None,
    ) -> None:
        self.checks += 1
        if condition:
            return
        frame = ""
        if case is not None and case.alive():
            frame = "\n--- last frame ---\n" + "\n".join(case.capture())
        raise FeatureFailure(f"{label}{frame}")

    def begin(self, name: str) -> None:
        self.cases += 1
        print(f"TUI_FEATURE_CASE {name}", flush=True)


class Case:
    _serial = 0

    def __init__(
        self,
        binary: Path,
        home: Path,
        *,
        cols: int = 110,
        rows: int = 32,
        environment: dict[str, str] | None = None,
    ) -> None:
        Case._serial += 1
        self.binary = binary
        self.home = home
        self.cols = cols
        self.rows = rows
        self.environment = environment or {}
        self.session = f"optimus-tui-matrix-{os.getpid()}-{Case._serial}"

    def launch(self, *, allow_approval: bool = False) -> None:
        assignments = [
            f"{key}={shlex.quote(value)}"
            for key, value in sorted(self.environment.items())
        ]
        command = " ".join(
            [
                *assignments,
                shlex.quote(str(self.binary)),
                "--home",
                shlex.quote(str(self.home)),
            ]
        )
        started = tmux(
            "new-session",
            "-d",
            "-s",
            self.session,
            "-x",
            str(self.cols),
            "-y",
            str(self.rows),
            "--",
            command,
        )
        if started.returncode != 0:
            raise FeatureFailure(
                f"tmux could not launch {self.session}: {started.stderr.strip()}"
            )
        self.wait(
            lambda lines: "· ready" in normalized(lines)
            or (allow_approval and "! approval" in normalized(lines)),
            20,
            "launch ready",
        )

    def close(self) -> None:
        tmux("kill-session", "-t", self.session)

    def alive(self) -> bool:
        return tmux("has-session", "-t", self.session).returncode == 0

    def capture(self, *, escapes: bool = False) -> list[str]:
        args = ["capture-pane", "-t", self.session, "-p"]
        if escapes:
            args.append("-e")
        result = tmux(*args)
        if result.returncode != 0:
            return []
        lines = result.stdout.replace("\r", "").splitlines()
        while len(lines) < self.rows:
            lines.append("")
        return lines[: self.rows]

    def settled_capture(self, timeout: float = 1.0) -> list[str]:
        """Capture after the terminal has produced two identical frames.

        A burst of wheel events can still be queued after the semantic target
        first becomes visible.  Comparing that in-flight frame with a later
        hover frame falsely reports a layout mutation, even though hover only
        changes styles.  Waiting for two consecutive snapshots drains the
        input/render queue without imposing a fixed sleep on every assertion.
        """

        frame = self.capture()
        deadline = time.monotonic() + timeout
        identical = 0
        while time.monotonic() < deadline:
            time.sleep(0.06)
            next_frame = self.capture()
            if next_frame == frame:
                identical += 1
                if identical >= 2:
                    return next_frame
            else:
                identical = 0
            frame = next_frame
        return frame

    def wait(
        self,
        predicate: Callable[[list[str]], bool],
        timeout: float,
        what: str,
    ) -> list[str]:
        deadline = time.monotonic() + timeout
        frame = self.capture()
        while time.monotonic() < deadline:
            if predicate(frame):
                return frame
            if not self.alive():
                raise FeatureFailure(f"{what}: TUI exited unexpectedly")
            time.sleep(0.06)
            frame = self.capture()
        raise FeatureFailure(
            f"{what} (within {timeout:.1f}s)\n--- last frame ---\n"
            + "\n".join(frame)
        )

    def wait_text(self, text: str, timeout: float = 12) -> list[str]:
        return self.wait(
            lambda lines: text in normalized(lines), timeout, f"missing {text!r}"
        )

    def wait_absent(self, text: str, timeout: float = 8) -> list[str]:
        return self.wait(
            lambda lines: text not in normalized(lines),
            timeout,
            f"{text!r} did not disappear",
        )

    def literal(self, text: str) -> None:
        result = tmux("send-keys", "-t", self.session, "-l", "--", text)
        if result.returncode != 0:
            raise FeatureFailure(result.stderr.strip() or "tmux literal send failed")

    def keys(self, *keys: str) -> None:
        result = tmux("send-keys", "-t", self.session, *keys)
        if result.returncode != 0:
            raise FeatureFailure(result.stderr.strip() or "tmux key send failed")

    def submit_draft(self, expected: str, *, expect_accept: bool = True) -> None:
        self.wait(
            lambda lines: expected in composer_text(lines),
            5,
            f"complete draft {expected!r}",
        )
        self.keys("Enter")
        if not expect_accept:
            return

        # tmux acknowledges send-keys before the application consumes the
        # input. Under the deliberately busy managed gate, one synthetic Enter
        # can occasionally be lost while a pane is being redrawn. Require the
        # accepted-state transition and make one bounded retry only while the
        # exact draft is still parked in an otherwise live composer. A genuine
        # application refusal still fails loudly after the retry.
        deadline = time.monotonic() + 1.0
        while time.monotonic() < deadline:
            frame = self.capture()
            if expected not in composer_text(frame):
                return
            if not self.alive():
                raise FeatureFailure(
                    f"submit {expected!r}: TUI exited unexpectedly"
                )
            time.sleep(0.06)
        self.keys("Enter")
        # The offline provider is paced at OPTIMUS_OFFLINE_LATENCY_MS (3000ms)
        # so busy states are visible; under host load a synthetic Enter can sit
        # unconsumed for several seconds before the pane accepts it. 15s keeps
        # the acceptance proof while absorbing the injected latency.
        self.wait(
            lambda lines: expected not in composer_text(lines),
            15,
            f"submit accepted {expected!r}",
        )

    def type_submit(self, text: str) -> None:
        self.literal(text)
        self.submit_draft(text)

    def command(self, command: str, expected: str, timeout: float = 12) -> list[str]:
        self.type_submit(command)
        settled = lambda lines: expected in normalized(lines) and (
            "Ask Optimus anything" in composer_text(lines)
        )
        self.wait(settled, timeout, f"command {command!r} -> {expected!r}")
        # tmux can snapshot while Ratatui is midway through painting the next
        # frame. Requiring the same semantic state after one render interval
        # keeps a partially updated screen from becoming evidence.
        time.sleep(0.12)
        return self.wait(
            settled,
            timeout,
            f"stable command {command!r} -> {expected!r}",
        )

    def prompt(self, prompt: str, timeout: float = 30) -> list[str]:
        self.type_submit(prompt)
        expected = re.sub(r"\s+", " ", f"offline echo: {prompt}").strip()
        return self.wait(
            lambda lines: expected in normalized(lines)
            and "· ready" in normalized(lines),
            timeout,
            f"offline turn {prompt!r}",
        )

    def paste(self, text: str) -> None:
        buffered = tmux("set-buffer", text)
        if buffered.returncode != 0:
            raise FeatureFailure(buffered.stderr.strip() or "tmux set-buffer failed")
        pasted = tmux("paste-buffer", "-t", self.session, "-p")
        if pasted.returncode != 0:
            raise FeatureFailure(pasted.stderr.strip() or "tmux paste-buffer failed")

    def mouse(self, kind: str, column: int, row: int) -> None:
        code, suffix = {
            "left-down": (0, "M"),
            "left-up": (0, "m"),
            "right-down": (2, "M"),
            "drag": (32, "M"),
            "move": (35, "M"),
            "wheel-up": (64, "M"),
            "wheel-down": (65, "M"),
        }[kind]
        sequence = f"\x1b[<{code};{column + 1};{row + 1}{suffix}"
        self.literal(sequence)

    def click(self, column: int, row: int) -> None:
        self.mouse("left-down", column, row)
        self.mouse("left-up", column, row)

    def click_text(self, needle: str, *, sidebar_only: bool = False) -> None:
        frame = self.wait_text(needle)
        for row, line in enumerate(frame):
            candidate = line.split("│", 1)[0] if sidebar_only else line
            if needle in candidate:
                self.click(candidate.index(needle), row)
                return
        raise FeatureFailure(f"could not locate clickable text {needle!r}")

    def mouse_flag(self) -> str:
        return tmux(
            "display-message", "-p", "-t", self.session, "#{mouse_any_flag}"
        ).stdout.strip()

    def wait_exit(self, timeout: float = 10) -> None:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline and self.alive():
            time.sleep(0.05)
        if self.alive():
            raise FeatureFailure(
                "TUI did not exit\n--- last frame ---\n" + "\n".join(self.capture())
            )


def command_surface(audit: Audit, case: Case) -> None:
    audit.begin("commands-and-policies")
    case.launch()
    try:
        frame = case.command("/help", "/providers")
        help_text = normalized(frame)
        for command in (
            "/providers",
            "/sessions",
            "/pinned",
            "/projects",
            "/provider <id>",
            "/model <id>",
            "/thinking <lvl>",
            "/approval",
            "/access <profile>",
            "/yolo",
            "/pin",
            "/frame",
            "/mouse",
            "/new",
            "/quit",
            "/help",
        ):
            audit.check(command in help_text, f"help omitted {command}", case)
        for instruction in (
            "Tab completes",
            "Ctrl-D exits",
            "Ctrl-A/E",
            "PageUp/PageDown",
            "Ctrl-B toggles",
        ):
            audit.check(instruction in help_text, f"help omitted {instruction}", case)
        audit.check("max|ultra" in help_text, "help omitted the ultra alias", case)

        case.command("/", "type /help for commands")
        case.command("/does-not-exist", "unknown command /does-not-exist")
        case.command("/provider", "usage: /provider <id>")
        case.command("/provider does-not-exist", "unknown provider does-not-exist")
        case.command("/provider OFFLINE", "provider is now offline")
        case.command("/model offline-scripted", "model is now offline-scripted")
        case.command("/provider auto", "choose model Auto")
        case.command("/model auto", "model selection is now Auto")
        case.command("/provider auto", "provider is now auto")
        case.command("/model matrix-model", "choose a provider")

        case.command("/thinking", "usage: /thinking")
        upper = case.command("/thinking HIGH", "thinking is now high")
        audit.check("think:high" in normalized(upper), "uppercase thinking level was not normalized", case)
        ultra = case.command("/thinking ultra", "thinking is now ultra")
        audit.check("think:ultra" in normalized(ultra), "ultra missing from status", case)
        case.command("/thinking impossible", "unknown level impossible")
        case.command("/thinking off", "thinking level left to the backend")

        case.command("/access", "access is review_changes (default)")
        case.command("/access read", "new turns run as read_only")
        case.command("/access unrestricted", "break-glass is /yolo only")
        case.command("/access banana", "unknown profile 'banana'")
        case.command("/approval", "no approval is pending")
        case.command("/pin", "send a prompt before pinning")

        case.type_submit("/yolo")
        case.wait_text("Confirm unrestricted access")
        case.keys("Enter")
        case.wait_absent("Confirm unrestricted access")
        audit.check("YOLO" not in normalized(case.capture()), "cancel enabled yolo", case)
        case.type_submit("/yolo")
        case.wait_text("Confirm unrestricted access")
        case.keys("Down", "Enter")
        yolo = case.wait_text("unrestricted access on")
        audit.check("YOLO" in normalized(yolo), "enabled yolo missing from status", case)
        case.command("/access standard", "yolo no longer rides new turns")

        case.command("/frame", "plain gutters")
        case.command("/frame", "workbench surface")
        audit.check(case.mouse_flag() == "1", "mouse capture was not initially active", case)
        case.command("/mouse", "mouse off")
        case.wait(lambda _: case.mouse_flag() == "0", 5, "mouse capture release")
        case.command("/mouse", "mouse on")
        case.wait(lambda _: case.mouse_flag() == "1", 5, "mouse capture restore")
        case.command("/new", "new session ready")
        audit.check("unknown command" not in normalized(case.capture()), "new failed", case)
    finally:
        case.close()


def picker_and_menu(audit: Audit, case: Case) -> None:
    audit.begin("pickers-suggestions-and-command-menu")
    seed_projects(case.home)
    case.launch()
    try:
        case.type_submit("/providers")
        picker = case.wait_text("Select a provider")
        for item in ("Auto (recommended)", "offline", "codex", "open-ai-compat"):
            audit.check(item in normalized(picker), f"provider picker omitted {item}", case)

        picker_frame = case.capture()
        title_row = next(
            index for index, line in enumerate(picker_frame) if "Select a provider" in line
        )
        right_border = picker_frame[title_row].rindex("┐")
        case.click(right_border, title_row + 1)
        case.wait_text("Select a provider")
        audit.check(case.alive(), "provider picker closed from a border click", case)

        case.keys("C-b")
        still_open = case.wait_text("Select a provider")
        audit.check("WORKSPACE" in normalized(still_open), "Ctrl-B leaked through picker", case)
        case.keys("Down", "Enter")
        case.wait_text("provider is now offline")

        case.type_submit("/providers")
        case.wait_text("Select a provider")
        case.keys("Escape")
        case.wait_absent("Select a provider")
        audit.check(case.alive(), "Escape exited from provider picker", case)

        case.type_submit("/providers")
        case.wait_text("Select a provider")
        case.keys("C-c")
        case.wait_absent("Select a provider")
        audit.check(case.alive(), "Ctrl-C exited instead of closing picker", case)

        case.literal("/pro")
        suggestions = case.wait_text("/provider <id>")
        audit.check("/providers" in normalized(suggestions), "providers suggestion missing", case)
        case.keys("Down", "Down", "Tab")
        completed = case.wait(
            lambda lines: "/provider" in composer_text(lines),
            5,
            "selected suggestion completion",
        )
        audit.check(
            "/provider" in composer_text(completed),
            "Tab ignored highlighted suggestion",
            case,
        )
        case.literal("offline")
        case.submit_draft("/provider offline")
        case.wait_text("provider is now offline")

        case.literal("/pro")
        case.keys("Tab")
        case.submit_draft("/providers")
        case.wait_text("Select a provider")
        case.click_text("Auto")
        case.wait_text("provider is now auto")

        case.type_submit("/projects")
        project_picker = case.wait_text("Choose a project scope")
        audit.check(
            case.home.name in normalized(project_picker),
            "project picker omitted workspace",
            case,
        )
        case.keys("Enter")
        case.wait_text("project scope:")

        case.type_submit("/projects")
        named_project_picker = case.wait_text("Choose a project scope")
        audit.check(
            "project-1" in normalized(named_project_picker),
            "project picker omitted the first named scope",
            case,
        )
        case.keys("Down", "Enter")
        case.wait_text("project scope: project-1")

        # The modal owns the wheel too: three notches from the first menu row
        # land on /yolo, so Enter should open its confirmation picker rather
        # than acting on whichever transcript row sits underneath the menu.
        case.mouse("right-down", 70, 5)
        case.wait_text("Commands")
        case.mouse("wheel-down", 70, 5)
        case.keys("Enter")
        case.wait_text("Confirm unrestricted access")
        case.keys("Escape")
        case.wait_absent("Confirm unrestricted access")

        case.mouse("right-down", 70, 5)
        menu = case.wait_text("Commands")
        for item in ("/providers", "/approval", "/access", "/yolo", "/pin", "/frame", "/new", "/help"):
            audit.check(item in normalized(menu), f"command menu omitted {item}", case)
        case.keys("C-b")
        audit.check("WORKSPACE" in normalized(case.capture()), "menu leaked Ctrl-B", case)
        case.click_text("/help")
        help_frame = case.wait_text("Tab completes a half-typed command")
        audit.check("Commands" not in normalized(help_frame), "menu click did not close overlay", case)
    finally:
        case.close()


def scrolled_picker_at_low_height(audit: Audit, case: Case) -> None:
    audit.begin("scrolled-picker-at-low-height")
    case.launch()
    try:
        # Nine saved sessions make the picker taller than an 80x10 terminal.
        # The newest session is selected first; moving down to the oldest one
        # forces the overlay to scroll before the mouse is allowed to choose a
        # row that is only visible after that scroll.
        for index in range(9):
            case.prompt(f"picker-session-{index}")
            if index < 8:
                case.command("/new", "new session ready")

        case.type_submit("/sessions")
        case.wait_text("Open a saved session")
        # Crossterm receives one key event per loop turn. Pacing this burst
        # keeps tmux from presenting eight arrows while the first repaint is
        # still pending, which would make the probe stop at the first row.
        for _ in range(8):
            case.keys("Down")
            time.sleep(0.08)
        frame = case.wait(
            lambda lines: len(lines) > 8
            and "picker-session-0" in lines[8]
            and "›" in lines[8],
            8,
            "scrolled session picker rows",
        )
        # At 80x10 the modal fills the frame vertically, so its first content
        # row is screen row 1. Restricting the locator to that row avoids the
        # same title in the context rail or sidebar becoming the click target.
        first_row = frame[1]
        match = re.search(r"picker-session-\d+", first_row)
        audit.check(match is not None, "scrolled picker painted no first row", case)
        first_visible = match.group(0) if match else ""
        audit.check(
            first_visible == "picker-session-7",
            "picker did not move its visible window after selection moved",
            case,
        )

        case.click(first_row.index(first_visible), 1)
        opened = case.wait_text(f"offline echo: {first_visible}", 12)
        audit.check(
            f"offline echo: {first_visible}" in normalized(opened),
            "mouse chose a different session than the visible row",
            case,
        )
    finally:
        case.close()


def approval_resolution_journey(
    audit: Audit,
    binary: Path,
    approval_home: Path,
    denial_home: Path,
) -> None:
    audit.begin("provider-backed-approval-resolution-and-denial")
    server = http.server.ThreadingHTTPServer(
        ("127.0.0.1", 0), ApprovalFixtureHandler
    )
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    environment = {
        "OPTIMUS_API_BASE": f"http://127.0.0.1:{server.server_address[1]}/v1",
        "OPTIMUS_API_KEY": "tui-feature-fixture",
        "OPTIMUS_MODEL": "tui-approval-fixture",
    }
    try:
        approve = Case(
            binary,
            approval_home,
            cols=40,
            rows=20,
            environment=environment,
        )
        approve.launch()
        try:
            approve.command(
                "/provider open-ai-compat",
                "provider is now open-ai-compat",
            )
            approve.type_submit("write the approval proof")
            approval = approve.wait_text("Approval required", 20)
            audit.check(
                "Write appro" in normalized(approval)
                and "bytes)" in normalized(approval),
                "approval picker omitted the bounded effect summary",
                approve,
            )
            audit.check(
                "! approval" in normalized(approval),
                "approval footer did not show that it is waiting on a decision",
                approve,
            )
            audit.check(
                all(len(line) <= approve.cols for line in approval),
                "approval modal overflowed the narrow terminal",
                approve,
            )
            # The decision must remain actionable if the terminal closes while
            # the human is considering it. Escaping the picker leaves the
            # pending job untouched; /quit simulates an ordinary app exit.
            approve.keys("Escape")
            approve.type_submit("/quit")
            approve.wait_exit()
        finally:
            approve.close()

        resumed = Case(
            binary,
            approval_home,
            cols=40,
            rows=20,
            environment=environment,
        )
        resumed.launch(allow_approval=True)
        try:
            restored = resumed.wait_text("Approval required", 20)
            restored_text = normalized(restored)
            audit.check(
                "write_file" in restored_text and "[approval]" in restored_text,
                "relaunch did not restore the blocked tool row",
                resumed,
            )
            audit.check(
                "approval-call-1" not in restored_text,
                "relaunch leaked the model's raw tool-call envelope",
                resumed,
            )
            audit.check(
                "! approval" in restored_text,
                "relaunch did not restore the approval footer",
                resumed,
            )
            audit.check(
                all(len(line) <= resumed.cols for line in restored),
                "restored approval modal overflowed the narrow terminal",
                resumed,
            )
            resumed.click_text("Approve and continue")
            settled = resumed.wait(
                lambda lines: "approved continuation" in normalized(lines)
                and "turn · ready" in normalized(lines),
                30,
                "approved continuation settled",
            )
            audit.check(
                "approved — running the exact" in normalized(settled)
                and "action…" in normalized(settled),
                "approval decision was not recorded in the transcript",
                resumed,
            )
            audit.check(
                "turn · ready" in normalized(settled),
                "approved continuation did not return the workbench to ready",
                resumed,
            )
            proof = approval_home / "workspace" / "approval-proof.txt"
            resumed.wait(lambda _lines: proof.is_file(), 10, "approved file")
            resumed.command("/approval", "no approval is pending")
        finally:
            resumed.close()

        deny = Case(
            binary,
            denial_home,
            cols=40,
            rows=20,
            environment=environment,
        )
        deny.launch()
        try:
            deny.command(
                "/provider open-ai-compat",
                "provider is now open-ai-compat",
            )
            deny.type_submit("write the denial proof")
            deny.wait_text("Approval required", 20)
            deny.keys("Down", "Enter")
            denied = deny.wait(
                lambda lines: "denied continuation" in normalized(lines)
                and "turn · ready" in normalized(lines),
                30,
                "denied continuation settled",
            )
            audit.check(
                "denied — it will not" in normalized(denied),
                "denial decision was not recorded in the transcript",
                deny,
            )
            audit.check(
                "turn · ready" in normalized(denied),
                "denied continuation did not return the workbench to ready",
                deny,
            )
            audit.check(
                not (denial_home / "workspace" / "denial-proof.txt").exists(),
                "denied effect changed the workspace",
                deny,
            )
        finally:
            deny.close()
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=2)


def approval_continuation_journey(
    audit: Audit, binary: Path, home: Path
) -> None:
    audit.begin("second-approval-continuation-keeps-the-new-card-actionable")
    ApprovalContinuationFixtureHandler.requests = 0
    server = http.server.ThreadingHTTPServer(
        ("127.0.0.1", 0), ApprovalContinuationFixtureHandler
    )
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    environment = {
        "OPTIMUS_API_BASE": f"http://127.0.0.1:{server.server_address[1]}/v1",
        "OPTIMUS_API_KEY": "tui-approval-continuation-fixture",
        "OPTIMUS_MODEL": "tui-approval-continuation-fixture",
    }
    try:
        case = Case(binary, home, cols=52, rows=22, environment=environment)
        case.launch()
        try:
            case.command(
                "/provider open-ai-compat",
                "provider is now open-ai-compat",
            )
            case.type_submit("perform the two approved writes")
            first = case.wait(
                lambda lines: "Approval required" in normalized(lines)
                and "Approve and continue" in normalized(lines)
                and "first-approval-pr" in normalized(lines),
                20,
                "first approval picker",
            )
            audit.check(
                "Approval required" in normalized(first)
                and "! approval" in normalized(first),
                "first approval did not present an actionable picker",
                case,
            )
            case.click_text("Approve and continue")

            second = case.wait(
                lambda lines: "Approval required" in normalized(lines)
                and "second-approval-pr" in normalized(lines)
                and "Approve and continue" in normalized(lines)
                and "! approval" in normalized(lines),
                30,
                "second approval from a resumed turn",
            )
            audit.check(
                "second-approval-pr" in normalized(second),
                "continuation lost the second approval summary",
                case,
            )
            audit.check(
                all(len(line) <= case.cols for line in second),
                "second approval overflowed the narrow terminal",
                case,
            )
            case.click_text("Approve and continue")
            settled = case.wait(
                lambda lines: "both approval continuations completed" in normalized(lines)
                and "turn · ready" in normalized(lines),
                30,
                "second approval continuation settled",
            )
            audit.check(
                "approved — running the exact" in normalized(settled),
                "second approval decision was not recorded",
                case,
            )
            audit.check(
                (home / "workspace" / "first-approval-proof.txt").is_file()
                and (home / "workspace" / "second-approval-proof.txt").is_file(),
                "one of the two approved effects did not run",
                case,
            )
            case.command("/approval", "no approval is pending")
        finally:
            case.close()
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=2)


def completed_tool_relaunch_journey(audit: Audit, binary: Path, home: Path) -> None:
    audit.begin("completed-tool-relaunch-keeps-polished-lifecycle")
    CompletedToolFixtureHandler.requests = 0
    server = http.server.ThreadingHTTPServer(
        ("127.0.0.1", 0), CompletedToolFixtureHandler
    )
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    environment = {
        "OPTIMUS_API_BASE": f"http://127.0.0.1:{server.server_address[1]}/v1",
        "OPTIMUS_API_KEY": "completed-tool-fixture",
        "OPTIMUS_MODEL": "tui-completed-tool",
    }
    binary = binary.resolve()
    try:
        first = Case(binary, home, cols=80, rows=28, environment=environment)
        first.launch()
        try:
            first.command(
                "/provider open-ai-compat",
                "provider is now open-ai-compat",
            )
            first.type_submit("show the workspace")
            first.wait(
                lambda lines: "directory listing came back clean" in normalized(lines)
                and "turn · ready" in normalized(lines),
                30,
                "completed tool turn settled",
            )
            first.keys("C-c")
            first.wait_exit()
        finally:
            first.close()

        resumed = Case(binary, home, cols=80, rows=28, environment=environment)
        resumed.launch()
        try:
            restored = resumed.capture()
            restored_text = normalized(restored)
            audit.check(
                "list_dir" in restored_text and "[done]" in restored_text,
                "relaunch lost the completed tool lifecycle row",
                resumed,
            )
            audit.check(
                "list-call-1" not in restored_text,
                "relaunch leaked the completed model tool-call envelope",
                resumed,
            )
            audit.check(
                '"version":1' not in restored_text and '"call_id"' not in restored_text,
                "relaunch leaked the raw tool receipt payload",
                resumed,
            )
            audit.check(
                "directory listing came back clean" in restored_text,
                "relaunch lost the assistant answer after the tool result",
                resumed,
            )
            audit.check(
                all(len(line) <= resumed.cols for line in restored),
                "completed tool relaunch overflowed the terminal",
                resumed,
            )
        finally:
            resumed.close()
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=2)


def reused_call_id_relaunch_journey(audit: Audit, binary: Path, home: Path) -> None:
    audit.begin("reused-tool-call-id-keeps-each-run-distinct")
    ReusedCallFixtureHandler.requests = 0
    workspace = home / "workspace"
    workspace.mkdir(parents=True, exist_ok=True)
    (workspace / "alpha.txt").write_text("alpha", encoding="utf-8")
    (workspace / "beta.txt").write_text("beta", encoding="utf-8")
    server = http.server.ThreadingHTTPServer(
        ("127.0.0.1", 0), ReusedCallFixtureHandler
    )
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    environment = {
        "OPTIMUS_API_BASE": f"http://127.0.0.1:{server.server_address[1]}/v1",
        "OPTIMUS_API_KEY": "reused-call-fixture",
        "OPTIMUS_MODEL": "tui-reused-call",
    }
    try:
        first = Case(binary, home, cols=100, rows=34, environment=environment)
        first.launch()
        try:
            first.command(
                "/provider open-ai-compat",
                "provider is now open-ai-compat",
            )
            first.type_submit("read the alpha file")
            first.wait_text("The alpha file was read", 30)
            first.type_submit("read the beta file")
            first.wait_text("The beta file was read", 30)
            first.keys("Escape")
            first.type_submit("/quit")
            first.wait_exit()
        finally:
            first.close()

        resumed = Case(binary, home, cols=100, rows=34, environment=environment)
        resumed.launch()
        try:
            restored = resumed.capture()
            restored_text = normalized(restored)
            audit.check(
                "path=alpha.txt" in restored_text
                and "path=beta.txt" in restored_text,
                "relaunch merged repeated provider call ids into one outcome",
                resumed,
            )
            audit.check(
                "call_1" not in restored_text,
                "relaunch exposed the reused raw provider call id",
                resumed,
            )
            audit.check(
                "The alpha file was read" in restored_text
                and "The beta file was read" in restored_text,
                "relaunch lost one answer beside the repeated tool id",
                resumed,
            )
            audit.check(
                all(len(line) <= resumed.cols for line in restored),
                "reused-call relaunch overflowed the terminal",
                resumed,
            )
        finally:
            resumed.close()
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=2)


def pending_repeated_call_relaunch_journey(
    audit: Audit, binary: Path, home: Path
) -> None:
    audit.begin("pending-call-reuse-keeps-the-completed-occurrence-visible")
    PendingRepeatedCallFixtureHandler.requests = 0
    workspace = home / "workspace"
    workspace.mkdir(parents=True, exist_ok=True)
    (workspace / "alpha.txt").write_text("alpha", encoding="utf-8")
    server = http.server.ThreadingHTTPServer(
        ("127.0.0.1", 0), PendingRepeatedCallFixtureHandler
    )
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    environment = {
        "OPTIMUS_API_BASE": f"http://127.0.0.1:{server.server_address[1]}/v1",
        "OPTIMUS_API_KEY": "pending-repeated-call-fixture",
        "OPTIMUS_MODEL": "tui-pending-repeated-call",
    }
    try:
        first = Case(binary, home, cols=100, rows=34, environment=environment)
        first.launch()
        try:
            first.command(
                "/provider open-ai-compat",
                "provider is now open-ai-compat",
            )
            first.type_submit("read alpha first")
            first.wait_text("The completed read alpha", 30)
            first.type_submit("write the pending proof")
            first.wait_text("Approval required", 30)
            first.keys("Escape")
            first.type_submit("/quit")
            first.wait_exit()
        finally:
            first.close()

        resumed = Case(binary, home, cols=100, rows=34, environment=environment)
        resumed.launch(allow_approval=True)
        try:
            restored = resumed.wait_text("Approval required", 20)
            restored_text = normalized(restored)
            audit.check(
                "path=alpha.txt" in restored_text and "[done]" in restored_text,
                "relaunch hid the completed occurrence behind the pending reused call id",
                resumed,
            )
            audit.check(
                "write_file" in restored_text and "[approval]" in restored_text,
                "relaunch lost the pending occurrence beside the completed one",
                resumed,
            )
            audit.check(
                "call_1" not in restored_text,
                "pending-call relaunch exposed the raw provider call id",
                resumed,
            )
            audit.check(
                all(len(line) <= resumed.cols for line in restored),
                "pending-call relaunch overflowed the terminal",
                resumed,
            )
        finally:
            resumed.close()
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=2)


def pending_approval_session_switch_journey(
    audit: Audit, binary: Path, home: Path
) -> None:
    audit.begin("pending-approval-survives-new-session-and-session-switch")
    PendingRepeatedCallFixtureHandler.requests = 0
    workspace = home / "workspace"
    workspace.mkdir(parents=True, exist_ok=True)
    (workspace / "alpha.txt").write_text("alpha", encoding="utf-8")
    server = http.server.ThreadingHTTPServer(
        ("127.0.0.1", 0), PendingRepeatedCallFixtureHandler
    )
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    environment = {
        "OPTIMUS_API_BASE": f"http://127.0.0.1:{server.server_address[1]}/v1",
        "OPTIMUS_API_KEY": "pending-session-switch-fixture",
        "OPTIMUS_MODEL": "tui-pending-session-switch",
    }
    try:
        case = Case(binary, home, cols=100, rows=34, environment=environment)
        case.launch()
        try:
            case.command(
                "/provider open-ai-compat",
                "provider is now open-ai-compat",
            )
            case.type_submit("read alpha first")
            case.wait_text("The completed read alpha", 30)
            case.type_submit("write the pending proof")
            case.wait_text("Approval required", 30)
            case.keys("Escape")
            case.command("/new", "new session ready")

            picker = case.command("/sessions", "Open a saved session")
            audit.check(
                "read alpha first" in normalized(picker),
                "session picker omitted the parked session",
                case,
            )
            case.keys("Enter")
            restored = case.wait_text("Approval required", 20)
            restored_text = normalized(restored)
            audit.check(
                "path=alpha.txt" in restored_text
                and "write_file" in restored_text
                and "[approval]" in restored_text,
                "switching back did not restore history beside the pending card",
                case,
            )
            audit.check(
                "Approve and continue" in restored_text
                and "! approval" in restored_text,
                "switched-back approval was not actionable",
                case,
            )
            audit.check(
                all(len(line) <= case.cols for line in restored),
                "switched-back approval overflowed the terminal",
                case,
            )
            case.click_text("Approve and continue")
            settled = case.wait(
                lambda lines: "pending proof was approved" in normalized(lines)
                and "turn · ready" in normalized(lines),
                30,
                "switched-back approval resolution",
            )
            audit.check(
                (home / "workspace" / "pending-proof.txt").is_file(),
                "approval restored after a session switch did not run the effect",
                case,
            )
            audit.check(
                "turn · ready" in normalized(settled),
                "switched-back approval did not return to ready",
                case,
            )
        finally:
            case.close()
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=2)


def editor_and_history(audit: Audit, case: Case) -> None:
    audit.begin("composer-editing-unicode-paste-and-history")
    case.launch()
    try:
        case.literal("ac")
        case.keys("Left")
        case.literal("b")
        case.keys("Home")
        case.literal("[")
        case.keys("End")
        case.literal("]")
        case.keys("Left", "DC", "BSpace")
        case.literal("c]")
        case.submit_draft("[abc]")
        case.wait_text("offline echo: [abc]")
        audit.check(case.alive(), "cursor/edit sequence exited", case)

        case.literal("middle")
        case.keys("C-a")
        case.literal("start-")
        case.keys("C-e")
        case.literal("-end")
        case.submit_draft("start-middle-end")
        case.wait_text("offline echo: start-middle-end")

        case.literal("discard-this")
        case.keys("C-u")
        case.literal("ctrl-u-ok")
        case.submit_draft("ctrl-u-ok")
        case.wait_text("offline echo: ctrl-u-ok")

        case.literal("keep remove")
        case.keys("C-a", "M-f", "C-k")
        case.literal("-ctrl-k-ok")
        case.submit_draft("keep-ctrl-k-ok")
        case.wait_text("offline echo: keep-ctrl-k-ok")

        case.literal("keep remove")
        case.keys("C-w")
        case.literal("ctrl-w-ok")
        case.submit_draft("keep ctrl-w-ok")
        case.wait_text("offline echo: keep ctrl-w-ok")

        case.literal("keep remove")
        case.keys("M-BSpace")
        case.literal("alt-backspace-ok")
        case.submit_draft("keep alt-backspace-ok")
        case.wait_text("offline echo: keep alt-backspace-ok")

        case.literal("one three")
        case.keys("M-Left")
        case.literal("two ")
        case.keys("M-Right")
        case.literal("!")
        case.submit_draft("one two three!")
        case.wait_text("offline echo: one two three!")

        case.literal("a👩‍👩‍👦b")
        case.keys("Left", "BSpace")
        case.submit_draft("ab")
        case.wait_text("offline echo: ab")

        case.literal("line one")
        case.keys("C-j")
        case.literal("line two")
        case.submit_draft("line two")
        case.wait(
            lambda lines: "offline echo: line one" in normalized(lines)
            and sum("line two" in line for line in lines) >= 2,
            20,
            "Ctrl-J multiline submit",
        )

        # tmux itself rewrites carriage returns before Crossterm receives the
        # bracketed payload; CRLF normalization is therefore covered by the
        # Composer unit contract, while this real pty proves multiline paste.
        case.paste("paste one\npaste two\npaste three")
        pasted = case.wait(
            lambda lines: all(
                text in normalized(lines)
                for text in ("paste one", "paste two", "paste three")
            ),
            8,
            "bracketed multiline paste",
        )
        audit.check(
            "offline echo: paste one" not in normalized(pasted),
            "bracketed paste submitted its first line",
            case,
        )
        case.submit_draft("paste three")
        case.wait(
            lambda lines: "offline echo: paste one" in normalized(lines)
            and sum("paste two" in line for line in lines) >= 2
            and sum("paste three" in line for line in lines) >= 2,
            20,
            "multiline paste submit",
        )

        case.literal("escape-me")
        case.wait(lambda lines: "escape-me" in composer_text(lines), 5, "draft paint")
        case.keys("Escape")
        cleared = case.wait(
            lambda lines: "escape-me" not in composer_text(lines),
            5,
            "Escape draft clear",
        )
        audit.check(case.alive(), "Escape exited with a draft", case)
        audit.check("Ask Optimus anything" in composer_text(cleared), "placeholder not restored", case)

        case.keys("C-l")
        audit.check(case.alive(), "Ctrl-L redraw exited", case)
        case.wait_text("· ready")

        case.literal("ax")
        case.keys("Home", "C-d")
        case.submit_draft("x")
        case.wait_text("offline echo: x")

        case.prompt("history-one")
        case.prompt("history-two")
        case.literal("history-draft")
        case.wait(
            lambda lines: "history-draft" in composer_text(lines),
            5,
            "history parked draft paint",
        )
        case.keys("Up")
        case.wait(
            lambda lines: "history-two" in composer_text(lines),
            5,
            "history older once",
        )
        case.keys("Up")
        case.wait(
            lambda lines: "history-one" in composer_text(lines),
            5,
            "history older twice",
        )
        case.keys("Down")
        case.wait(
            lambda lines: "history-two" in composer_text(lines),
            5,
            "history newer once",
        )
        case.keys("Down")
        restored = case.wait(
            lambda lines: "history-draft" in composer_text(lines),
            5,
            "history restored parked draft",
        )
        audit.check(
            "history-draft" in composer_text(restored),
            "history discarded the in-progress draft",
            case,
        )
        case.submit_draft("history-draft")
        case.wait_text("offline echo: history-draft")
    finally:
        case.close()


def streaming_and_cancel(audit: Audit, case: Case) -> None:
    audit.begin("streaming-spinner-busy-draft-and-cancellation")
    case.launch()
    try:
        case.type_submit("cancel this deliberately")
        busy = case.wait(busy_visible, 8, "busy state visible")
        audit.check(any(BRAILLE.intersection(line) for line in busy), "spinner missing", case)

        first = next(
            (character for line in busy for character in line if character in BRAILLE),
            None,
        )
        changed = False
        for _ in range(12):
            time.sleep(0.08)
            frame = case.capture()
            current = next(
                (character for line in frame for character in line if character in BRAILLE),
                None,
            )
            if current is not None and current != first:
                changed = True
                break
        audit.check(changed, "spinner did not animate", case)

        case.literal("draft survives cancellation")
        case.submit_draft("draft survives cancellation", expect_accept=False)
        kept = case.wait(
            lambda lines: "draft survives cancellation" in composer_text(lines),
            5,
            "busy draft stayed in composer",
        )
        audit.check(
            "offline echo: draft survives cancellation" not in normalized(kept),
            "Enter submitted a second simultaneous turn",
            case,
        )
        case.keys("C-c")
        cancelled = case.wait(
            lambda lines: "turn cancelled" in normalized(lines)
            and "· ready" in normalized(lines),
            8,
            "cancel settlement",
        )
        audit.check(case.alive(), "Ctrl-C killed the app while busy", case)
        audit.check(
            "draft survives cancellation" in composer_text(cancelled),
            "cancellation lost the parked draft",
            case,
        )
        case.submit_draft("draft survives cancellation")
        case.wait(
            lambda lines: "offline echo: draft survives cancellation"
            in normalized(lines)
            and "· ready" in normalized(lines),
            35,
            "parked draft after cancellation",
        )

        case.type_submit("busy session guard")
        case.wait(busy_visible, 8, "busy session guard turn is busy")
        case.mouse("left-down", 4, 3)
        guard = case.wait_text("stop the current turn before starting a new session", 8)
        audit.check(
            "Ctrl-C to interrupt" in normalized(guard),
            "busy guard stopped the turn",
            case,
        )
        case.keys("C-c")
        case.wait_text("turn cancelled", 8)
    finally:
        case.close()


def scroll_and_inspect(audit: Audit, case: Case) -> None:
    audit.begin("transcript-scroll-wheel-hover-and-inspect")
    case.launch()
    try:
        for index in range(12):
            case.prompt(f"scroll-{index:02d}")
        tail = case.capture()
        audit.check("scroll-11" in main_text(tail), "tail omitted newest turn", case)
        audit.check("scroll-00" not in main_text(tail), "fixture did not overflow", case)

        before = main_text(tail)
        case.keys("PPage", "PPage", "PPage")
        paged = case.wait(
            lambda lines: main_text(lines) != before and "scroll-00" in main_text(lines),
            8,
            "PageUp reached transcript head",
        )
        audit.check("scroll-00" in main_text(paged), "PageUp missed oldest row", case)
        case.keys("NPage")
        moved_down = case.wait(
            lambda lines: main_text(lines) != main_text(paged),
            5,
            "PageDown moved toward tail",
        )
        audit.check(main_text(moved_down) != main_text(paged), "PageDown was inert", case)
        case.keys("End")
        case.wait(lambda lines: "scroll-11" in main_text(lines), 5, "End followed tail")

        wheel_tail = main_text(case.capture())
        for _ in range(6):
            case.mouse("wheel-up", 70, 8)
        wheeled = case.wait(
            lambda lines: main_text(lines) != wheel_tail,
            5,
            "mouse wheel scrolled transcript",
        )
        audit.check(main_text(wheeled) != wheel_tail, "wheel-up was inert", case)
        for _ in range(10):
            case.mouse("wheel-down", 70, 8)
        case.wait(lambda lines: "scroll-11" in main_text(lines), 5, "wheel returned to tail")

        for _ in range(30):
            case.mouse("wheel-up", 70, 8)
        case.wait(lambda lines: "scroll-00" in main_text(lines), 5, "wheel reached head")

        plain_before_hover = normalized(case.settled_capture())
        case.mouse("move", 70, 7)
        hovered = normalized(case.settled_capture())
        audit.check(
            hovered == plain_before_hover,
            "hover changed terminal text or layout",
            case,
        )

        case.keys("Tab")
        case.wait_text("Inspect ·")
        case.literal("gignoredjGk")
        case.keys("Space", "Enter", "PPage", "NPage")
        inspected = case.wait_text("Inspect ·")
        audit.check(
            "gignoredjGk" not in composer_text(inspected),
            "inspect keys leaked into composer",
            case,
        )
        case.keys("Escape")
        case.wait_absent("Inspect ·")
        case.literal("composer owns keys again")
        typed_again = case.wait(
            lambda lines: "composer owns keys again" in composer_text(lines),
            5,
            "composer focus typing",
        )
        audit.check(
            "composer owns keys again" in composer_text(typed_again),
            "Escape did not return focus to composer",
            case,
        )
        case.keys("Escape")

        frame = case.capture()
        target_row = next(
            index for index, line in enumerate(frame) if "scroll-11" in line
        )
        case.click(70, target_row)
        case.wait_text("Inspect ·")
        audit.check(case.alive(), "transcript click exited", case)
    finally:
        case.close()


def sidebar_and_persistence(audit: Audit, case: Case) -> None:
    audit.begin("sidebar-sessions-projects-pins-resize-and-persistence")
    seed_projects(case.home)
    case.launch()
    try:
        def sidebar_window(heading: str, item: str | None = None) -> list[str]:
            return case.wait(
                lambda lines: heading in normalized(lines)
                and (item is None or item in normalized(lines)),
                12,
                f"sidebar window {heading!r} {item!r}",
            )

        initial = case.capture()
        for label in ("WORKSPACE", "New session", "SESSIONS", "PROJECTS", "PINNED"):
            audit.check(label in normalized(initial), f"sidebar omitted {label}", case)
        audit.check("5 projects" in normalized(initial), "project count omitted scopes", case)

        case.click_text("PROJECTS", sidebar_only=True)
        projects = sidebar_window("PROJECTS 1–2/5", "project-1")
        audit.check(case.home.name in normalized(projects), "workspace project missing", case)
        audit.check("project-1" in normalized(projects), "first named project missing", case)
        case.mouse("wheel-down", 4, 10)
        overflow = sidebar_window("PROJECTS 4–5/5", "project-4")
        audit.check("project-4" in normalized(overflow), "last project inaccessible", case)
        case.click_text("project-4", sidebar_only=True)
        filtered = case.wait(
            lambda lines: "project scope: project-4" in normalized(lines)
            and any(
                "project-4" in line and "auto" in line
                for line in lines
            ),
            12,
            "active project scope did not reach the context rail",
        )
        audit.check(
            any(
                "project-4" in line and "auto" in line
                for line in filtered
            ),
            "active project scope missing from context rail",
            case,
        )
        case.keys("C-b")
        collapsed_scope = case.wait_absent("WORKSPACE")
        audit.check(
            any(
                "project-4" in line and "auto" in line
                for line in collapsed_scope
            ),
            "active project scope disappeared with the sidebar",
            case,
        )
        case.keys("C-b")
        case.wait_text("WORKSPACE")
        case.prompt("project-four-session")
        case.command("/pin", "session pinned")
        case.type_submit("/sessions")
        saved_picker = case.wait_text("Open a saved session")
        audit.check(
            "project-four-session" in normalized(saved_picker),
            "session picker omitted current session",
            case,
        )
        case.keys("Escape")
        case.type_submit("/sessions project-four")
        filtered_picker = case.wait_text("Saved sessions · project-four")
        audit.check(
            "project-four-session" in normalized(filtered_picker),
            "session query omitted the matching saved session",
            case,
        )
        case.keys("Escape")
        case.type_submit("/sessions no-such-session")
        no_match = case.wait_text("no saved sessions matching `no-such-session`")
        audit.check(
            "Open a saved session" not in normalized(no_match),
            "empty session query opened a blank picker",
            case,
        )
        case.type_submit("/pinned")
        pinned_picker = case.wait_text("Open a pinned session")
        audit.check(
            "project-four-session" in normalized(pinned_picker),
            "pinned picker omitted current session",
            case,
        )
        case.keys("Escape")
        case.type_submit("/pinned project-four")
        filtered_pinned = case.wait_text("Pinned sessions · project-four")
        audit.check(
            "project-four-session" in normalized(filtered_pinned),
            "pinned session query omitted the matching session",
            case,
        )
        case.keys("Escape")

        case.command("/new", "new session ready")
        case.click_text("PROJECTS", sidebar_only=True)
        case.mouse("wheel-up", 4, 10)
        sidebar_window("PROJECTS 1–2/5", case.home.name)
        case.click_text(case.home.name, sidebar_only=True)
        case.wait_text(f"project scope: {case.home.name}")
        case.prompt("workspace-session-one")
        case.command("/pin", "session pinned")

        case.type_submit("/sessions")
        session_picker = case.wait_text("Open a saved session")
        audit.check(
            "project-four-session" in normalized(session_picker),
            "session picker omitted the earlier session",
            case,
        )
        case.keys("Up", "Enter")
        case.wait_text("offline echo: project-four-session")
        case.type_submit("/pinned")
        case.wait_text("Open a pinned session")
        case.keys("Down", "Enter")
        case.wait_text("offline echo: workspace-session-one")

        for index in range(2, 7):
            case.command("/new", "new session ready")
            case.prompt(f"matrix-session-{index}")
            if index <= 3:
                case.command("/pin", "session pinned")

        sessions = case.capture()
        audit.check("matrix-session-6" in normalized(sessions), "active session hidden", case)
        case.click_text("SESSIONS", sidebar_only=True)
        sidebar_window("SESSIONS 1–3/7", "matrix-session-3")
        for _ in range(4):
            case.mouse("wheel-up", 4, 6)
        head = sidebar_window("SESSIONS 1–3/7", "matrix-session-3")
        audit.check("matrix-session-3" in normalized(head), "newest pin missing", case)
        for _ in range(4):
            case.mouse("wheel-down", 4, 6)
        tail = sidebar_window("SESSIONS 5–7/7", "matrix-session-4")
        audit.check("matrix-session-4" in normalized(tail), "oldest window inaccessible", case)
        case.click_text("matrix-session-4", sidebar_only=True)
        case.wait_text("offline echo: matrix-session-4")

        case.click_text("New session", sidebar_only=True)
        case.wait_text("new session ready")
        case.click_text("PINNED", sidebar_only=True)
        pinned = sidebar_window("PINNED 1–3/4", "matrix-session-3")
        audit.check("matrix-session-3" in normalized(pinned), "newest pin missing", case)
        case.mouse("wheel-down", 4, 13)
        last_pins = sidebar_window("PINNED 2–4/4", "project-four-session")
        audit.check("project-four-session" in normalized(last_pins), "oldest pin inaccessible", case)
        case.click_text("project-four-session", sidebar_only=True)
        case.wait_text("offline echo: project-four-session")
        case.command("/pin", "session unpinned")
        case.command("/pin", "session pinned")

        case.click_text("PROJECTS", sidebar_only=True)
        case.mouse("wheel-up", 4, 10)
        sidebar_window("PROJECTS 1–2/5", case.home.name)
        case.click_text(case.home.name, sidebar_only=True)
        refused = case.wait_text("start a new session before changing its project scope")
        audit.check("project-four-session" in normalized(refused), "scope refusal lost session", case)

        case.click_text("New session", sidebar_only=True)
        case.wait_text("new session ready")
        case.click_text("PROJECTS", sidebar_only=True)
        for _ in range(2):
            case.mouse("wheel-down", 4, 10)
        sidebar_window("PROJECTS 4–5/5", "project-4")
        case.click_text("project-4", sidebar_only=True)
        filtered = case.wait_text("project scope: project-4")
        audit.check("matrix-session-6" not in normalized(filtered), "project filter leaked sessions", case)
        audit.check("project-four-session" in normalized(filtered), "project filter hid its session", case)
        case.click_text("project-four-session", sidebar_only=True)
        case.wait_text("offline echo: project-four-session")

        case.keys("C-b")
        collapsed = case.wait_absent("WORKSPACE")
        audit.check(collapsed[0].startswith("›"), "Ctrl-B collapse left no reopen tab", case)
        case.keys("C-b")
        case.wait_text("WORKSPACE")

        case.click_text("collapse", sidebar_only=True)
        closed_by_click = case.wait_absent("WORKSPACE")
        audit.check(closed_by_click[0].startswith("›"), "collapse click left no tab", case)
        case.click(0, 0)
        case.wait_text("WORKSPACE")

        frame = case.capture()
        divider_row = next(index for index, line in enumerate(frame) if "┊" in line)
        divider = frame[divider_row].index("┊")
        case.mouse("left-down", divider, divider_row)
        case.mouse("drag", divider + 6, divider_row)
        resized = case.wait(
            lambda lines: any(
                "┊" in line and line.index("┊") == divider + 6 for line in lines
            ),
            5,
            "sidebar divider resize",
        )
        audit.check(
            any("┊" in line and line.index("┊") == divider + 6 for line in resized),
            "divider drag did not enlarge sidebar",
            case,
        )
        case.mouse("left-up", divider + 6, divider_row)

        case.mouse("left-down", divider + 6, divider_row)
        case.mouse("drag", 6, divider_row)
        dragged_closed = case.wait_absent("WORKSPACE")
        audit.check(dragged_closed[0].startswith("›"), "far-left drag did not close", case)
        case.mouse("left-up", 6, divider_row)
        case.click(0, 0)
        case.wait_text("WORKSPACE")

        case.click_text("New session", sidebar_only=True)
        case.wait_text("new session ready")
        case.click_text("PROJECTS", sidebar_only=True)
        case.mouse("wheel-up", 4, 10)
        sidebar_window("PROJECTS 1–2/5", case.home.name)
        case.click_text(case.home.name, sidebar_only=True)
        case.wait_text(f"project scope: {case.home.name}")

        case.command("/provider offline", "provider is now offline")
        case.command("/model offline-scripted", "model is now offline-scripted")
        case.command("/thinking ultra", "thinking is now ultra")
        case.prompt("persistence-session")
        case.type_submit("/quit")
        case.wait_exit()

        case.launch()
        restored = case.wait_text("offline echo: persistence-session", 15)
        status = normalized(restored)
        for choice in ("offline/offline-scripted", "think:ultra"):
            audit.check(choice in status, f"relaunch forgot {choice}", case)
        audit.check("persistence-session" in status, "latest session was not restored", case)
        case.keys("Up")
        case.wait(lambda lines: "/quit" in composer_text(lines), 5, "persisted command history")
        case.keys("Up")
        case.wait(
            lambda lines: "persistence-session" in composer_text(lines),
            5,
            "persisted composer recall",
        )
        case.keys("Escape")
    finally:
        case.close()


def responsive_sidebar_reopen(audit: Audit, case: Case) -> None:
    audit.begin("responsive-sidebar-reopen-at-width-threshold")
    case.launch()
    try:
        narrow = case.capture()
        audit.check(
            "WORKSPACE" not in normalized(narrow),
            "narrow rail should collapse into its gutter tab",
            case,
        )

        # The rail is still logically open at this width. Clicking the visible
        # tab must not turn that preference off before the user widens the
        # terminal again.
        case.click(0, 0)
        resized = tmux(
            "resize-window",
            "-t",
            case.session,
            "-x",
            "80",
            "-y",
            str(case.rows),
        )
        audit.check(resized.returncode == 0, "responsive resize failed", case)
        reopened = case.wait_text("WORKSPACE")
        audit.check("WORKSPACE" in normalized(reopened), "hidden rail did not reappear", case)

        case.click_text("collapse", sidebar_only=True)
        closed = case.wait_absent("WORKSPACE")
        audit.check(closed[0].startswith("›"), "closed rail left no reopen tab", case)
        case.click(0, 0)
        audit.check(
            "WORKSPACE" in normalized(case.wait_text("WORKSPACE")),
            "closed rail tab did not reopen the rail",
            case,
        )
    finally:
        case.close()


def compact_sidebar_at_low_height(audit: Audit, case: Case) -> None:
    audit.begin("compact-sidebar-at-low-height")
    seed_projects(case.home)
    case.launch()
    try:
        initial = case.capture()
        for label in ("SESSIONS", "PROJECTS", "PINNED"):
            audit.check(
                label in normalized(initial),
                f"low-height sidebar omitted {label}",
                case,
            )

        case.click_text("PROJECTS", sidebar_only=True)
        projects = case.wait(
            lambda lines: "PROJECTS 1–2/5" in normalized(lines)
            and "project-1" in normalized(lines),
            12,
            "low-height project rows",
        )
        audit.check(
            "project-1" in normalized(projects),
            "low-height project row was unreachable",
            case,
        )
        case.click_text("project-1", sidebar_only=True)
        case.wait_text("project scope: project-1")

        case.click_text("PINNED", sidebar_only=True)
        pinned = case.wait_text("PINNED")
        audit.check(
            "SESSIONS" in normalized(pinned)
            and "PROJECTS" in normalized(pinned)
            and "PINNED" in normalized(pinned),
            "low-height section headings were not all retained",
            case,
        )
    finally:
        case.close()


def exit_routes(audit: Audit, binary: Path, root: Path) -> None:
    audit.begin("all-exit-routes")
    routes: tuple[tuple[str, Callable[[Case], None]], ...] = (
        ("slash-quit", lambda case: case.type_submit("/quit")),
        ("slash-exit-alias", lambda case: case.type_submit("/exit")),
        ("ctrl-d-empty", lambda case: case.keys("C-d")),
        ("ctrl-c-idle", lambda case: case.keys("C-c")),
    )
    for name, action in routes:
        home = root / f"exit-{name}"
        home.mkdir()
        case = Case(binary, home)
        case.launch()
        try:
            case.keys("Escape")
            audit.check(case.alive(), f"idle Escape exited before {name}", case)
            action(case)
            case.wait_exit()
            audit.check(not case.alive(), f"{name} left the process alive")
        finally:
            case.close()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--binary",
        type=Path,
        default=ROOT / "target" / "debug" / "optimus",
    )
    args = parser.parse_args()
    binary = args.binary.resolve()
    if not binary.is_file():
        raise SystemExit(f"TUI_FEATURE_MATRIX_FAIL: {binary} is not built")
    if tmux("-V").returncode != 0:
        raise SystemExit("TUI_FEATURE_MATRIX_FAIL: tmux is required")

    audit = Audit()
    print(
        "TUI_FEATURE_MATRIX_METHOD 2026-08-02 "
        "TestBackend+portable-pty/vt100+tmux-real-binary "
        f"sources={','.join(METHOD_SOURCES)}",
        flush=True,
    )
    try:
        with tempfile.TemporaryDirectory(prefix="optimus-tui-feature-") as scratch:
            root = Path(scratch)

            def fresh(name: str) -> Path:
                home = root / name
                home.mkdir()
                return home

            command_surface(audit, Case(binary, fresh("commands")))
            picker_and_menu(audit, Case(binary, fresh("pickers")))
            scrolled_picker_at_low_height(
                audit,
                Case(binary, fresh("scrolled-picker"), cols=80, rows=10),
            )
            approval_resolution_journey(
                audit,
                binary,
                fresh("approval-resolution"),
                fresh("approval-denial"),
            )
            approval_continuation_journey(
                audit,
                binary,
                fresh("approval-continuation"),
            )
            completed_tool_relaunch_journey(
                audit,
                binary,
                fresh("completed-tool-relaunch"),
            )
            reused_call_id_relaunch_journey(
                audit,
                binary,
                fresh("reused-call-id-relaunch"),
            )
            pending_repeated_call_relaunch_journey(
                audit,
                binary,
                fresh("pending-repeated-call-relaunch"),
            )
            pending_approval_session_switch_journey(
                audit,
                binary,
                fresh("pending-approval-session-switch"),
            )
            editor_and_history(audit, Case(binary, fresh("editor")))
            streaming_and_cancel(
                audit,
                Case(
                    binary,
                    fresh("streaming"),
                    environment={"OPTIMUS_OFFLINE_LATENCY_MS": "3000"},
                ),
            )
            scroll_and_inspect(audit, Case(binary, fresh("scroll"), rows=24))
            sidebar_and_persistence(audit, Case(binary, fresh("sidebar")))
            responsive_sidebar_reopen(
                audit,
                Case(binary, fresh("responsive-sidebar"), cols=60, rows=20),
            )
            compact_sidebar_at_low_height(
                audit,
                Case(binary, fresh("compact-sidebar"), cols=100, rows=10),
            )
            exit_routes(audit, binary, root)
    except (FeatureFailure, StopIteration) as error:
        print(f"TUI_FEATURE_MATRIX_FAIL: {error}", file=os.sys.stderr)
        return 1

    print(f"TUI_FEATURE_MATRIX_OK cases={audit.cases} checks={audit.checks}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
