#!/usr/bin/env python3
"""Fail-closed desktop IPC contract matrix for the Tauri desktop shell.

Compares:
  1. Rust host registry methods — crates/optimus-host/src/router.rs
  2. React DesktopMethod union — apps/optimus-ui/src/ipc/contracts.ts
     (the renderer surface; the Tauri bridge forwards every method to the
     host, so the typed transport is the renderer's only declared surface)

Rules:
  - React DesktopMethod set ⊆ Rust registry (typed transport cannot invent).
  - Critical product paths must exist on the renderer surface and in the Rust
    registry (or be explicitly documented as non-invoke channels: chat
    stream, window, pick_folder).
  - Main-only methods (project_root_stage_native) must NOT appear in the
    React renderer surface.
  - Every Rust registry method is either renderer-callable or a documented
    non-invoke channel (no silent host methods).

Exit 0 on success; print matrix summary and exit 1 on any violation.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

# Methods the renderer is allowed to invoke through the Tauri host bridge.
# Chat streaming and window/folder OS affordances use dedicated Tauri commands.
# P15: critical product paths must stay on the renderer surface (U1/U2).
CRITICAL_INVOKE_METHODS = frozenset(
    {
        "ping",
        "doctor",
        "sessions",
        "new_session",
        "get_session",
        "delete_session",
        "rename_session",
        "session_search",
        "archive_session",
        "pin_session",
        "chat_approval_resolve",
        "project_scopes_list",
        "project_scopes_authorize",
        "approvals_list",
        "approvals_grant",
        "fs_roots",
        "fs_list",
        "fs_read",
        "settings_get",
        "settings_set",
        "term_run",
        "jobs_list",
    }
)

# Host registry methods that must never be callable from the React allowlist.
# Tag: main_only — requires native selection / host-only authority.
MAIN_ONLY_METHODS = frozenset({"project_root_stage_native"})

# Host methods intentionally not exposed through the React DesktopMethod
# surface (dedicated Tauri commands / host-only channels).
# Tag: non_invoke — dedicated tauri command or OS channel, not host_invoke.
# U1: every Rust METHOD_DOMAINS entry must be renderer-callable OR listed here
# (no silent host methods).
HOST_NON_INVOKE_CHANNELS = frozenset(
    {
        "chat",  # non_invoke: SSE chat_stream via preload chat.start
        "chat_offline",  # non_invoke: offline path via chat channel
        "window_minimize",  # non_invoke: window chrome
        "window_maximize",
        "window_close",
        "window_drag",
        "window_outer_position",
        "window_set_outer_position",
        "pick_folder",  # non_invoke: optimus:pick-folder
        "open_path",  # non_invoke: optimus:open-path
        "open_url",  # non_invoke: optimus:open-url
        "project_root_stage_native",  # main_only: native_selection_token
    }
)


def parse_rust_registry(path: Path) -> list[str]:
    text = path.read_text(encoding="utf-8")
    block = re.search(
        r"const METHOD_DOMAINS:.*?= &\[(.*?)\];",
        text,
        re.DOTALL,
    )
    if not block:
        raise SystemExit(f"cannot find METHOD_DOMAINS in {path}")
    methods = re.findall(r'\(\s*"([a-z0-9_]+)",\s*Domain::', block.group(1))
    if not methods:
        raise SystemExit(f"empty METHOD_DOMAINS parse in {path}")
    return methods


def parse_react_desktop_methods(path: Path) -> list[str]:
    text = path.read_text(encoding="utf-8")
    block = re.search(
        r"export type DesktopMethod\s*=\s*(.*?);",
        text,
        re.DOTALL,
    )
    if not block:
        raise SystemExit(f"cannot find DesktopMethod in {path}")
    methods = re.findall(r"'([a-z0-9_]+)'", block.group(1))
    if not methods:
        raise SystemExit(f"empty DesktopMethod parse in {path}")
    return methods


def main() -> int:
    rust_path = ROOT / "crates/optimus-host/src/router.rs"
    react_path = ROOT / "apps/optimus-ui/src/ipc/contracts.ts"

    rust = parse_rust_registry(rust_path)
    react = parse_react_desktop_methods(react_path)

    rust_set = set(rust)
    react_set = set(react)

    errors: list[str] = []

    if len(rust) != len(rust_set):
        errors.append("Rust METHOD_DOMAINS has duplicate method names")
    if len(react) != len(react_set):
        errors.append("React DesktopMethod has duplicates")

    unknown_react = sorted(react_set - rust_set)
    if unknown_react:
        errors.append(
            "React DesktopMethod methods missing from Rust registry: "
            + ", ".join(unknown_react)
        )

    leaked_main_only = sorted(react_set & MAIN_ONLY_METHODS)
    if leaked_main_only:
        errors.append(
            "Main-only methods exposed to renderer surface: "
            + ", ".join(leaked_main_only)
        )

    missing_critical = sorted(CRITICAL_INVOKE_METHODS - react_set)
    if missing_critical:
        errors.append(
            "Critical invoke methods missing from React surface: "
            + ", ".join(missing_critical)
        )

    missing_critical_rust = sorted(CRITICAL_INVOKE_METHODS - rust_set)
    if missing_critical_rust:
        errors.append(
            "Critical invoke methods missing from Rust registry: "
            + ", ".join(missing_critical_rust)
        )

    # Host methods that are neither renderer-callable nor documented non-invoke.
    uncovered = sorted(rust_set - react_set - HOST_NON_INVOKE_CHANNELS)
    if uncovered:
        errors.append(
            "Rust registry methods neither renderer-callable nor documented "
            "non-invoke channels: " + ", ".join(uncovered)
        )

    # main_only must be documented as non-invoke (never renderer-callable).
    if MAIN_ONLY_METHODS - HOST_NON_INVOKE_CHANNELS:
        errors.append(
            "MAIN_ONLY_METHODS must also appear in HOST_NON_INVOKE_CHANNELS: "
            + ", ".join(sorted(MAIN_ONLY_METHODS - HOST_NON_INVOKE_CHANNELS))
        )
    # Phantom non-invoke tags (not in host registry) are documentation drift.
    phantom_non_invoke = sorted(HOST_NON_INVOKE_CHANNELS - rust_set)
    if phantom_non_invoke:
        errors.append(
            "HOST_NON_INVOKE_CHANNELS entries missing from Rust registry: "
            + ", ".join(phantom_non_invoke)
        )

    print("DESKTOP_IPC_MATRIX")
    print(f"rust_registry={len(rust_set)} react_surface={len(react_set)}")
    print(f"critical_invoke={len(CRITICAL_INVOKE_METHODS)} main_only={len(MAIN_ONLY_METHODS)}")
    print(f"non_invoke_channels={len(HOST_NON_INVOKE_CHANNELS)}")
    print("default_shell=tauri_react")
    print("legacy_shell=wry_optional")
    print("coverage=host_methods_all_classified")
    print("critical:")
    for method in sorted(CRITICAL_INVOKE_METHODS):
        print(f"  {method}")

    if errors:
        print("DESKTOP_IPC_MATRIX_FAILED", file=sys.stderr)
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        return 1

    print("DESKTOP_IPC_MATRIX_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
