#!/usr/bin/env python3
"""Fail-closed dependency layer rules for Optimus control-plane peels (P11).

Rules:
  - optimus-eval may depend on optimus-kernel; kernel must not depend on eval
  - optimus-ops must not depend on optimus-kernel
  - optimus-agent must not depend on optimus-workflow, optimus-kernel, or eval
  - optimus-workflow may depend on optimus-agent + optimus-artifacts
  - optimus-artifacts must not depend on kernel/agent/workflow/eval
  - optimus-browser must not depend on optimus-kernel
  - optimus-engineering must not depend on kernel/eval/agent/workflow/ops/runtime/host
  - no peeled crate may depend on apps/*

Apps layer rule (#65, success criterion C5 in north-star-2026-07.md):
  An app in apps/ may NAME core types but may not CONSTRUCT OR OPEN core
  state — opening a session, a runtime, or a store is the host's job, reached
  through `optimus_host::handle_ipc` or a pub host function. Enforced as a
  grep for the core-state constructors over apps/**/*.rs, with a shrinking
  allowlist seeded with the 6 sites measured in #65. Each conversion deletes
  its entry; an entry the code no longer needs fails the gate until deleted
  (the ratchet only tightens). Accepted limit per #65: this is a grep — it
  defends against drift, not against an adversary.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CRATES = ROOT / "crates"
APPS = ROOT / "apps"

# Core-state constructors no app may call. Extend when a new way to open core
# state appears; do NOT extend the allowlist below except by shrinking it.
BANNED_CONSTRUCTORS = (
    "Kernel::open_session",
    "Runtime::open",
    "Runtime::open_with_config",
    "CampaignStore::open",
)

# (app-relative file, constructor) -> allowed occurrence count.
# Seeded 2026-07-28 from #65's measured violation table; may only shrink.
APPS_ALLOWLIST: dict[tuple[str, str], int] = {
    # chat-offline (main.rs)
    ("optimus-cli/src/main.rs", "Kernel::open_session"): 1,
    # jobs / resume / resume-all
    ("optimus-cli/src/runtime_open.rs", "Runtime::open"): 1,
    ("optimus-cli/src/runtime_open.rs", "Runtime::open_with_config"): 1,
    # campaign
    ("optimus-cli/src/main.rs", "CampaignStore::open"): 1,
}

SKIP_DIR_NAMES = {"node_modules", "target", "dist", "build", "e2e", "test-results"}


def app_rust_files() -> list[Path]:
    out: list[Path] = []
    for path in sorted(APPS.rglob("*.rs")):
        if any(part in SKIP_DIR_NAMES for part in path.parts):
            continue
        out.append(path)
    return out


def check_apps_layer(errors: list[str]) -> dict[tuple[str, str], int]:
    counts: dict[tuple[str, str], int] = {}
    for path in app_rust_files():
        rel = path.relative_to(APPS).as_posix()
        text = path.read_text(encoding="utf-8")
        for line in text.splitlines():
            stripped = line.strip()
            if stripped.startswith("//"):
                continue
            for ctor in BANNED_CONSTRUCTORS:
                # Require a call site; `Runtime::open` must not match
                # `Runtime::open_with_config`.
                if re.search(rf"\b{re.escape(ctor)}\s*\(", stripped):
                    counts[(rel, ctor)] = counts.get((rel, ctor), 0) + 1
    for key, found in sorted(counts.items()):
        allowed = APPS_ALLOWLIST.get(key, 0)
        if found > allowed:
            rel, ctor = key
            errors.append(
                f"apps/{rel}: {found} call(s) to {ctor}, allowlist permits {allowed} — "
                "apps may not open core state; go through optimus-host"
            )
    for key, allowed in sorted(APPS_ALLOWLIST.items()):
        found = counts.get(key, 0)
        if found < allowed:
            rel, ctor = key
            errors.append(
                f"stale allowlist entry: apps/{rel} has {found} call(s) to {ctor} "
                f"but the allowlist still permits {allowed} — shrink APPS_ALLOWLIST"
            )
    return counts


def deps_of(crate: str) -> set[str]:
    cargo = CRATES / crate / "Cargo.toml"
    if not cargo.is_file():
        raise SystemExit(f"missing {cargo}")
    text = cargo.read_text(encoding="utf-8")
    found: set[str] = set()
    for match in re.finditer(r"^(optimus-[\w-]+)(?:\.workspace)?\s*=", text, re.M):
        found.add(match.group(1))
    for match in re.finditer(r'path\s*=\s*"\.\./\.\./crates/(optimus-[\w-]+)"', text):
        found.add(match.group(1))
    for match in re.finditer(r'path\s*=\s*"\.\./(optimus-[\w-]+)"', text):
        found.add(match.group(1))
    return found


def main() -> int:
    errors: list[str] = []

    def forbid(crate: str, banned: set[str]) -> None:
        have = deps_of(crate)
        bad = have & banned
        if bad:
            errors.append(f"{crate} must not depend on {sorted(bad)}; has {sorted(have)}")

    forbid("optimus-kernel", {"optimus-eval"})
    forbid("optimus-ops", {"optimus-kernel", "optimus-eval", "optimus-agent", "optimus-workflow"})
    forbid(
        "optimus-agent",
        {
            "optimus-kernel",
            "optimus-eval",
            "optimus-workflow",
            "optimus-ops",
            "optimus-artifacts",
        },
    )
    forbid(
        "optimus-artifacts",
        {
            "optimus-kernel",
            "optimus-eval",
            "optimus-agent",
            "optimus-workflow",
            "optimus-ops",
            "optimus-runtime",
            "optimus-graph",
        },
    )
    forbid(
        "optimus-workflow",
        {"optimus-kernel", "optimus-eval", "optimus-ops"},
    )
    forbid(
        "optimus-browser",
        {
            "optimus-kernel",
            "optimus-eval",
            "optimus-agent",
            "optimus-workflow",
            "optimus-artifacts",
            "optimus-ops",
        },
    )
    forbid("optimus-eval", {"optimus-ops"})  # eval may use kernel only among control peels
    # optimus-engineering (ADR-0052) owns the development-task state machine and
    # the per-run worktree. It sits above optimus-policy and below the kernel:
    # it must never reach up into the control plane, or an engineering run could
    # grant itself authority the broker never issued.
    forbid(
        "optimus-engineering",
        {
            "optimus-kernel",
            "optimus-eval",
            "optimus-agent",
            "optimus-workflow",
            "optimus-ops",
            "optimus-runtime",
            "optimus-host",
        },
    )

    check_apps_layer(errors)

    # Required edges that define the peel graph
    agent_deps = deps_of("optimus-agent")
    if "optimus-runtime" not in agent_deps or "optimus-packs" not in agent_deps:
        errors.append("optimus-agent must depend on optimus-runtime and optimus-packs")
    workflow_deps = deps_of("optimus-workflow")
    for need in ("optimus-agent", "optimus-artifacts", "optimus-runtime"):
        if need not in workflow_deps:
            errors.append(f"optimus-workflow must depend on {need}")
    kernel_deps = deps_of("optimus-kernel")
    for need in (
        "optimus-agent",
        "optimus-workflow",
        "optimus-artifacts",
        "optimus-ops",
        "optimus-runtime",
    ):
        if need not in kernel_deps:
            errors.append(f"optimus-kernel must depend on {need}")

    if errors:
        print("CRATE_LAYER_FAIL")
        for err in errors:
            print(f"  - {err}")
        return 1
    print("CRATE_LAYER_OK")
    print(f"  optimus-agent -> {sorted(deps_of('optimus-agent'))}")
    print(f"  optimus-workflow -> {sorted(deps_of('optimus-workflow'))}")
    print(f"  optimus-artifacts -> {sorted(deps_of('optimus-artifacts'))}")
    print(f"  optimus-kernel -> {sorted(deps_of('optimus-kernel'))}")
    remaining = sum(APPS_ALLOWLIST.values())
    print(
        f"  apps-layer: {remaining} allowlisted core-state call(s) remaining "
        f"across {len(APPS_ALLOWLIST)} allowlist entries (C5 target: 0)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
