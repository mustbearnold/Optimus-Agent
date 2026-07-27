#!/usr/bin/env python3
"""Fail-closed project-scope assertion counter (success criterion C2 in
docs/architecture/north-star-2026-07.md).

Every host method in crates/optimus-host/src/router.rs METHOD_DOMAINS carries
a third column: its project-scope assertion. `Some(ScopePolicy::Project)` and
`Some(ScopePolicy::Host)` are enforced by `scope::enforce` before dispatch
(declaring is load-bearing, never documentation); `None` means unasserted.

Rules:
  - Unasserted methods live on UNASSERTED_ALLOWLIST, seeded 2026-07-28 with
    all 82 registry methods. The allowlist may only shrink: asserting a method
    without deleting its entry fails as stale.
  - A method absent from the allowlist must be asserted — so a new registry
    method must declare its scope policy at birth.
  - The scope column must parse for every registry entry (format drift in the
    table breaks the counter loudly, not silently).

C2 is met when the allowlist is empty: 82/82 methods asserted.
Exit 0 on success; print counter summary and exit 1 on any violation.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ROUTER = ROOT / "crates/optimus-host/src/router.rs"

# Methods with no scope assertion yet. Seeded with the full registry; may only
# shrink — do NOT add entries. Each per-method conversion (declaring Project
# or Host in METHOD_DOMAINS) deletes its line here.
UNASSERTED_ALLOWLIST = frozenset(
    {
        "approvals_grant",
        "approvals_list",
        "approvals_release_yolo",
        "archive_session",
        "artifacts_delete",
        "artifacts_delete_many",
        "artifacts_export",
        "artifacts_export_zip",
        "artifacts_get",
        "artifacts_list",
        "artifacts_put_text",
        "auth_import_cli",
        "auth_import_hermes",
        "auth_status",
        "browser_click",
        "browser_navigate",
        "browser_reload",
        "campaign_create",
        "campaign_list",
        "campaign_run",
        "campaign_status",
        "chat",
        "chat_approval_resolve",
        "chat_offline",
        "commands_list",
        "cron_add",
        "cron_history",
        "cron_list",
        "cron_remove",
        "cron_set_enabled",
        "cron_tick",
        "delete_session",
        "doctor",
        "fs_list",
        "fs_read",
        "fs_roots",
        "gateway_ack_delivery",
        "gateway_ambiguous",
        "gateway_enqueue",
        "gateway_inbox",
        "gateway_outbox",
        "gateway_status",
        "gateway_telegram_status",
        "get_session",
        "jobs_list",
        "logs_tail",
        "mcp_status",
        "mcp_tools",
        "memory_correct",
        "memory_forget",
        "memory_list",
        "memory_recall",
        "new_session",
        "open_path",
        "open_url",
        "packs_activate",
        "packs_deactivate",
        "packs_state",
        "packs_verify_signed",
        "pick_folder",
        "pin_session",
        "ping",
        "project_root_stage_native",
        "project_scopes_authorize",
        "project_scopes_list",
        "providers_catalog",
        "providers_route_preview",
        "rename_session",
        "session_search",
        "sessions",
        "settings_get",
        "settings_set",
        "skills_deprecate",
        "skills_list",
        "skills_pin",
        "term_run",
        "window_close",
        "window_drag",
        "window_maximize",
        "window_minimize",
        "window_outer_position",
        "window_set_outer_position",
    }
)


def registry_block(text: str) -> str:
    block = re.search(r"const METHOD_DOMAINS:.*?= &\[(.*?)\];", text, re.DOTALL)
    if not block:
        raise SystemExit(f"cannot find METHOD_DOMAINS in {ROUTER}")
    return block.group(1)


def parse_scope_column(block: str) -> dict[str, str | None]:
    """Map method -> 'Project' | 'Host' | None (unasserted)."""
    entries = re.findall(
        r'\(\s*"([a-z0-9_]+)",\s*Domain::\w+,\s*'
        r"(None|Some\(ScopePolicy::(?:Project|Host)\))\s*,?\s*\)",
        block,
        re.DOTALL,
    )
    out: dict[str, str | None] = {}
    for name, policy in entries:
        out[name] = None if policy == "None" else policy[len("Some(ScopePolicy::") : -1]
    return out


def main() -> int:
    text = ROUTER.read_text(encoding="utf-8")
    block = registry_block(text)

    # The method universe, parsed the same way check-desktop-ipc-matrix.py
    # parses it — any entry this sees but parse_scope_column misses is a
    # scope-column format drift, not a smaller registry.
    universe = re.findall(r'\("([a-z0-9_]+)",\s*Domain::', block)
    scoped = parse_scope_column(block)

    errors: list[str] = []

    if len(universe) != len(set(universe)):
        errors.append("METHOD_DOMAINS has duplicate method names")
    unparsed = sorted(set(universe) - set(scoped))
    if unparsed:
        errors.append(
            "scope column failed to parse for registry entries (format drift): "
            + ", ".join(unparsed)
        )
    if not universe:
        errors.append("empty METHOD_DOMAINS parse")

    asserted = sorted(name for name, policy in scoped.items() if policy is not None)
    unasserted = sorted(name for name, policy in scoped.items() if policy is None)

    for name in asserted:
        if name in UNASSERTED_ALLOWLIST:
            errors.append(
                f"stale allowlist entry: {name} is asserted "
                f"({scoped[name]}) but still on UNASSERTED_ALLOWLIST — shrink it"
            )
    for name in unasserted:
        if name not in UNASSERTED_ALLOWLIST:
            errors.append(
                f"{name} has no scope assertion and is not allowlisted — "
                "new methods must declare Project or Host at birth"
            )
    for name in sorted(UNASSERTED_ALLOWLIST - set(universe)):
        errors.append(
            f"stale allowlist entry: {name} is not in METHOD_DOMAINS — "
            "shrink UNASSERTED_ALLOWLIST"
        )

    total = len(universe)
    print("PROJECT_SCOPE_ASSERTIONS")
    print(
        f"  asserted: {len(asserted)}/{total} methods carry a scope policy "
        f"(C2 target: {total}/{total})"
    )
    print(f"  unasserted allowlist: {len(UNASSERTED_ALLOWLIST)} (shrink-only)")

    if errors:
        print("PROJECT_SCOPE_FAIL", file=sys.stderr)
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        return 1
    print("PROJECT_SCOPE_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
