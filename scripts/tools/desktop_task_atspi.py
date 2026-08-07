#!/usr/bin/env python3
"""Optional AT-SPI input channel for the desktop task harness.

The repo's native-UI evidence ladder (`skills/optimus-native-ui-testing`)
ranks accessibility-level input above DOM scripting: AT-SPI is the same path
a screen-reader or keyboard user takes, so a submission made through it is
the closest thing to a real user turn that a synthetic harness can produce.

This module is deliberately optional. Hosts without the AT stack (pyatspi,
at-spi2-core, an a11y bus) keep the deterministic DOM channel: the harness
probes `atspi_available()` once per task, records the chosen channel per
prompt in its evidence, and the offline-echo contracts are identical either
way (spec-014 R13).

Importing this module must never raise on hosts without pyatspi — the
`pyatspi` import happens inside the functions, and every public function
returns instead of raising.
"""

from __future__ import annotations

import time
from typing import Any, Callable

TEXTAREA_NAME = "Message Optimus"  # aria-label of the composer textarea
SEND_BUTTON_NAME = "Send message"  # aria-label of the composer send button


def atspi_available(timeout: float = 4.0) -> tuple[bool, str]:
    """Probe the host AT stack.

    Returns ``(ok, detail)``. ``ok`` is True only when the a11y bus is live
    and exposes at least one accessible application. Never raises.
    """
    try:
        import pyatspi  # noqa: PLC0415
    except Exception as exc:  # noqa: BLE001 — any import failure means no stack
        return False, f"pyatspi import failed: {exc.__class__.__name__}"

    try:
        deadline = time.monotonic() + timeout
        desktop = None
        while time.monotonic() < deadline:
            try:
                desktop = pyatspi.Registry.getDesktop(0)
                break
            except Exception:  # noqa: BLE001 — bus may still be starting
                time.sleep(0.4)
        if desktop is None:
            return False, "a11y bus did not come up within timeout"
        # The bus can be live while the webview's AT bridge is still
        # registering applications; give it a beat before declaring the
        # stack unusable.
        while time.monotonic() < deadline:
            try:
                if desktop.childCount:
                    return True, f"a11y bus live with {desktop.childCount} application(s)"
            except Exception:  # noqa: BLE001 — tree reshuffle mid-probe
                pass
            time.sleep(0.4)
        return False, "a11y bus up but no accessible applications"
    except Exception as exc:  # noqa: BLE001
        return False, f"a11y bus probe failed: {exc.__class__.__name__}"


def _find_descendant(
    root: Any,
    predicate: Callable[[Any], bool],
    depth_limit: int = 12,
) -> Any | None:
    """Find the first descendant matching ``predicate``; never raises."""
    try:
        import pyatspi  # noqa: PLC0415, F401

        return pyatspi.findDescendant(root, predicate, depth_limit=depth_limit)
    except Exception:  # noqa: BLE001 — mid-tree disconnects are common
        return None


def submit_prompt(prompt: str) -> dict:
    """Submit a composer prompt through AT-SPI.

    Locates the composer textarea by accessible name, replaces its text via
    the editable-text interface, then presses the send button through its
    accessible action. Returns evidence ``{"ok": bool, "detail": str}`` and
    never raises — callers fall back to the DOM channel on any failure.
    """
    try:
        import pyatspi  # noqa: PLC0415
    except Exception as exc:  # noqa: BLE001
        return {"ok": False, "detail": f"pyatspi unavailable: {exc.__class__.__name__}"}

    try:
        desktop = pyatspi.Registry.getDesktop(0)
        textarea = _find_descendant(
            desktop,
            lambda acc: (
                acc.getRole() in (pyatspi.ROLE_TEXT, pyatspi.ROLE_MULTI_LINE_TEXT)
                and acc.name == TEXTAREA_NAME
            ),
        )
        if textarea is None:
            return {
                "ok": False,
                "detail": f"composer textarea {TEXTAREA_NAME!r} not in the a11y tree",
            }
        try:
            editable = textarea.queryEditableText()
        except NotImplementedError as exc:
            return {"ok": False, "detail": f"textarea is not editable over AT-SPI: {exc}"}
        editable.setTextContents(prompt)

        button = _find_descendant(
            desktop,
            lambda acc: acc.getRole() == pyatspi.ROLE_PUSH_BUTTON
            and acc.name == SEND_BUTTON_NAME,
        )
        if button is None:
            return {
                "ok": False,
                "detail": f"send button {SEND_BUTTON_NAME!r} not in the a11y tree",
            }
        try:
            action = button.queryAction()
        except NotImplementedError as exc:
            return {"ok": False, "detail": f"send button exposes no actions: {exc}"}
        if action.nActions < 1:
            return {"ok": False, "detail": "send button exposes zero actions"}
        action.doAction(0)
        return {"ok": True, "detail": f"text set and {action.getActionName(0)!r} pressed"}
    except Exception as exc:  # noqa: BLE001 — never raise; the DOM channel owns the contract
        return {"ok": False, "detail": f"atspi submission failed: {exc.__class__.__name__}: {exc}"}


def choose_channel(atspi_ok: bool) -> str:
    """Deterministic per-prompt channel decision, recorded in evidence."""
    return "atspi" if atspi_ok else "dom"
