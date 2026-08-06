#!/usr/bin/env python3
"""Surface-contract gate (spec-015 A5/R12) — the single surface contract.

Replaces check-desktop-ipc-matrix.py: the renderer surface is a client of
the surface protocol now, so the gate owns the FULL formula:

  wire_set = registry − NON_WIRE_CHANNELS − SUPERSEDED_CHAT_FAMILY
             + STREAMING_TRIO + PROTOCOL_METHODS
             (project_root_stage_native is the shell-gated bucket: wire
             reachable from client_kind:"shell" connections ONLY)

and pins:
  1. The committed schema (docs/architecture/surface-protocol.schema.json):
     its method set == wire_set exactly (no phantoms, no missing), its
     event vocabulary == the runtime stream-event vocabulary, its
     protocol_version == PROTOCOL_VERSION.
  2. The committed registry dump (docs/architecture/surface-protocol.registry.json):
     a snapshot of the derivation above; drift requires the --update-dump
     ritual (just surface-contract-dump).
  3. Renderer-union rules (contracts.ts DesktopMethod):
     - CRITICAL − SUPERSEDED ⊆ union (critical product paths stay)
     - union ⊆ wire_set ∪ shell allowlist (the renderer cannot invent)
     - staging methods are shell-kind only: never in the union
     - union ∩ (SUPERSEDED ∪ NON_WIRE ∪ SERVER_ORIGIN) = ∅
  4. The HTTP-legacy bucket (A14): the superseded chat_approval_resolve
     string MAY appear in httpTransport.ts / fixtureTransport.ts (the
     named legacyInvoke shims) but NEVER in the DesktopMethod union or
     anywhere else in the renderer surface.

Exit 0 on success; print a summary and exit 1 on any violation.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

CONTRACT_RS = ROOT / "crates/optimus-host/src/contract.rs"
ROUTER_RS = ROOT / "crates/optimus-host/src/router.rs"
CONTRACTS_TS = ROOT / "apps/optimus-ui/src/ipc/contracts.ts"
SCHEMA_JSON = ROOT / "docs/architecture/surface-protocol.schema.json"
DUMP_JSON = ROOT / "docs/architecture/surface-protocol.registry.json"
UI_IPC_DIR = ROOT / "apps/optimus-ui/src/ipc"

# The renderer's critical product paths (P15/U1/U2, carried over from the
# desktop-ipc-matrix gate). chat_approval_resolve is superseded by the trio
# member and drops out via the CRITICAL − SUPERSEDED rule.
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

# The runtime stream-event vocabulary (stream_event_to_json, chat.rs).
EVENT_VOCABULARY = frozenset(
    {"delta", "thinking", "tool", "status", "timing", "done", "cancelled", "error"}
)

# The HTTP-legacy bucket (A14/R12): transports that may still reference the
# superseded member through their named legacyInvoke shim.
LEGACY_TRANSPORTS = frozenset({"httpTransport.ts", "fixtureTransport.ts"})


def parse_rust_registry(path: Path) -> list[str]:
    text = path.read_text(encoding="utf-8")
    block = re.search(r"const METHOD_DOMAINS:.*?= &\[(.*?)\];", text, re.DOTALL)
    if not block:
        raise SystemExit(f"cannot find METHOD_DOMAINS in {path}")
    methods = re.findall(r'\(\s*"([a-z0-9_]+)",\s*Domain::', block.group(1))
    if not methods:
        raise SystemExit(f"empty METHOD_DOMAINS parse in {path}")
    return methods


def parse_string_const(path: Path, name: str) -> list[str]:
    """Parse `pub const NAME: &[&str] = &["a", "b", ...];` from contract.rs."""
    text = path.read_text(encoding="utf-8")
    block = re.search(rf"pub const {name}:[^=]*= &\s*\[(.*?)\];", text, re.DOTALL)
    if not block:
        raise SystemExit(f"cannot find const {name} in {path}")
    entries = re.findall(r'"([a-z0-9_.]+)"', block.group(1))
    return entries


def parse_react_desktop_methods(path: Path) -> list[str]:
    text = path.read_text(encoding="utf-8")
    block = re.search(r"export type DesktopMethod\s*=\s*(.*?);", text, re.DOTALL)
    if not block:
        raise SystemExit(f"cannot find DesktopMethod in {path}")
    methods = re.findall(r"'([a-z0-9_]+)'", block.group(1))
    if not methods:
        raise SystemExit(f"empty DesktopMethod parse in {path}")
    return methods


def load_schema() -> dict:
    schema = json.loads(SCHEMA_JSON.read_text(encoding="utf-8"))
    methods = schema.get("methods", {})
    events = schema.get("events", {})
    return {
        "protocol_version": schema.get("protocol_version"),
        "methods": set(methods.keys()),
        "events": set(events.keys()),
    }


def derive() -> dict:
    registry = parse_rust_registry(ROUTER_RS)
    contract = {
        name: parse_string_const(CONTRACT_RS, name)
        for name in (
            "NON_WIRE_CHANNELS",
            "SUPERSEDED_CHAT_FAMILY",
            "STREAMING_TRIO",
            "PROTOCOL_METHODS",
            "SHELL_GATED_METHODS",
            "SERVER_ORIGIN_METHODS",
        )
    }
    non_wire = set(contract["NON_WIRE_CHANNELS"])
    superseded = set(contract["SUPERSEDED_CHAT_FAMILY"])
    trio = set(contract["STREAMING_TRIO"])
    protocol = set(contract["PROTOCOL_METHODS"])
    shell_gated = set(contract["SHELL_GATED_METHODS"])
    server_origin = set(contract["SERVER_ORIGIN_METHODS"])

    registry_set = set(registry)
    wire_set = (registry_set - non_wire - superseded) | trio | protocol
    return {
        "registry": registry_set,
        "non_wire": non_wire,
        "superseded": superseded,
        "trio": trio,
        "protocol": protocol,
        "shell_gated": shell_gated,
        "server_origin": server_origin,
        "wire": wire_set,
    }


def dump_payload(derived: dict, schema: dict) -> dict:
    return {
        "schema_version": 1,
        "protocol_version": schema["protocol_version"],
        "wire_methods": sorted(derived["wire"]),
        "schema_methods": sorted(schema["methods"]),
        "events": sorted(schema["events"]),
    }


DUMP_KEYS = ("schema_version", "protocol_version", "wire_methods", "schema_methods", "events")


def load_dump() -> dict:
    raw = json.loads(DUMP_JSON.read_text(encoding="utf-8"))
    return {key: raw[key] for key in DUMP_KEYS}


def check(derived: dict, schema: dict, dump: dict) -> list[str]:
    errors: list[str] = []

    # --- Bucket sanity -----------------------------------------------------
    # SERVER_ORIGIN_METHODS / STREAMING_TRIO / PROTOCOL_METHODS are wire
    # additions, not registry entries — only the exclusion buckets must
    # resolve into the registry.
    for name, bucket in (
        ("NON_WIRE_CHANNELS", derived["non_wire"]),
        ("SUPERSEDED_CHAT_FAMILY", derived["superseded"]),
        ("SHELL_GATED_METHODS", derived["shell_gated"]),
    ):
        phantom = sorted(bucket - derived["registry"])
        if phantom:
            errors.append(f"{name} entries missing from registry: " + ", ".join(phantom))

    # The shell-gated bucket is inside the wire set (R12), never in the union.
    if not derived["shell_gated"] <= derived["wire"]:
        errors.append(
            "SHELL_GATED_METHODS must be a subset of the wire set: "
            + ", ".join(sorted(derived["shell_gated"] - derived["wire"]))
        )

    # --- Schema pin --------------------------------------------------------
    if schema["protocol_version"] is None:
        errors.append("schema declares no protocol_version")
    missing_methods = sorted(derived["wire"] - schema["methods"])
    if missing_methods:
        errors.append("wire methods missing from schema: " + ", ".join(missing_methods))
    phantom_methods = sorted(schema["methods"] - derived["wire"])
    if phantom_methods:
        errors.append("schema declares methods outside the wire set: " + ", ".join(phantom_methods))
    missing_events = sorted(EVENT_VOCABULARY - schema["events"])
    if missing_events:
        errors.append("schema events missing from the runtime vocabulary: " + ", ".join(missing_events))
    phantom_events = sorted(schema["events"] - EVENT_VOCABULARY)
    if phantom_events:
        errors.append("schema declares events outside the runtime vocabulary: " + ", ".join(phantom_events))

    # --- Registry dump pin --------------------------------------------------
    live_dump = dump_payload(derived, schema)
    if dump != live_dump:
        errors.append(
            "surface-protocol.registry.json is stale — run the --update-dump "
            "ritual (just surface-contract-dump)"
        )

    # --- Renderer union rules (R12) -----------------------------------------
    union = set(parse_react_desktop_methods(CONTRACTS_TS))
    shell_allowlist = derived["shell_gated"]

    staged = sorted(union & derived["shell_gated"])
    if staged:
        errors.append("staging methods must be shell-kind only (never renderer): " + ", ".join(staged))
    invented = sorted(union - derived["wire"] - shell_allowlist)
    if invented:
        errors.append("DesktopMethod invents methods outside the wire set: " + ", ".join(invented))
    superseded_union = sorted(union & derived["superseded"])
    if superseded_union:
        errors.append("superseded methods must not be renderer-callable: " + ", ".join(superseded_union))
    non_wire_union = sorted(union & derived["non_wire"])
    if non_wire_union:
        errors.append("non-wire channels must not be renderer-callable: " + ", ".join(non_wire_union))
    server_origin_union = sorted(union & derived["server_origin"])
    if server_origin_union:
        errors.append("server-origin notifications must not be renderer-callable: " + ", ".join(server_origin_union))
    missing_critical = sorted(CRITICAL_INVOKE_METHODS - derived["superseded"] - union)
    if missing_critical:
        errors.append(
            "critical methods missing from the renderer surface: "
            + ", ".join(missing_critical)
        )

    # --- HTTP-legacy bucket (A14) -------------------------------------------
    # Exact member token: `chat_approval_resolve` — never the trio member
    # `chat_approval_resolve_start`.
    legacy_member = re.compile(r"chat_approval_resolve(?!_)")
    for source in UI_IPC_DIR.glob("*.ts"):
        text = source.read_text(encoding="utf-8")
        if not legacy_member.search(text):
            continue
        if source.name not in LEGACY_TRANSPORTS:
            errors.append(
                f"superseded member referenced outside the legacy shim bucket: {source.name}"
            )
        elif "legacyInvoke" not in text:
            errors.append(
                f"{source.name} references the superseded member without the named legacyInvoke shim"
            )

    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--update-dump",
        action="store_true",
        help="regenerate surface-protocol.registry.json from the live derivation",
    )
    args = parser.parse_args()

    derived = derive()
    schema = load_schema()

    if args.update_dump:
        payload = dump_payload(derived, schema)
        DUMP_JSON.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
        print(f"SURFACE_CONTRACT_DUMP_UPDATED methods={len(payload['wire_methods'])}")
        return 0

    dump = load_dump()
    errors = check(derived, schema, dump)

    print("SURFACE_CONTRACT")
    print(
        f"registry={len(derived['registry'])} wire={len(derived['wire'])} "
        f"trio={len(derived['trio'])} protocol={len(derived['protocol'])} "
        f"shell_gated={len(derived['shell_gated'])}"
    )
    print(
        f"schema_methods={len(schema['methods'])} schema_events={len(schema['events'])} "
        f"protocol_version={schema['protocol_version']}"
    )
    print(f"renderer_union={len(set(parse_react_desktop_methods(CONTRACTS_TS)))}")
    print(f"critical={len(CRITICAL_INVOKE_METHODS)} legacy_bucket={sorted(LEGACY_TRANSPORTS)}")
    print("coverage=every_host_method_classified_and_pinned")

    if errors:
        print("SURFACE_CONTRACT_FAILED", file=sys.stderr)
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        return 1

    print("SURFACE_CONTRACT_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
