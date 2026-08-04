#!/usr/bin/env python3
"""Fail closed if a second tool catalog or plane-confused auth appears.

P13 domain modularity gate (merge-adjacent):
  - optimus-packs owns ToolDesc / ToolId
  - kernel must not define a parallel ToolDesc struct or ad-hoc catalog vec
  - no surface invents grant-from-session / grant-from-em APIs

Exit 0 when clean; exit 1 with findings.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

# Forbidden patterns in kernel/surfaces (second catalog / plane confusion).
FORBIDDEN: list[tuple[str, re.Pattern[str], str]] = [
    (
        "crates/optimus-kernel/src",
        re.compile(r"struct\s+ToolDesc\b"),
        "kernel must not define ToolDesc; use optimus_packs::ToolDesc",
    ),
    (
        "crates/optimus-kernel/src",
        re.compile(r"struct\s+ToolCatalog\b"),
        "kernel must not own a ToolCatalog type",
    ),
    (
        "crates/optimus-kernel/src",
        re.compile(r"fn\s+grant_from_session\b"),
        "session plane must not authorize host effects",
    ),
    (
        "crates/optimus-kernel/src",
        re.compile(r"fn\s+grant_from_engineering_memory\b|fn\s+grant_from_em\b"),
        "Engineering Memory must not authorize host effects",
    ),
    (
        "crates/optimus-store/src",
        re.compile(r"struct\s+ChatMessage\b|CREATE TABLE.*chat_messages", re.I),
        "store must not own chat UI schema",
    ),
]

REQUIRED_PACKS_EXPORT = re.compile(r"pub struct ToolDesc\b")
REQUIRED_KERNEL_USE = re.compile(
    r"use optimus_packs::\{[^}]*ToolId|pub use optimus_packs::ToolDesc as ToolSchema"
)


def scan_dir(rel: str, pattern: re.Pattern[str]) -> list[tuple[Path, int, str]]:
    base = ROOT / rel
    hits: list[tuple[Path, int, str]] = []
    if not base.exists():
        return hits
    for path in base.rglob("*"):
        if path.suffix not in {".rs", ".sql"}:
            continue
        if "target" in path.parts:
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except OSError:
            continue
        for i, line in enumerate(text.splitlines(), 1):
            if pattern.search(line):
                hits.append((path.relative_to(ROOT), i, line.strip()))
    return hits


def main() -> int:
    findings: list[str] = []

    packs_lib = ROOT / "crates/optimus-packs/src/lib.rs"
    if not packs_lib.is_file() or not REQUIRED_PACKS_EXPORT.search(packs_lib.read_text()):
        findings.append("optimus-packs must export pub struct ToolDesc")

    kernel_lib = ROOT / "crates/optimus-kernel/src/lib.rs"
    if kernel_lib.is_file():
        text = kernel_lib.read_text()
        if not REQUIRED_KERNEL_USE.search(text) and "ToolDesc as ToolSchema" not in text:
            findings.append(
                "kernel must re-export or use optimus_packs::ToolDesc (no local catalog)"
            )
        # Surfaces: forbid free-standing local tool schema registries in apps.
    for rel in ("apps/optimus-cli/src", "apps/optimus-desktop/src"):
        for path, line_no, line in scan_dir(rel, re.compile(r"struct\s+ToolDesc\b|struct\s+ToolCatalog\b")):
            findings.append(f"{path}:{line_no}: surface second catalog :: {line}")

    for rel, pattern, msg in FORBIDDEN:
        for path, line_no, line in scan_dir(rel, pattern):
            findings.append(f"{path}:{line_no}: {msg} :: {line}")

    if findings:
        print("domain modularity check FAILED:", file=sys.stderr)
        for f in findings:
            print(f"  - {f}", file=sys.stderr)
        return 1
    print("domain modularity check OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
