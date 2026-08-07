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


def ocr_qwen(
    image_path: Path,
    model: str = "qwen3.5:9b",
    endpoint: str = "http://127.0.0.1:11434",
    timeout: float = 180,
) -> str:
    """OCR a screenshot with the local qwen vision model (Ollama).

    Uses the OpenAI-compatible endpoint — the same path Hermes' aux vision
    uses (proven 8/8 with qwen3.5:9b). `/api/generate` with this model
    stalls (reasoning_effort must be top-level, and even then the response
    comes back empty); `/v1/chat/completions` with `reasoning_effort: none`
    and `max_tokens` returns text reliably.
    """
    payload = {
        "model": model,
        "messages": [
            {
                "role": "user",
                "content": [
                    {
                        "type": "text",
                        "text": (
                            "OCR this screenshot of a desktop app. Reproduce every visible "
                            "text character-for-character, preserving order. Output only the "
                            "extracted text, no commentary."
                        ),
                    },
                    {
                        "type": "image_url",
                        "image_url": {
                            "url": "data:image/png;base64,"
                            + base64.b64encode(image_path.read_bytes()).decode()
                        },
                    },
                ],
            }
        ],
        "reasoning_effort": "none",
        "max_tokens": 2000,
        "stream": False,
    }
    url = endpoint.rstrip("/") + "/v1/chat/completions"
    request = urllib.request.Request(
        url,
        data=json.dumps(payload).encode(),
        headers={"Content-Type": "application/json"},
    )
    try:
        with urllib.request.urlopen(request, timeout=timeout) as resp:  # noqa: S310
            data = json.loads(resp.read())
    except OSError as exc:
        raise EvidenceError(f"qwen OCR unavailable ({model} @ {url}): {exc}") from exc
    try:
        text = data["choices"][0]["message"]["content"] or ""
    except (KeyError, IndexError, TypeError) as exc:
        raise EvidenceError(f"qwen OCR unexpected response: {str(data)[:200]}") from exc
    if not text.strip():
        raise EvidenceError(f"qwen OCR returned empty text for {image_path.name}")
    return text


def _levenshtein(a: str, b: str) -> int:
    if len(a) < len(b):
        a, b = b, a
    if not b:
        return len(a)
    prev = list(range(len(b) + 1))
    for i, ca in enumerate(a, 1):
        cur = [i]
        for j, cb in enumerate(b, 1):
            cur.append(min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + (ca != cb)))
        prev = cur
    return prev[-1]


def term_matches(term: str, text: str) -> bool:
    """Rubric terms support `a|b` alternation; any alternative matches.

    Matching is layered so a vision-model OCR glyph misread does not fail a
    task that demonstrably rendered the term:
      1. exact casefold substring (prose rubric terms),
      2. alphanumeric skeleton substring (separator/case noise),
      3. for token-like terms (>=8 alnum chars containing a digit), any
         text window within Levenshtein distance 1 of the skeleton —
         single-glyph misreads like `6` -> `G` in a nonce.
    """
    lowered = text.casefold()
    for alt in term.split("|"):
        alt = alt.strip()
        if not alt:
            continue
        if alt.casefold() in lowered:
            return True
        skeleton = "".join(ch for ch in alt.casefold() if ch.isalnum())
        text_skeleton = "".join(ch for ch in lowered if ch.isalnum())
        if not skeleton:
            continue
        if skeleton in text_skeleton:
            return True
        if len(skeleton) >= 8 and any(ch.isdigit() for ch in skeleton):
            for start in range(len(text_skeleton) - len(skeleton) + 1):
                if _levenshtein(skeleton, text_skeleton[start : start + len(skeleton)]) <= 1:
                    return True
    return False


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
        """OCR `path` and check required terms; record and return the result.

        Retries per stalled/degenerate generate (cold model reload, transient
        busy, or a warmup transcript that matched nothing) so an infra hiccup
        does not fail an otherwise clean turn.
        """
        attempts = 0
        while True:
            attempts += 1
            try:
                text = ocr_qwen(path, self.model, self.endpoint)
            except EvidenceError:
                if attempts >= 2:
                    raise
                continue
            missing = [term for term in required_terms if not term_matches(term, text)]
            # A just-warmed model can answer with a degenerate transcript
            # (e.g. only the app title) that matches nothing; one fresh call
            # usually recovers the full text.
            if missing and attempts < 3:
                continue
            break
        record = {"capture": str(path), "ocr": text, "missing_required_terms": missing}
        self.captures[-1]["ocr"] = record
        return record
