#!/usr/bin/env python3
"""Fail-closed packaging honesty checks for program P29 (no live install required).

Validates repository packaging contracts and optionally probes a user install
under XDG if present. Does not perform install/relaunch.
"""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def fail(msg: str) -> None:
    print(f"PRODUCT_COMPLETE_INSTALL_FAIL: {msg}", file=sys.stderr)
    raise SystemExit(1)


def main() -> int:
    script = ROOT / "scripts" / "rebuild-install-relaunch.sh"
    if not script.is_file():
        fail("missing rebuild-install-relaunch.sh")
    text = script.read_text(encoding="utf-8", errors="replace")
    for token in (
        "react-electron",
        "optimus-agent.desktop",
        "Electron",
        "desktop_shell",
    ):
        if token not in text:
            fail(f"install script missing expected token: {token}")

    adr = ROOT / "docs" / "decisions" / "0043-no-auto-updater-channel.md"
    if not adr.is_file():
        fail("missing ADR-0043 no auto-updater")

    install_doc = ROOT / "docs" / "architecture" / "desktop-install-relaunch.md"
    if not install_doc.is_file():
        fail("missing desktop-install-relaunch.md")
    if "Electron + React" not in install_doc.read_text(encoding="utf-8", errors="replace"):
        fail("install doc must declare Electron + React default")

    # Optional: probe existing user install (read-only)
    data_home = Path(os.environ.get("XDG_DATA_HOME", Path.home() / ".local" / "share"))
    install_root = data_home / "optimus-agent"
    meta_path = install_root / "install-meta.json"
    desktop = Path.home() / ".local" / "share" / "applications" / "optimus-agent.desktop"
    status = {
        "repo_script_ok": True,
        "adr_0043_ok": True,
        "install_present": meta_path.is_file(),
        "desktop_entry_present": desktop.is_file(),
    }
    if meta_path.is_file():
        meta = json.loads(meta_path.read_text(encoding="utf-8"))
        status["install_version"] = meta.get("version")
        status["install_root"] = meta.get("install_root")
        # Desktop entry should exist when install meta does
        if not desktop.is_file():
            fail("install-meta present but desktop entry missing")
        de = desktop.read_text(encoding="utf-8", errors="replace")
        if "optimus-desktop" not in de:
            fail("desktop entry does not reference optimus-desktop")
        status["desktop_entry_ok"] = True
    print("PRODUCT_COMPLETE_INSTALL_OK", json.dumps(status, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
