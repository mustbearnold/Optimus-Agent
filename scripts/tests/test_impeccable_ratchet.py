#!/usr/bin/env python3
"""Offline self-test for check-impeccable-ratchet.py's ratchet core.

The gate's value is its direction: findings may only shrink, and the pin must
not drift silently. The detector itself is exercised by `just verify`; here we
hold the ratchet semantics against known-good and known-broken finding sets
without invoking npx.
"""

from __future__ import annotations

import importlib.util
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parent.parent
GATES = SCRIPTS / "gates"

spec = importlib.util.spec_from_file_location(
    "check_impeccable_ratchet", GATES / "check-impeccable-ratchet.py"
)
assert spec is not None and spec.loader is not None
gate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gate)


def baseline(version: str, findings: list[dict]) -> dict:
    return {"version": version, "findings": findings}


def test_ratchet_accepts_known_findings():
    current = {("ui/styles.css", "overused-font")}
    known = baseline("3.5.0", [{"file": "ui/styles.css", "antipattern": "overused-font"}])
    code, _lines = gate.ratchet(current, known)
    assert code == 0


def test_ratchet_fails_new_findings():
    current = {("ui/styles.css", "overused-font"), ("ui/app.tsx", "layout-transition")}
    known = baseline("3.5.0", [{"file": "ui/styles.css", "antipattern": "overused-font"}])
    code, lines = gate.ratchet(current, known)
    assert code == 1
    assert any("app.tsx" in line for line in lines)


def test_ratchet_reports_retired_findings():
    current: set[tuple[str, str]] = set()
    known = baseline("3.5.0", [{"file": "ui/styles.css", "antipattern": "overused-font"}])
    code, lines = gate.ratchet(current, known)
    assert code == 0
    assert any("retired" in line for line in lines)


def test_ratchet_fails_on_version_drift():
    current: set[tuple[str, str]] = set()
    stale = baseline("3.4.0", [])
    code, lines = gate.ratchet(current, stale)
    assert code == 1
    assert any("3.4.0" in line for line in lines)
