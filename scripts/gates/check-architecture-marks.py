#!/usr/bin/env python3
"""Fail-closed architecture marks claim hygiene (P17).

If architecture-marks.md grades a dimension **S+++**, the corresponding program
phase must be marked done and required gate/evidence paths must exist.

This is intentionally lightweight — not a proof assistant. It blocks greenwashed
S+++ claims without re-running full hold suites.

Usage:
  python3 scripts/gates/check-architecture-marks.py
"""

from __future__ import annotations

import re
import sys
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MARKS = ROOT / "docs" / "runbooks" / "architecture-marks.md"
PROGRAM = ROOT / "docs" / "plans" / "s-plus-plus-plus-program.md"

# Grade cell in the current-grades table: | Name | **GRADE** | notes |
# Accept bold or plain grade tokens so unbolded S+++ cannot skip the gate.
GRADE_ROW = re.compile(
    r"^\|\s*(?P<name>[^|]+?)\s*\|\s*(?:\*\*(?P<grade_bold>[^*]+)\*\*|"
    r"(?P<grade_plain>S\+{0,3}|A\+?|B\+?|C))\s*\|",
    re.MULTILINE,
)
# Program phase table: | P17 | … | … | pending|**done** |
PHASE_ROW = re.compile(
    r"^\|\s*(?P<phase>P\d+|0|1|2|3|4|5)\s*\|\s*(?P<focus>[^|]+?)\s*\|"
    r"\s*(?P<marks>[^|]+?)\s*\|\s*(?P<status>[^|]+?)\s*\|",
    re.MULTILINE,
)
# "done" only at start of status (after bold strip). "not done" / "undone" ≠ done.
DONE_STATUS = re.compile(r"^done\b")


@dataclass(frozen=True)
class MarkRequirement:
    """What must hold when a mark is graded S+++."""

    name_prefixes: tuple[str, ...]
    phases_done: tuple[str, ...]
    required_paths: tuple[str, ...]
    # Optional verification docs (any one is enough when non-empty).
    verification_any: tuple[str, ...] = ()


# Requirements for each architecture mark that may claim S+++.
REQUIREMENTS: tuple[MarkRequirement, ...] = (
    MarkRequirement(
        name_prefixes=("Multi-agent readiness",),
        phases_done=("P10", "P12"),
        required_paths=(
            "crates/optimus-agent/src/lib.rs",
            "crates/optimus-workflow/src/lib.rs",
        ),
        verification_any=(
            "_attic/architecture-records/s-plus-plus-plus-p12-verification.md",
            "_attic/architecture-records/s-plus-plus-plus-p10-verification.md",
        ),
    ),
    MarkRequirement(
        name_prefixes=("Control-plane modularity",),
        phases_done=("P11",),
        required_paths=("scripts/gates/check-crate-layers.py",),
        verification_any=(
            "_attic/architecture-records/s-plus-plus-plus-p11-verification.md",
            "docs/decisions/0034-control-plane-crate-peels.md",
        ),
    ),
    MarkRequirement(
        name_prefixes=("Security boundary design",),
        phases_done=("P12",),
        required_paths=(
            "docs/decisions/0035-command-capability-envelope.md",
            "specs/004-runtime-effects/spec.md",
        ),
        verification_any=(
            "_attic/architecture-records/s-plus-plus-plus-p12-verification.md",
        ),
    ),
    MarkRequirement(
        name_prefixes=("Domain modularity",),
        phases_done=("P13",),
        required_paths=("scripts/gates/check-domain-modularity.py",),
        verification_any=(
            "_attic/architecture-records/s-plus-plus-plus-p13-verification.md",
        ),
    ),
    MarkRequirement(
        name_prefixes=("Observability",),
        phases_done=("P14",),
        required_paths=("scripts/gates/check-observability-gate.py",),
        verification_any=(
            "_attic/architecture-records/s-plus-plus-plus-p14-verification.md",
        ),
    ),
    MarkRequirement(
        name_prefixes=("UI architecture",),
        phases_done=("P15",),
        required_paths=("scripts/gates/check-desktop-ipc-matrix.py",),
        verification_any=(
            "_attic/architecture-records/s-plus-plus-plus-p15-verification.md",
        ),
    ),
    MarkRequirement(
        name_prefixes=("Doc / claim hygiene", "Doc hygiene"),
        phases_done=("P16",),
        required_paths=("scripts/tools/engineering_memory.py",),
        verification_any=(
            "_attic/architecture-records/s-plus-plus-plus-p16-verification.md",
        ),
    ),
    MarkRequirement(
        name_prefixes=("Release / parity", "Release / parity gating"),
        phases_done=("P17",),
        required_paths=(
            "scripts/tools/optimus_version.py",
            "scripts/gates/check-parity-ledger.py",
            "scripts/gates/check-architecture-marks.py",
            "docs/architecture.md",
        ),
        verification_any=(
            "_attic/architecture-records/s-plus-plus-plus-p17-verification.md",
        ),
    ),
    MarkRequirement(
        name_prefixes=("Durability",),
        phases_done=("P18",),
        required_paths=(
            "crates/optimus-runtime/tests/crash_resume.rs",
            "apps/optimus-cli/src/doctor.rs",
            "docs/architecture.md",
        ),
        verification_any=(
            "_attic/architecture-records/s-plus-plus-plus-p18-verification.md",
        ),
    ),
)


def parse_grades(text: str) -> dict[str, str]:
    """Return mark name → grade from the current-grades table."""
    # Restrict to the Current grades section to avoid grade-scale rows.
    section = text
    if "## Current grades" in text:
        section = text.split("## Current grades", 1)[1]
        if "\n## " in section:
            section = section.split("\n## ", 1)[0]
    grades: dict[str, str] = {}
    for match in GRADE_ROW.finditer(section):
        name = match.group("name").strip()
        grade = (match.group("grade_bold") or match.group("grade_plain") or "").strip()
        if name.lower() in {"mark", "grade"} or not grade:
            continue
        grades[name] = grade
    return grades


def parse_phase_status(text: str) -> dict[str, str]:
    """Return phase id → normalized status (done|pending|other)."""
    section = text
    if "## Program phases" in text:
        section = text.split("## Program phases", 1)[1]
        if "\n## " in section:
            section = section.split("\n## ", 1)[0]
    statuses: dict[str, str] = {}
    for match in PHASE_ROW.finditer(section):
        phase = match.group("phase").strip()
        raw = match.group("status").strip().lower().replace("*", "").strip()
        # Fail-closed: only leading "done" counts (not "not done" / "undone").
        if DONE_STATUS.match(raw):
            statuses[phase] = "done"
        elif "pending" in raw:
            statuses[phase] = "pending"
        else:
            statuses[phase] = raw
    return statuses


def match_requirement(mark_name: str) -> MarkRequirement | None:
    for req in REQUIREMENTS:
        for prefix in req.name_prefixes:
            if mark_name.startswith(prefix) or prefix in mark_name:
                return req
    return None


def is_s_plus_plus_plus(grade: str) -> bool:
    normalized = grade.replace(" ", "").upper()
    return normalized == "S+++" or normalized.startswith("S+++")


def check(
    marks_text: str,
    *,
    root: Path = ROOT,
    require_program_mentions: bool = True,
) -> list[str]:
    """Return human-readable findings (empty ⇒ OK)."""
    findings: list[str] = []
    grades = parse_grades(marks_text)
    if not grades:
        findings.append("could not parse any grades from architecture-marks.md")
        return findings

    phases = parse_phase_status(marks_text)

    for mark_name, grade in grades.items():
        if not is_s_plus_plus_plus(grade):
            continue
        req = match_requirement(mark_name)
        if req is None:
            findings.append(
                f"{mark_name}: graded S+++ but no requirement map is registered"
            )
            continue

        for phase in req.phases_done:
            status = phases.get(phase)
            if status is None:
                findings.append(
                    f"{mark_name}: S+++ requires phase {phase} in program table"
                )
            elif status != "done":
                findings.append(
                    f"{mark_name}: S+++ claimed but phase {phase} status is {status!r}"
                )

        for rel in req.required_paths:
            if not (root / rel).exists():
                findings.append(
                    f"{mark_name}: S+++ requires existing path {rel}"
                )

        if req.verification_any:
            if not any((root / rel).exists() for rel in req.verification_any):
                options = ", ".join(req.verification_any)
                findings.append(
                    f"{mark_name}: S+++ requires one of verification paths: {options}"
                )

    if require_program_mentions and PROGRAM.is_file():
        program = PROGRAM.read_text(encoding="utf-8")
        # Soft consistency: S+++ program plan should still list Release mark.
        if "Release / parity" not in program and "P17" not in program:
            findings.append("s-plus-plus-plus-program.md missing P17 / Release mark")

    return findings


def main() -> int:
    try:
        text = MARKS.read_text(encoding="utf-8")
    except OSError as exc:
        print(f"architecture-marks unreadable: {exc}", file=sys.stderr)
        return 1

    findings = check(text, root=ROOT)
    if findings:
        print("architecture marks check FAILED:", file=sys.stderr)
        for item in findings:
            print(f"  - {item}", file=sys.stderr)
        return 1
    print("architecture marks check OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
