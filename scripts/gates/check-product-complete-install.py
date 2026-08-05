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

ROOT = Path(__file__).resolve().parents[2]


def fail(msg: str) -> None:
    print(f"PRODUCT_COMPLETE_INSTALL_FAIL: {msg}", file=sys.stderr)
    raise SystemExit(1)


def main() -> int:
    script = ROOT / "scripts" / "rebuild-install-relaunch.sh"
    if not script.is_file():
        fail("missing rebuild-install-relaunch.sh")
    text = script.read_text(encoding="utf-8", errors="replace")
    for token in (
        "react-tauri",
        "optimus-agent-tauri",
        "optimus-agent.desktop",
        "desktop_shell",
    ):
        if token not in text:
            fail(f"install script missing expected token: {token}")
    # Anti-resurgence: the installer is exclusively Tauri. Electron and the
    # retired Wry rollback shell must not creep back into the staged/rollback
    # paths (spec-001 R6/R7).
    for token in ("Electron", "LegacyWry", "OPTIMUS_DESKTOP_SHELL", "optimus-desktop-host"):
        if token in text:
            fail(
                f"install script must not reference {token} "
                "(Tauri is exclusive; Wry rollback is retired)"
            )

    adr = ROOT / "docs" / "decisions" / "0043-no-auto-updater-channel.md"
    if not adr.is_file():
        fail("missing ADR-0043 no auto-updater")

    install_doc = ROOT / "docs" / "runbooks" / "install-relaunch.md"
    if not install_doc.is_file():
        fail("missing desktop-install-relaunch.md")
    install_doc_text = install_doc.read_text(encoding="utf-8", errors="replace")
    if "Tauri + React" not in install_doc_text:
        fail("install doc must declare Tauri + React default")
    if "Electron" in install_doc_text:
        fail("install doc must not retain Electron as a rollback")

    # Optional: probe existing user install (read-only), respecting XDG_DATA_HOME
    # the same way rebuild-install-relaunch.sh does.
    data_home = Path(os.environ.get("XDG_DATA_HOME", Path.home() / ".local" / "share"))
    install_root = Path(
        os.environ.get("OPTIMUS_INSTALL_ROOT", data_home / "optimus-agent")
    )
    meta_path = install_root / "install-meta.json"
    desktop = data_home / "applications" / "optimus-agent.desktop"
    status = {
        "repo_script_ok": True,
        "adr_0043_ok": True,
        "install_present": meta_path.is_file(),
        "desktop_entry_present": desktop.is_file(),
        "data_home": str(data_home),
    }
    if meta_path.is_file():
        meta = json.loads(meta_path.read_text(encoding="utf-8"))
        status["install_version"] = meta.get("version")
        status["install_root"] = meta.get("install_root")
        status["desktop_shell"] = meta.get("desktop_shell")
        # Prefer path recorded in meta when present (absolute install-owned entry).
        meta_desktop = meta.get("desktop_entry")
        if isinstance(meta_desktop, str) and meta_desktop.strip():
            desktop = Path(meta_desktop)
            status["desktop_entry_present"] = desktop.is_file()
        if not desktop.is_file():
            fail("install-meta present but desktop entry missing")
        de = desktop.read_text(encoding="utf-8", errors="replace")
        if "optimus-desktop" not in de:
            fail("desktop entry does not reference optimus-desktop")
        shell = meta.get("desktop_shell")
        if shell and shell != "react-tauri":
            fail(f"install-meta desktop_shell must be react-tauri, got {shell!r}")
        if "X-Optimus-UI=react-tauri" not in de:
            fail("desktop entry does not evidence Tauri/React default shell")
        for token in ("ElectronRollback", "LegacyWry", "OPTIMUS_DESKTOP_SHELL"):
            if token in de:
                fail(f"desktop entry must not expose a rollback shell action ({token})")
        status["desktop_entry_ok"] = True
    print("PRODUCT_COMPLETE_INSTALL_OK", json.dumps(status, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
