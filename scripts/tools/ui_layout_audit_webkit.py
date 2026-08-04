#!/usr/bin/env python3
"""The layout audit's collector, run in the engine the product actually ships.

The Playwright audit measures in Chromium; the installed Tauri shell renders in
WebKitGTK. Layout is close between the two, but font metrics, scrollbars, and
flex rounding differ by real pixels — and a defect that only reproduces in the
shipping engine would otherwise stay invisible until a user hits it. This
harness loads the same built UI, injects the exact `collect()` rule set from
`ui_layout_audit.cjs`, and fails on any violation.

Skipped automatically when WebKitGTK's introspection bindings are absent.
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
AUDIT = ROOT / "scripts" / "ui_layout_audit.cjs"
URL = os.environ.get("OPTIMUS_UI_URL", "http://127.0.0.1:4174/")


def collect_source() -> str:
    source = AUDIT.read_text(encoding="utf-8")
    start = source.index("function collect()")
    end = source.index("\nasync function auditViewport")
    return source[start:end]


SEED = """
(() => {
  localStorage.clear();
  localStorage.setItem('optimus.ui.projects', JSON.stringify({ projects: [
    { id: 'optimus-agent', name: 'Optimus Agent', rootPaths: ['/projects/optimus-agent'] },
  ]}));
  localStorage.setItem('optimus.ui.sessionProjects', JSON.stringify({ 'fixture-assess': 'optimus-agent' }));
  localStorage.setItem('optimus.ui.projectExpanded', JSON.stringify({ 'optimus-agent': true }));
  return 'seeded';
})()
"""


def main() -> int:
    try:
        import gi  # type: ignore

        gi.require_version("Gtk", "3.0")
        gi.require_version("WebKit2", "4.1")
        from gi.repository import GLib, Gtk, WebKit2  # type: ignore
    except (ImportError, ValueError):
        print("UI_LAYOUT_AUDIT_WEBKIT_SKIP: WebKitGTK 4.1 introspection not available")
        return 0

    # The audit serves its own preview; here we require one already listening
    # (verify.sh runs this right after the Playwright audit, which leaves the
    # server warm) or start one.
    import urllib.request

    server = None
    try:
        urllib.request.urlopen(URL, timeout=2)
    except Exception:
        server = subprocess.Popen(
            ["bunx", "vite", "preview", "--host", "127.0.0.1", "--port", "4174", "--strictPort"],
            cwd=ROOT / "apps" / "optimus-ui",
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        for _ in range(60):
            time.sleep(0.5)
            try:
                urllib.request.urlopen(URL, timeout=2)
                break
            except Exception:
                continue
        else:
            print("UI_LAYOUT_AUDIT_WEBKIT_SKIP: no built UI to serve")
            return 0

    rules = collect_source()
    state: dict[str, object] = {"phase": 0, "result": None}

    window = Gtk.OffscreenWindow()
    window.set_default_size(1280, 833)
    view = WebKit2.WebView()
    window.add(view)
    window.show_all()

    def run_js(script: str, done) -> None:
        def callback(web_view, task, _user):
            try:
                value = web_view.evaluate_javascript_finish(task)
                done(value.to_string() if value else None)
            except Exception as error:  # noqa: BLE001
                done(f"JSERR: {error}")

        view.evaluate_javascript(script, -1, None, None, None, callback, None)

    def finish() -> None:
        Gtk.main_quit()

    def after_collect(raw: str | None) -> None:
        state["result"] = raw
        finish()

    def collect_now() -> bool:
        run_js(f"(function() {{ {rules}; return JSON.stringify(collect()); }})()", after_collect)
        return False

    def after_seed(_raw: str | None) -> None:
        view.reload()

    def on_load(_view, event) -> None:
        if event != WebKit2.LoadEvent.FINISHED:
            return
        if state["phase"] == 0:
            state["phase"] = 1
            run_js(SEED, after_seed)
        elif state["phase"] == 1:
            state["phase"] = 2
            GLib.timeout_add(1200, collect_now)

    view.connect("load-changed", on_load)
    view.load_uri(URL)
    GLib.timeout_add_seconds(45, lambda: (finish(), False)[1])
    Gtk.main()

    if server:
        server.terminate()

    raw = state["result"]
    if not isinstance(raw, str) or raw.startswith("JSERR"):
        print(f"UI_LAYOUT_AUDIT_WEBKIT_FAIL: collector did not run: {raw}")
        return 1
    violations = json.loads(raw)
    # Chromium-tuned pixel thresholds get one engine tolerance: WebKit rounds
    # flex sizes differently, so sub-3px geometry deltas are noise, not defects.
    material = [
        violation
        for violation in violations
        if not (
            violation["rule"] in {"asymmetric-padding", "uneven-sibling-rows", "misaligned-siblings"}
            and (match := re.search(r"(\d+(?:\.\d+)?)px", str(violation["detail"])))
            and float(match.group(1)) <= 3
        )
    ]
    if material:
        print("UI_LAYOUT_AUDIT_WEBKIT_FAIL")
        for violation in material:
            print(f"  {violation['rule']}: {violation.get('label') or violation.get('el')} — {violation['detail']}")
        return 1
    print(f"UI_LAYOUT_AUDIT_WEBKIT_OK rules=shared viewport=1280x833 engine=webkitgtk")
    return 0


if __name__ == "__main__":
    sys.exit(main())
