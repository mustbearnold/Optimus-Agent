#!/usr/bin/env python3
"""Enforced capture + qwen OCR evidence layer for the desktop task suite.

Every task turn MUST produce a screenshot (capture is enforced: a failed
capture fails the task), and the final capture MUST be OCR-verified with
the local qwen vision model before the task can pass (unless the operator
explicitly opts out with --no-ocr, which is documented as not acceptable
for acceptance runs).

Capture backends:
  - KDE Plasma/Wayland: `spectacle -b -n -o` (compositor capture).
  - X11 / Xvfb: ImageMagick `import -window root`.
"""

from __future__ import annotations

import base64
import json
import os
import shutil
import subprocess
import urllib.request
from pathlib import Path
from typing import Any


class EvidenceError(RuntimeError):
    pass


def capture_screen(path: Path, display: str | None = None) -> Path:
    """Capture the screen to `path`; raise on failure (enforced contract).

    KDE Plasma/Wayland uses `spectacle`; X11/Xvfb uses ImageMagick
    `import` against the session's display (the harness may run with no
    DISPLAY of its own while the app lives on its own Xvfb).
    """
    if os.environ.get("WAYLAND_DISPLAY") and shutil.which("spectacle"):
        result = subprocess.run(
            ["spectacle", "-b", "-n", "-o", str(path)],
            capture_output=True, text=True, timeout=60, check=False,
        )
        if result.returncode != 0 or not path.is_file() or path.stat().st_size == 0:
            raise EvidenceError(f"spectacle capture failed: {result.stderr.strip()[:200]}")
        return path
    command = ["import", "-window", "root"]
    if display:
        command = ["import", "-display", display, "-window", "root"]
    command.append(str(path))
    result = subprocess.run(
        command, capture_output=True, text=True, timeout=60, check=False,
    )
    if result.returncode != 0 or not path.is_file() or path.stat().st_size == 0:
        raise EvidenceError(f"import capture failed: {result.stderr.strip()[:200] or path}")
    return path


def ocr_qwen(image_path: Path, model: str = "qwen3.5:9b", endpoint: str = "http://127.0.0.1:11434") -> str:
    """OCR a screenshot with the local qwen vision model (Ollama)."""
    payload = {
        "model": model,
        "prompt": (
            "OCR this screenshot of a desktop app. Reproduce every visible "
            "text character-for-character, preserving order. Output only the "
            "extracted text, no commentary."
        ),
        "images": [base64.b64encode(image_path.read_bytes()).decode()],
        "stream": False,
        "options": {"reasoning_effort": "none"},
    }
    request = urllib.request.Request(
        endpoint + "/api/generate",
        data=json.dumps(payload).encode(),
        headers={"Content-Type": "application/json"},
    )
    try:
        with urllib.request.urlopen(request, timeout=600) as resp:  # noqa: S310
            data = json.loads(resp.read())
    except OSError as exc:
        raise EvidenceError(f"qwen OCR unavailable ({model} @ {endpoint}): {exc}") from exc
    text = data.get("response", "")
    if not text.strip():
        raise EvidenceError(f"qwen OCR returned empty text for {image_path.name}")
    return text


def term_matches(term: str, text: str) -> bool:
    """Rubric terms support `a|b` alternation; any alternative matches."""
    lowered = text.casefold()
    return any(alt.strip().casefold() in lowered for alt in term.split("|") if alt.strip())


class Evidence:
    """Mandatory capture + OCR evidence for one task run."""

    def __init__(self, directory: Path, model: str = "qwen3.5:9b", endpoint: str = "http://127.0.0.1:11434", display: str | None = None) -> None:
        self.directory = Path(directory)
        self.directory.mkdir(parents=True, exist_ok=True)
        self.model = model
        self.endpoint = endpoint
        self.display = display
        self.captures: list[dict[str, Any]] = []

    def capture(self, label: str) -> Path:
        """Capture the screen; a failure raises — capture is enforced."""
        path = self.directory / f"{label}.png"
        capture_screen(path, self.display)
        self.captures.append({"label": label, "path": str(path)})
        return path

    def ocr(self, path: Path, required_terms: list[str]) -> dict[str, Any]:
        """OCR `path` and check required terms; record and return the result."""
        text = ocr_qwen(path, self.model, self.endpoint)
        missing = [term for term in required_terms if not term_matches(term, text)]
        record = {"capture": str(path), "ocr": text, "missing_required_terms": missing}
        self.captures[-1]["ocr"] = record
        return record
