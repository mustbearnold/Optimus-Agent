#!/usr/bin/env python3
"""Generate and validate repository-local Optimus Engineering Memory indexes.

The generator intentionally uses only Python's standard library plus `cargo
metadata`. It treats Rust source as canonical for the current tool catalog and
fails closed when the expected catalog shape cannot be reconciled.
"""

from __future__ import annotations

import argparse
import functools
import fnmatch
import hashlib
import json
import os
import re
import subprocess
import sys
from collections import Counter, defaultdict
from pathlib import Path
from typing import Any, Iterable

ROOT = Path(__file__).resolve().parents[1]
MEMORY_DIR = ROOT / ".engineering-memory"
GENERATOR = "scripts/engineering_memory.py"
SCHEMA_VERSION = 1
GENERATED_NAMES = (
    "repository-index.json",
    "agent-registry.json",
    "tool-registry.json",
    "workflow-registry.json",
    "prompt-registry.json",
    "model-registry.json",
    "dependency-graph.json",
    "source-to-test-map.json",
    "contract-coverage.json",
    "evaluation-coverage.json",
    "change-impact.json",
    "knowledge-staleness.json",
)
EXCLUDED_PARTS = {
    ".git",
    ".engineering-memory",
    ".cache",
    "__pycache__",
    "node_modules",
    "target",
    "local",
    "dist",
    "build",
}
SENSITIVE_NAMES = {
    ".env",
    "auth.json",
    "credentials.json",
    "secrets.json",
    "id_rsa",
    "id_ed25519",
}
LANGUAGE_BY_SUFFIX = {
    ".rs": "Rust",
    ".py": "Python",
    ".js": "JavaScript",
    ".ts": "TypeScript",
    ".tsx": "TypeScript",
    ".jsx": "JavaScript",
    ".md": "Markdown",
    ".json": "JSON",
    ".toml": "TOML",
    ".css": "CSS",
    ".html": "HTML",
    ".ps1": "PowerShell",
    ".sh": "Shell",
    ".yml": "YAML",
    ".yaml": "YAML",
}
IMPORTANT_DOCS = (
    "docs/engineering-memory/README.md",
    "docs/architecture/system-overview.md",
    "docs/maps/repository-and-ownership.md",
    "docs/maps/memory-and-retrieval.md",
    "docs/maps/model-routing.md",
    "docs/maps/security-and-approvals.md",
    "docs/maps/observability-and-evaluations.md",
    "docs/contracts/high-risk-contracts.md",
    "docs/plans/engineering-memory-phases.md",
    "docs/lessons/ai-agent-mistakes.md",
    "docs/decisions/0017-engineering-memory-separation.md",
    "docs/decisions/README.md",
)


class MemoryError(RuntimeError):
    """Deterministic generation or validation failure."""


def relative(path: Path) -> str:
    return path.resolve().relative_to(ROOT.resolve()).as_posix()


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def generated_header() -> dict[str, Any]:
    return {
        "schema_version": SCHEMA_VERSION,
        "generated_by": GENERATOR,
        "do_not_edit": True,
    }


def is_excluded(path: Path) -> bool:
    try:
        parts = path.relative_to(ROOT).parts
    except ValueError:
        return True
    return any(part in EXCLUDED_PARTS for part in parts)


def is_sensitive(path: Path) -> bool:
    name = path.name.lower()
    return name in SENSITIVE_NAMES or name.startswith(".env.")


@functools.lru_cache(maxsize=1)
def repository_files() -> tuple[Path, ...]:
    out: list[Path] = []
    for directory, dirnames, filenames in os.walk(ROOT):
        dirnames[:] = sorted(name for name in dirnames if name not in EXCLUDED_PARTS)
        base = Path(directory)
        for filename in sorted(filenames):
            path = base / filename
            if is_sensitive(path):
                continue
            out.append(path)
    return tuple(sorted(out, key=relative))


def file_record(path: Path) -> dict[str, Any]:
    data = path.read_bytes()
    try:
        lines = len(data.decode("utf-8").splitlines())
    except UnicodeDecodeError:
        lines = None
    return {
        "path": relative(path),
        "sha256": sha256_bytes(data),
        "bytes": len(data),
        "lines": lines,
        "language": LANGUAGE_BY_SUFFIX.get(path.suffix.lower(), "Other"),
    }


def records_tree_hash(records: Iterable[dict[str, Any]]) -> str:
    digest = hashlib.sha256()
    for record in sorted(records, key=lambda item: item["path"]):
        digest.update(record["path"].encode("utf-8"))
        digest.update(b"\0")
        digest.update(record["sha256"].encode("ascii"))
        digest.update(b"\0")
    return digest.hexdigest()


def cargo_metadata() -> dict[str, Any]:
    proc = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        timeout=120,
        check=False,
    )
    if proc.returncode != 0:
        raise MemoryError(f"cargo metadata failed: {proc.stderr.strip()}")
    try:
        return json.loads(proc.stdout)
    except json.JSONDecodeError as exc:
        raise MemoryError(f"cargo metadata returned invalid JSON: {exc}") from exc


def package_records(metadata: dict[str, Any]) -> list[dict[str, Any]]:
    workspace_ids = set(metadata["workspace_members"])
    packages = []
    for package in metadata["packages"]:
        if package["id"] not in workspace_ids:
            continue
        manifest = Path(package["manifest_path"])
        targets = [
            {
                "name": target["name"],
                "kind": target["kind"],
                "crate_types": target["crate_types"],
                "src_path": relative(Path(target["src_path"])),
            }
            for target in package["targets"]
        ]
        packages.append(
            {
                "name": package["name"],
                "version": package["version"],
                "manifest_path": relative(manifest),
                "description": package.get("description"),
                "targets": targets,
                "application": relative(manifest).startswith("apps/"),
            }
        )
    return sorted(packages, key=lambda item: item["name"])


def git_identity() -> dict[str, Any]:
    if not (ROOT / ".git").exists():
        return {
            "available": False,
            "head": None,
            "verification_basis": "sha256_tree_no_git",
        }
    proc = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        timeout=15,
        check=False,
    )
    return {
        "available": proc.returncode == 0,
        "head": proc.stdout.strip() if proc.returncode == 0 else None,
        "verification_basis": "git_head_and_sha256_tree",
    }


def build_repository_index(metadata: dict[str, Any]) -> dict[str, Any]:
    records = [file_record(path) for path in repository_files()]
    language_counts: dict[str, dict[str, int]] = defaultdict(lambda: {"files": 0, "bytes": 0})
    for record in records:
        bucket = language_counts[record["language"]]
        bucket["files"] += 1
        bucket["bytes"] += record["bytes"]
    top_entries = sorted(path.name for path in ROOT.iterdir() if path.name not in EXCLUDED_PARTS)
    independent = []
    for manifest in (path for path in repository_files() if path.name == "Cargo.toml"):
        if manifest == ROOT / "Cargo.toml" or is_excluded(manifest):
            continue
        text = manifest.read_text(encoding="utf-8", errors="replace")
        if re.search(r"(?m)^\[workspace\]\s*$", text):
            independent.append(relative(manifest.parent))
    domains = {}
    for name in ("agents", "workflows", "tools", "prompts", "evals", "fixtures", "packages"):
        domains[name] = (ROOT / name).is_dir()
    return {
        **generated_header(),
        "git": git_identity(),
        "tree_sha256": records_tree_hash(records),
        "root_entries": top_entries,
        "workspace_packages": package_records(metadata),
        "independent_workspaces": sorted(independent),
        "top_level_domains": domains,
        "languages": dict(sorted(language_counts.items())),
        "file_count": len(records),
        "files": records,
        "excluded_parts": sorted(EXCLUDED_PARTS),
        "sensitive_files": "excluded_without_reading",
    }


def build_dependency_graph(metadata: dict[str, Any]) -> dict[str, Any]:
    workspace = {
        package["name"]: package
        for package in metadata["packages"]
        if package["id"] in set(metadata["workspace_members"])
    }
    edges = []
    external: dict[str, list[str]] = {}
    for name, package in sorted(workspace.items()):
        ext = []
        for dependency in package["dependencies"]:
            dep_name = dependency.get("rename") or dependency["name"]
            if dependency["name"] in workspace:
                edges.append(
                    {
                        "from": name,
                        "to": dependency["name"],
                        "kind": dependency["kind"] or "normal",
                    }
                )
            else:
                ext.append(dep_name)
        external[name] = sorted(set(ext))
    return {
        **generated_header(),
        "nodes": sorted(workspace),
        "workspace_edges": sorted(edges, key=lambda item: (item["from"], item["to"], item["kind"])),
        "external_dependencies": external,
    }


def find_matching(text: str, opening: int, left: str = "(", right: str = ")") -> int:
    if opening >= len(text) or text[opening] != left:
        raise MemoryError(f"expected {left!r} at offset {opening}")
    depth = 0
    in_string = False
    escaped = False
    for index in range(opening, len(text)):
        char = text[index]
        if in_string:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                in_string = False
            continue
        if char == '"':
            in_string = True
        elif char == left:
            depth += 1
        elif char == right:
            depth -= 1
            if depth == 0:
                return index
    raise MemoryError(f"unbalanced {left}{right} block at offset {opening}")


def extract_calls(text: str, pattern: str) -> list[str]:
    calls = []
    for match in re.finditer(pattern + r"\s*\(", text):
        opening = text.find("(", match.start())
        end = find_matching(text, opening)
        calls.append(text[match.start() : end + 1])
    return calls


def parse_json_macro(call: str) -> Any:
    match = re.search(r"json!\s*\(", call)
    if not match:
        raise MemoryError("tool schema is missing json! macro")
    opening = call.find("(", match.start())
    end = find_matching(call, opening)
    raw = call[opening + 1 : end].strip()
    try:
        return json.loads(raw)
    except json.JSONDecodeError as exc:
        raise MemoryError(f"cannot parse canonical tool JSON schema: {raw}: {exc}") from exc


def source_test_hits(tool_id: str) -> list[str]:
    hits = []
    for path in repository_files():
        rel = relative(path)
        if "/tests/" not in f"/{rel}" and not rel.startswith("apps/optimus-desktop/e2e/"):
            continue
        if path.suffix not in {".rs", ".js", ".ts"}:
            continue
        text = path.read_text(encoding="utf-8", errors="replace")
        if tool_id in text:
            hits.append(rel)
    return sorted(hits)


def tool_operational_metadata(tool_id: str, policy: str, available: bool) -> dict[str, Any]:
    side_effects = {
        "workspace_read": ["none"],
        "workspace_write": ["filesystem_write"],
        "process": ["process_execution", "filesystem_and_external_effects_possible"],
        "network_read": ["network_read"],
        "memory_read": ["none"],
        "skill_read": ["none"],
        "capability": ["session_capability_state"],
        "browser": ["network_read", "browser_state_write"],
        "user_interaction": ["user_interaction"],
        "desktop": ["desktop_control"],
        "media": ["media_generation_or_capture"],
        "network_write": ["network_write"],
    }.get(policy, ["unknown"])
    permissions = {
        "workspace_read": {"filesystem": "scoped_read", "network": "none"},
        "workspace_write": {"filesystem": "workspace_write", "network": "none"},
        "process": {"filesystem": "workspace_cwd", "process": "approval_gated"},
        "network_read": {"network": "read"},
        "memory_read": {"memory": "read"},
        "skill_read": {"skills": "read"},
        "capability": {"capability_session": "mutate"},
        "browser": {"network": "ssrf_filtered_read", "browser": "bounded_http"},
    }.get(policy, {"status": "unknown_or_unresolved"})
    if not available:
        timeout = {"status": "not_applicable_unavailable", "seconds": None}
        approval = {"status": "not_applicable_unavailable"}
    elif tool_id == "terminal":
        timeout = {"status": "confirmed_current_behaviour", "seconds": 30, "source": "JobBudget::default"}
        approval = {"status": "required", "scope": "job", "policy": "SmartDeny"}
    elif policy in {"network_read", "browser"}:
        timeout = {"status": "confirmed_current_behaviour", "seconds": 30, "note": "effector-specific upper bound"}
        approval = {"status": "not_required_current_policy"}
    else:
        timeout = {"status": "unknown_or_unresolved", "seconds": None}
        approval = {"status": "not_required_current_policy"}
    return {
        "side_effects": side_effects,
        "permissions": permissions,
        "timeout": timeout,
        "supports_cancellation": available and tool_id in {"write_file", "terminal"},
        "cancellation_status": (
            "terminal_owner" if available and tool_id in {"write_file", "terminal"}
            else "unsupported" if available else "not_applicable_unavailable"
        ),
        "retry": {"status": "never" if available else "not_applicable_unavailable"},
        "idempotency": {
            "status": (
                "convergent" if tool_id == "write_file"
                else "keyed" if available and tool_id in {"read_file", "memory_recall", "skill_resolve", "activate_pack"}
                else "none" if available else "not_applicable_unavailable"
            )
        },
        "determinism": {"status": "declared_by_replay_class" if available else "not_applicable_unavailable"},
        "replay": {"status": "canonical_tool_descriptor" if available else "not_applicable_unavailable"},
        "observability_contract": {
            "call_identity_required": available,
            "trace_span_required": available,
            "provenance_required": available,
        },
        "approval": approval,
        "error_taxonomy": {"status": "tool_specific_not_canonical" if available else "not_applicable_unavailable"},
        "logging": {"status": "kernel_trace_and_or_runtime_events" if available else "not_applicable_unavailable"},
    }


def parse_tool_catalog() -> dict[str, Any]:
    path = ROOT / "crates/optimus-packs/src/lib.rs"
    text = path.read_text(encoding="utf-8")
    start = text.find("pub fn builtin_catalog")
    end = text.find("pub struct CapabilitySession")
    if start < 0 or end < 0 or end <= start:
        raise MemoryError("cannot locate builtin_catalog boundaries")
    catalog = text[start:end]

    invocation_ids = dict(re.findall(r"Self::(\w+)\s*=>\s*Some\(\"([^\"]+)\"\)", text))
    invocation_policy = dict(
        re.findall(r"Self::(\w+)\s*=>\s*Some\(ToolPolicy::(\w+)\)", text)
    )
    for variant in ("BrowserNavigate", "BrowserSnapshot", "BrowserClick"):
        invocation_policy.setdefault(variant, "Browser")
    if len(invocation_ids) != 10:
        raise MemoryError(f"expected 10 available ToolInvocation IDs, found {len(invocation_ids)}")

    packs = []
    tools = []
    for block in extract_calls(catalog, r"m\.insert"):
        pack_match = re.search(r"PackId::(\w+)", block)
        description_match = re.search(r"summary:\s*\"([^\"]+)\"", block)
        if not (pack_match and description_match):
            raise MemoryError("cannot parse a builtin pack descriptor")
        pack_variant = pack_match.group(1)
        pack_id = re.sub(r"(?<!^)(?=[A-Z])", "_", pack_variant).lower()
        pack_tools = []
        for call in extract_calls(block, r"\btool"):
            variant_match = re.search(r"ToolInvocation::(\w+)", call)
            if not variant_match:
                raise MemoryError(f"available tool in {pack_id} lacks invocation")
            variant = variant_match.group(1)
            tool_id = invocation_ids.get(variant)
            policy_variant = invocation_policy.get(variant)
            if not tool_id or not policy_variant:
                raise MemoryError(f"unmapped invocation {variant}")
            desc_match = re.search(
                r"ToolInvocation::\w+\s*,\s*\"([^\"]+)\"\s*,\s*(\d+)",
                call,
                re.S,
            )
            if not desc_match:
                raise MemoryError(f"cannot parse descriptor for {tool_id}")
            properties = parse_json_macro(call)
            required_match = re.search(r"&\s*\[(.*?)\]", call, re.S)
            required = re.findall(r'\"([^\"]+)\"', required_match.group(1)) if required_match else []
            schema = {
                "type": "object",
                "properties": properties,
                "required": required,
                "additionalProperties": False,
            }
            policy = re.sub(r"(?<!^)(?=[A-Z])", "_", policy_variant).lower()
            record = {
                "id": tool_id,
                "version": 1,
                "owner": "optimus-packs",
                "effector_owner": "optimus-kernel" if tool_id not in {"write_file", "terminal"} else "optimus-runtime",
                "pack": pack_id,
                "available": True,
                "description": desc_match.group(1),
                "invocation": variant,
                "policy": policy,
                "input_schema": schema,
                "output_schema": {
                    "status": "confirmed_current_behaviour",
                    "transport": {"type": "json_string"},
                    "envelope_schema": "optimus_packs::ToolOutcome/v1",
                    "semantic_schema": "ToolOutcome.data JSON value is tool-specific",
                    "terminal_kinds": ["succeeded", "failed", "cancelled", "ambiguous"],
                },
                "schema_tokens": int(desc_match.group(2)),
                "validated_by": source_test_hits(tool_id),
                "source": "crates/optimus-packs/src/lib.rs",
                **tool_operational_metadata(tool_id, policy, True),
            }
            tools.append(record)
            pack_tools.append(tool_id)
        for call in extract_calls(block, r"\bunavailable"):
            strings = re.findall(r'\"([^\"]+)\"', call)
            policy_match = re.search(r"ToolPolicy::(\w+)", call)
            if len(strings) < 2 or not policy_match:
                raise MemoryError(f"cannot parse unavailable tool in {pack_id}")
            tool_id, description = strings[0], strings[1]
            policy = re.sub(r"(?<!^)(?=[A-Z])", "_", policy_match.group(1)).lower()
            record = {
                "id": tool_id,
                "version": 1,
                "owner": "optimus-packs",
                "effector_owner": None,
                "pack": pack_id,
                "available": False,
                "description": description,
                "invocation": "Unavailable",
                "policy": policy,
                "input_schema": None,
                "output_schema": None,
                "schema_tokens": 0,
                "validated_by": source_test_hits(tool_id),
                "source": "crates/optimus-packs/src/lib.rs",
                **tool_operational_metadata(tool_id, policy, False),
            }
            tools.append(record)
            pack_tools.append(tool_id)
        packs.append(
            {
                "id": pack_id,
                "description": description_match.group(1),
                "base_schema_tokens": sum(tool["schema_tokens"] for tool in tools if tool["id"] in pack_tools),
                "tools": pack_tools,
            }
        )
    ids = [tool["id"] for tool in tools]
    if len(ids) != 22 or len(set(ids)) != 22:
        raise MemoryError(f"expected 22 unique canonical tools, found {len(ids)}/{len(set(ids))}")
    return {
        **generated_header(),
        "canonical_source": "crates/optimus-packs/src/lib.rs",
        "canonical_type": "optimus_packs::ToolDesc",
        "packs": packs,
        "tools": sorted(tools, key=lambda item: item["id"]),
        "known_gaps": [
            "ToolOutcome.data remains tool-specific rather than one universal semantic data schema.",
            "Operational declarations do not create universal runtime cancellation or retry implementations.",
            "Unavailable placeholders are catalog entries but are not provider-advertised tools.",
        ],
    }


def build_agent_registry() -> dict[str, Any]:
    definitions = []
    pattern = re.compile(r"\b(?:trait|struct)\s+([A-Za-z_][A-Za-z0-9_]*Agent)\b")
    for path in repository_files():
        if path.suffix != ".rs":
            continue
        for name in pattern.findall(path.read_text(encoding="utf-8", errors="replace")):
            definitions.append({"name": name, "source": relative(path)})
    if definitions:
        raise MemoryError(f"specialist agent definitions require registry review: {definitions}")
    contract = ROOT / "crates/optimus-kernel/src/agent.rs"
    required_symbols = [
        "pub struct AgentDescriptor",
        "pub struct AgentRequest",
        "pub struct AgentResult",
        "pub struct AgentRegistry",
        "pub struct AgentInvocationStore",
    ]
    text = contract.read_text(encoding="utf-8")
    missing = [symbol for symbol in required_symbols if symbol not in text]
    if missing:
        raise MemoryError(f"agent contract extraction is stale; missing symbols: {missing}")
    return {
        **generated_header(),
        "agents": [],
        "implemented_specialist_agent_count": 0,
        "contract_substrate": {
            "id": "specialist-agent-contract",
            "version": 1,
            "status": "implemented",
            "source": relative(contract),
            "source_file_sha256": sha256_file(contract),
            "contracts": [
                "versioned descriptor/request/result",
                "canonical tool and permission closure",
                "immutable descriptor registry",
                "durable invocation/cancellation/retry/terminal ledger",
                "runtime-validated durable effect provenance",
            ],
            "validated_by": [
                "crates/optimus-kernel/tests/agent_contracts.rs",
                "crates/optimus-kernel/tests/integrity_integration.rs",
            ],
        },
        "status": "implemented_contract_no_builtin_specialists",
        "statement": "A universal typed agent contract and durable invocation substrate exist; no built-in specialist definition is registered.",
        "not_agents": [
            {"symbol": "optimus_kernel::ModelProvider", "reason": "provider adapter interface"},
            {"symbol": "optimus_kernel::ScriptedModel", "reason": "offline/test model adapter"},
            {"symbol": "optimus_runtime::CampaignStep", "reason": "deterministic effect step, not typed agent invocation"},
        ],
        "required_before_first_agent": [
            "purpose and non-responsibilities",
            "version, owner, status, and evaluation cases",
            "overlap/routing check",
        ],
    }


def workflow_record(**values: Any) -> dict[str, Any]:
    base = {
        "version": 1,
        "status": "active",
        "cancellation": {"status": "unknown_or_unresolved_behaviour"},
        "retry": {"status": "none_or_subsystem_specific"},
        "rollback": {"status": "not_defined"},
    }
    base.update(values)
    return base


def build_workflow_registry() -> dict[str, Any]:
    workflows = [
        workflow_record(
            id="kernel-turn",
            owner="optimus-kernel",
            trigger="direct CLI/desktop/gateway/cron call",
            inputs=["user text", "session", "provider adapter", "KernelConfig"],
            outputs=["TurnResult", "session transcript", "durable effect causal links", "stream events"],
            stages=["persist user", "complete model", "prevalidate call batch", "dispatch tools", "persist tool results", "repeat or finalize"],
            dependencies=["optimus-packs", "optimus-memory", "optimus-skills", "optimus-runtime"],
            state_transitions="bounded model/tool loop",
            timeout="provider/effector specific; no turn-wide deadline",
            approvals="durable runtime jobs for terminal",
            validation="strict provider and canonical tool schemas",
            completion=["non-empty assistant final response"],
            cancellation={"status": "implemented", "contract": "cloneable cooperative token reaches active providers and every model/tool loop boundary; Codex SSE polls on bounded reads; desktop native/HTTP stream delivery loss and explicit capability-local Stop request the same token; native ownership is exact-ID and bounded"},
            failure=["model/provider error", "invalid tool batch", "turn budget exceeded", "effect error", "synchronous transport connect/write is not force-abortable"],
            observability=["TurnEvent sink", "session transcript", "session tool-call to effect-attempt hash links", "runtime events for durable effects"],
            validated_by=["crates/optimus-kernel/tests/kernel_turn.rs", "crates/optimus-kernel/src/eval.rs"],
            source=["crates/optimus-kernel/src/lib.rs"],
        ),
        workflow_record(
            id="work-graph-job",
            owner="optimus-runtime",
            trigger="Runtime create/run/resume",
            inputs=["JobSpec", "RuntimeConfig", "optional approval grant"],
            outputs=["JobStatus", "StepOutcome", "ordered events", "optional command capture"],
            stages=["persist job/nodes", "select runnable node", "policy gate", "execute bounded effect", "persist outcome", "recompute status"],
            dependencies=["optimus-graph", "optimus-store", "optimus-skills"],
            state_transitions="job/node state machine",
            timeout="per-command JobBudget timeout",
            approvals="SmartDeny grant bound to exact job, node, effect hash, actor, and expiry for RunCommand",
            validation="path, policy, budget, and state-transition checks; projection/event transitions are atomic and legacy partial state is quarantined",
            cancellation={"status": "implemented", "contract": "durable idempotent request; pending atomic terminalization; running command tree termination and reap"},
            completion=["succeeded", "failed", "cancelled", "interrupted", "awaiting_approval"],
            failure=["invalid transition", "budget", "path escape", "effect/process failure"],
            observability=["ordered SQLite event rows", "job/node projections"],
            validated_by=["crates/optimus-runtime/tests/**"],
            source=["crates/optimus-runtime/src/lib.rs", "crates/optimus-graph/src/lib.rs", "crates/optimus-store/src/lib.rs"],
        ),
        workflow_record(
            id="durable-campaign",
            owner="optimus-runtime",
            trigger="campaign create then run/resume",
            inputs=["name", "ordered WriteFile/RunCommand steps"],
            outputs=["CampaignView", "step jobs and details"],
            stages=["transactionally persist campaign plan and deterministic job IDs", "atomically create or reuse each non-terminal step job", "target recovery at an interrupted step job", "pause for approval or derive a terminal result"],
            dependencies=["work-graph-job", "optimus.db"],
            state_transitions="job-derived campaign and campaign-step projections",
            timeout="delegated to each job",
            approvals="delegated to each command job",
            cancellation={"status": "implemented", "contract": "propagates to created jobs and uncreated steps; repeated cancellation is idempotent"},
            validation="campaign schema v4 transactional migrations, future-version rejection, typed fail-closed plan decoding, exact cardinality/indices, fenced owner leases, diagnostics, and deterministic projection repair before effects",
            completion=["succeeded", "failed", "cancelled", "awaiting_approval"],
            failure=["campaign persistence/runtime error", "step failure", "corrupt or unknown-version state fails before runtime effects", "stale or competing lease owner is fenced"],
            observability=["job-derived campaign projections", "underlying job events", "campaign diagnostics and repair reports"],
            validated_by=["crates/optimus-runtime/src/campaign.rs", "crates/optimus-store/src/lib.rs"],
            source=["crates/optimus-runtime/src/campaign.rs"],
        ),
        workflow_record(
            id="interval-cron-tick",
            owner="optimus-kernel",
            trigger="manual tick against due interval records",
            inputs=["CronJob", "current time", "provider"],
            outputs=["updated next run and last status", "kernel turn effects"],
            stages=["transactionally claim due enabled jobs", "run provider turn", "complete exact live claim and advance schedule"],
            dependencies=["cron.db", "kernel-turn"],
            state_transitions="enabled/due plus owner/generation/token/deadline lease and last status",
            retry={"status": "expiry takeover with stale-owner fencing; no general attempt policy"},
            timeout="provider/turn specific",
            approvals="inherits tool/runtime boundary",
            cancellation={"status": "implemented", "contract": "disable fences a live claim; explicit cancellation terminalizes the exact attempt and rejects stale completion"},
            validation="minimum interval, exact lease capability, legacy schema migration, and surface-specific provider match",
            completion=["tick reports each due job status"],
            failure=["provider or kernel error recorded as last status", "expired or stale owner cannot commit completion"],
            observability=["cron lease projection plus kernel/runtime evidence"],
            validated_by=["crates/optimus-kernel/src/cron.rs"],
            source=["crates/optimus-kernel/src/cron.rs", "apps/optimus-cli/src/main.rs"],
        ),
        workflow_record(
            id="gateway-inbox-drain",
            owner="optimus-kernel",
            trigger="filesystem enqueue or loopback HTTP then drain",
            inputs=["InboundMessage", "turn callback/provider"],
            outputs=["OutboundMessage", "processed/failed archive", "optional session ID"],
            stages=["idempotently ingest inbox UUID", "transactionally claim attempt", "run kernel turn", "atomically commit terminal outcome/outbound JSON", "reconcile outbox/archive files"],
            dependencies=["gateway/gateway.db", "gateway adapter directories", "kernel-turn"],
            state_transitions="pending/claimed/succeeded/failed with owner/generation/token/deadline and attempt history",
            retry={"status": "bounded three-attempt retry plus dead letter; expiry takeover and explicit release"},
            timeout="turn specific",
            approvals="inherits tool/runtime boundary",
            cancellation={"status": "implemented", "contract": "exact claim cancellation is terminal and fences late completion"},
            validation="canonical UUID/file identity, typed JSON, exact lease capability, deterministic conflict-checked materialization; malformed inbox files are skipped",
            completion=["processed", "dead_lettered", "cancelled", "no message"],
            failure=["turn error commits one failed attempt/outbound", "stale owners are fenced", "materialization conflict fails closed"],
            observability=["SQLite claims/attempts/terminal outbox JSON", "reconciled files", "kernel/runtime evidence"],
            validated_by=["crates/optimus-kernel/src/gateway.rs", "apps/optimus-cli/tests/gateway_http.rs"],
            source=["crates/optimus-kernel/src/gateway.rs", "apps/optimus-cli/src/gateway_http.rs"],
        ),
        workflow_record(
            id="general-workflow-contract",
            owner="optimus-kernel",
            trigger="immutable WorkflowDefinition registration; execution is adapter-owned",
            inputs=["versioned triggers", "typed JSON-schema ports", "dependency graph", "optional agent references", "policy declarations"],
            outputs=["validated immutable definition", "adapter capability/status mapping"],
            stages=["validate identity/schema", "validate ports and acyclic graph", "validate retry/timeout/approval/terminal policy", "persist immutable definition", "map owner lifecycle without coercion"],
            dependencies=["kernel workflow contract", "job/campaign/cron/gateway owner adapters"],
            state_transitions="definition registry only; owner adapters preserve execution state",
            retry={"status": "bounded declaration; execution support explicitly reported per adapter"},
            timeout="required bounded declaration per node",
            approvals="explicit declaration; owner capability may be unsupported",
            cancellation={"status": "implemented", "contract": "declaration and adapter capability conformance; execution remains owner-specific"},
            rollback={"status": "explicit_supported_compensating_or_unsupported"},
            validation="versioned fail-closed schema, canonical IDs, unique ports/nodes, DAG, bounded policies, exact terminal declarations, immutable reopen/corruption checks",
            completion=["succeeded", "failed", "cancelled", "ambiguous"],
            failure=["invalid/cyclic/unbounded definition", "duplicate identity/version", "unknown persisted adapter status", "unsupported capability remains explicit"],
            observability=["immutable registry definition", "adapter capability matrix", "owner event stores"],
            validated_by=["crates/optimus-kernel/tests/workflow_contracts.rs", "crates/optimus-kernel/tests/integrity_integration.rs"],
            source=["crates/optimus-kernel/src/workflow.rs"],
        ),
    ]
    return {
        **generated_header(),
        "workflows": workflows,
        "known_gaps": [
            "No universal workflow executor or cross-store transaction exists; execution remains adapter-owned.",
            "Rollback and retry execution are subsystem-specific even though declarations are typed.",
            "No implemented Aipedia, publishing, SEO, content, or project-specific workflows exist.",
        ],
    }


def line_number(path: Path, needle: str) -> int | None:
    for index, line in enumerate(path.read_text(encoding="utf-8", errors="replace").splitlines(), 1):
        if needle in line:
            return index
    return None


def build_prompt_registry() -> dict[str, Any]:
    source = ROOT / "crates/optimus-kernel/src/lib.rs"
    return {
        **generated_header(),
        "prompts": [
            {
                "id": "kernel-system-prompt",
                "version": None,
                "status": "active_unversioned",
                "owner": "optimus-kernel",
                "source": relative(source),
                "line": line_number(source, "fn system_prompt"),
                "source_file_sha256": sha256_file(source),
                "inputs": ["loaded pack names", "session capability state"],
                "validated_by": ["crates/optimus-kernel/tests/kernel_turn.rs"],
            }
        ],
        "prompt_directory_present": (ROOT / "prompts").is_dir(),
        "known_gaps": ["Prompt content has no independent stable version or evaluation binding."],
    }


def build_model_registry() -> dict[str, Any]:
    routing_path = ROOT / "crates/optimus-kernel/src/routing.rs"
    routing = routing_path.read_text(encoding="utf-8")
    catalog_start = routing.find("pub const CODEX_MODEL_CATALOG")
    catalog_end = routing.find("pub const DEFAULT_CODEX_MODEL")
    if catalog_start < 0 or catalog_end < 0:
        raise MemoryError("cannot locate canonical Codex model catalog")
    ids = sorted(set(re.findall(r'\"(gpt-5\.6-(?:terra|luna|sol))\"', routing[catalog_start:catalog_end])))
    if ids != ["gpt-5.6-luna", "gpt-5.6-sol", "gpt-5.6-terra"]:
        raise MemoryError(f"unexpected Codex catalog: {ids}")
    return {
        **generated_header(),
        "router_status": "implemented_canonical_policy_resolver",
        "router_source": relative(routing_path),
        "router_source_sha256": sha256_file(routing_path),
        "providers": [
            {
                "id": "offline",
                "adapter": "optimus_kernel::ScriptedModel",
                "network": False,
                "selection": "canonical_route_resolver",
                "models": ["offline-scripted"],
                "fallback": None,
                "evidence": ["crates/optimus-kernel/src/lib.rs", "crates/optimus-kernel/src/eval.rs"],
            },
            {
                "id": "openai-compatible",
                "adapter": "optimus_kernel::OpenAiCompatModel",
                "network": True,
                "selection": "canonical_route_resolver",
                "models": "OPTIMUS_OPENAI_MODEL environment value",
                "fallback": None,
                "evidence": ["crates/optimus-kernel/src/openai_compat.rs"],
            },
            {
                "id": "codex-oauth",
                "adapter": "optimus_kernel::CodexOAuthModel",
                "network": True,
                "selection": "canonical_route_resolver",
                "models": ids,
                "aliases": ["terra", "luna", "sol"],
                "unknown_model_behavior": "gpt-5.6-terra",
                "fallback": "one same-provider slim-history retry after HTTP failure",
                "evidence": ["crates/optimus-kernel/src/lib.rs", "crates/optimus-kernel/src/codex_oauth.rs"],
            },
        ],
        "capability_routing": ["text", "tools", "streaming", "reasoning", "local"],
        "cost_policy": "static estimated microunit ceiling",
        "latency_context_metadata": None,
        "privacy_policy": "local_only_or_remote_allowed",
        "decision_ledger": "routing.db",
        "local_model_adapters": [],
        "gpu_adapters": [],
        "known_gaps": [
            "No provider health, measured latency/cost, or evaluation-driven selection exists.",
            "No local-model or GPU adapter exists.",
        ],
    }


def rust_test_functions(path: Path) -> list[str]:
    text = path.read_text(encoding="utf-8", errors="replace")
    return re.findall(r"#\s*\[test\][\s\S]{0,200}?fn\s+([A-Za-z_][A-Za-z0-9_]*)", text)


def js_test_names(path: Path) -> list[str]:
    text = path.read_text(encoding="utf-8", errors="replace")
    return re.findall(r"\btest\s*\(\s*['\"]([^'\"]+)['\"]", text)


def package_root_from_manifest(manifest: str) -> Path:
    return (ROOT / manifest).parent


def build_source_to_test_map(metadata: dict[str, Any]) -> dict[str, Any]:
    mappings = []
    for package in package_records(metadata):
        package_root = package_root_from_manifest(package["manifest_path"])
        sources = sorted(
            relative(path)
            for path in package_root.joinpath("src").rglob("*.rs")
            if path.is_file()
        )
        tests = []
        tests_dir = package_root / "tests"
        if tests_dir.exists():
            for path in sorted(tests_dir.rglob("*.rs")):
                tests.append({"path": relative(path), "cases": rust_test_functions(path)})
        for target in package["targets"]:
            source = ROOT / target["src_path"]
            if source.exists() and "test" in target["kind"]:
                tests.append({"path": relative(source), "cases": rust_test_functions(source)})
        if package["name"] == "optimus-desktop":
            for path in sorted((ROOT / "apps/optimus-desktop/e2e").glob("*.js")):
                tests.append({"path": relative(path), "cases": js_test_names(path)})
        mappings.append(
            {
                "package": package["name"],
                "mapping_kind": "package_default_coarse",
                "sources": sources,
                "tests": tests,
            }
        )
    mappings.append(
        {
            "package": "engineering-memory",
            "mapping_kind": "explicit",
            "sources": [GENERATOR],
            "tests": [{"path": "scripts/test_engineering_memory.py", "cases": []}],
        }
    )
    return {
        **generated_header(),
        "mappings": mappings,
        "limitations": [
            "Package-default mappings establish candidate impact, not line/symbol coverage.",
            "No compiler-derived Rust call graph or coverage report is available.",
        ],
    }


def build_contract_coverage() -> dict[str, Any]:
    contracts = [
        ("C-01", "cancellation", "implemented", ["crates/optimus-store/src/lib.rs", "crates/optimus-runtime/src/lib.rs", "crates/optimus-runtime/src/campaign.rs"], ["crates/optimus-runtime/tests/cancellation.rs", "crates/optimus-runtime/src/campaign.rs"]),
        ("C-02", "exactly-one-terminal-outcome", "implemented", ["crates/optimus-store/src/lib.rs", "crates/optimus-graph/src/lib.rs"], ["crates/optimus-store/src/lib.rs", "crates/optimus-runtime/tests/cancellation.rs"]),
        ("C-03", "action-bound-approval", "implemented", ["crates/optimus-runtime/src/lib.rs", "crates/optimus-store/src/lib.rs"], ["crates/optimus-runtime/tests/approvals_surface.rs", "crates/optimus-runtime/tests/skill_bridge.rs"]),
        ("C-04", "runtime-filesystem-confinement", "implemented", ["crates/optimus-runtime/src/lib.rs", "crates/optimus-kernel/src/fs_sandbox.rs"], ["crates/optimus-runtime/tests/path_confinement.rs", "crates/optimus-kernel/tests/kernel_turn.rs"]),
        ("C-05", "loopback-api-authorization", "implemented", ["apps/optimus-desktop/src/server.rs", "apps/optimus-cli/src/gateway_http.rs"], ["apps/optimus-desktop/src/server.rs", "apps/optimus-cli/tests/gateway_http.rs", "apps/optimus-desktop/e2e"]),
        ("C-06", "fail-closed-workflow-decoding", "implemented", ["crates/optimus-runtime/src/campaign.rs"], ["crates/optimus-runtime/src/campaign.rs"]),
        ("C-07", "provider-call-envelope-and-batch-authorization", "implemented", ["crates/optimus-kernel/src/lib.rs", "crates/optimus-kernel/src/openai_compat.rs", "crates/optimus-kernel/src/codex_oauth.rs", "crates/optimus-packs/src/lib.rs"], ["crates/optimus-kernel/tests/kernel_turn.rs", "crates/optimus-kernel/tests/codex_oauth.rs", "crates/optimus-packs/tests/packs_budget.rs"]),
        ("C-08", "canonical-tool-result", "implemented", ["crates/optimus-packs/src/lib.rs", "crates/optimus-kernel/src/lib.rs", "crates/optimus-kernel/src/execution.rs"], ["crates/optimus-packs/tests/packs_budget.rs", "crates/optimus-kernel/tests/kernel_turn.rs", "crates/optimus-kernel/tests/session_resume.rs"]),
        ("C-09", "agent-lifecycle", "implemented", ["crates/optimus-kernel/src/agent.rs"], ["crates/optimus-kernel/tests/agent_contracts.rs", "crates/optimus-kernel/tests/integrity_integration.rs"]),
        ("C-10", "workflow-lifecycle", "implemented", ["crates/optimus-kernel/src/workflow.rs", "crates/optimus-runtime/src/campaign.rs", "crates/optimus-kernel/src/cron.rs", "crates/optimus-kernel/src/gateway.rs"], ["crates/optimus-kernel/tests/workflow_contracts.rs", "crates/optimus-kernel/tests/integrity_integration.rs"]),
        ("C-11", "model-routing", "implemented", ["crates/optimus-kernel/src/routing.rs", "apps/optimus-cli/src/main.rs", "apps/optimus-desktop/src/ipc/chat.rs"], ["crates/optimus-kernel/src/routing.rs", "crates/optimus-kernel/tests/integrity_integration.rs"]),
        ("C-12", "credential-and-local-transport-security", "implemented", ["crates/optimus-kernel/src/credential.rs", "crates/optimus-kernel/src/codex_oauth.rs", "apps/optimus-desktop/src/server.rs"], ["crates/optimus-kernel/tests/codex_oauth.rs", "apps/optimus-cli/tests/gateway_http.rs"]),
        ("C-13", "deterministic-replay-and-provenance", "implemented", ["crates/optimus-kernel/src/execution.rs", "crates/optimus-kernel/src/replay.rs", "crates/optimus-kernel/src/trace.rs"], ["crates/optimus-kernel/tests/replay_contracts.rs", "crates/optimus-kernel/tests/trace_contracts.rs"]),
        ("C-14", "memory-clock-retention-erasure", "implemented", ["crates/optimus-memory/src/lib.rs"], ["crates/optimus-memory/tests/metamemory_mvp.rs", "crates/optimus-kernel/tests/integrity_integration.rs"]),
        ("C-15", "atomic-projection-and-event-transitions", "implemented", ["crates/optimus-store/src/lib.rs", "crates/optimus-graph/src/lib.rs"], ["crates/optimus-store/src/lib.rs", "crates/optimus-runtime/tests/cancellation.rs"]),
        ("C-16", "campaign-job-consistency-and-recovery", "implemented", ["crates/optimus-runtime/src/campaign.rs", "crates/optimus-runtime/src/lib.rs", "crates/optimus-store/src/lib.rs"], ["crates/optimus-runtime/src/campaign.rs", "crates/optimus-store/src/lib.rs"]),
        ("C-17", "cron-gateway-claim-and-delivery", "implemented", ["crates/optimus-kernel/src/cron.rs", "crates/optimus-kernel/src/gateway.rs"], ["crates/optimus-kernel/src/cron.rs", "crates/optimus-kernel/src/gateway.rs", "apps/optimus-cli/tests/gateway_http.rs"]),
        ("C-18", "session-causality-around-durable-effects", "implemented", ["crates/optimus-store/src/lib.rs", "crates/optimus-runtime/src/lib.rs", "crates/optimus-kernel/src/lib.rs", "crates/optimus-kernel/src/session.rs"], ["crates/optimus-kernel/tests/session_resume.rs", "crates/optimus-kernel/tests/integrity_integration.rs"]),
    ]
    return {
        **generated_header(),
        "register": "docs/contracts/high-risk-contracts.md",
        "contracts": [
            {
                "id": cid,
                "name": name,
                "implementation_status": status,
                "sources": sources,
                "validated_by": tests,
            }
            for cid, name, status, sources, tests in contracts
        ],
    }


def build_evaluation_coverage() -> dict[str, Any]:
    path = ROOT / "crates/optimus-kernel/src/eval.rs"
    text = path.read_text(encoding="utf-8")
    start = text.find("pub fn builtin_suite")
    end = text.find("pub fn run_case")
    if start < 0 or end < 0:
        raise MemoryError("cannot locate built-in eval suite")
    ids = re.findall(
        r'EvalCase\s*\{\s*id:\s*"([^\"]+)"\.into\(\)',
        text[start:end],
        re.S,
    )
    if ids != ["offline-echo", "memory-then-answer", "pack-activate-browser", "write-file-job"]:
        raise MemoryError(f"unexpected built-in eval IDs: {ids}")
    required_start = text.find("pub const REQUIRED_INTEGRITY_EVALS")
    required_end = text.find("pub fn evaluate_integrity_observations")
    if required_start < 0 or required_end < 0:
        raise MemoryError("cannot locate required integrity eval catalog")
    integrity_ids = re.findall(r'"([a-z_]+)"', text[required_start:required_end])
    expected_integrity = [
        "sensitivity_denial",
        "smartdeny_approval",
        "route_policy_denial",
        "cooperative_cancellation",
        "stale_completion_fence",
        "gateway_dead_letter",
    ]
    if integrity_ids != expected_integrity:
        raise MemoryError(f"unexpected integrity eval IDs: {integrity_ids}")
    typed_path = ROOT / "crates/optimus-kernel/src/evaluation.rs"
    typed = typed_path.read_text(encoding="utf-8")
    required_typed_symbols = [
        "pub struct EvaluationDataset",
        "pub struct CandidateBinding",
        "pub struct EvaluationReportV1",
        "pub struct BaselineStore",
        "pub fn build_evaluation_report",
        "pub fn compare_evaluation_reports",
    ]
    missing = [symbol for symbol in required_typed_symbols if symbol not in typed]
    if missing:
        raise MemoryError(f"typed evaluation extraction is stale; missing symbols: {missing}")
    return {
        **generated_header(),
        "framework_status": "versioned_offline_evaluation_and_immutable_baselines",
        "builtin_cases": [
            {"id": case_id, "source": "crates/optimus-kernel/src/eval.rs"}
            for case_id in ids
        ],
        "integrity_cases": [
            {
                "id": case_id,
                "source": "crates/optimus-kernel/src/eval.rs",
                "executed_by": "crates/optimus-kernel/tests/integrity_integration.rs",
            }
            for case_id in integrity_ids
        ],
        "dimensions": {
            "canonical_tool_trace": "covered_by_builtin_cases_and_tests",
            "assistant_text": "exact_text_metric_in_versioned_dataset; legacy harness remains substring-based",
            "workflow_completion": "contract_adapter_and_cross_contract_tests",
            "security": "six_case_observed_integrity_suite_plus_focused_tests",
            "memory_precision_recall": "missing",
            "retrieval_relevance": "missing",
            "source_grounding": "missing",
            "citation_correctness": "missing",
            "cost": "checked_integer_mean",
            "latency": "checked_integer_mean",
            "replay": "fixture_replay_accuracy_plus_legacy_scripted_trajectories",
            "gpu_cpu_correctness": "not_applicable_no_gpu_component",
        },
        "typed_dataset": {
            "id": "priority2-integrity",
            "version": 1,
            "case_count": 10,
            "source": "crates/optimus-kernel/src/evaluation.rs",
            "validated_by": "crates/optimus-kernel/tests/evaluation_contracts.rs",
        },
        "metrics": ["exact_text", "tool_precision", "tool_recall", "terminal_accuracy", "replay_accuracy", "latency_millis", "cost_microunits"],
        "baseline_comparison": True,
        "version_binding": True,
        "candidate_bindings": ["source_tree_sha256", "contract_sha256", "tool_catalog_sha256", "route_policy_sha256", "provider", "model"],
    }


def parse_scalar(value: str) -> Any:
    value = value.strip()
    if value == "null":
        return None
    if value in {"true", "false"}:
        return value == "true"
    if (value.startswith('"') and value.endswith('"')) or (
        value.startswith("'") and value.endswith("'")
    ):
        return value[1:-1]
    return value


def parse_frontmatter(path: Path) -> dict[str, Any] | None:
    lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
    if not lines or lines[0].strip() != "---":
        return None
    try:
        end = lines.index("---", 1)
    except ValueError as exc:
        raise MemoryError(f"unterminated frontmatter: {relative(path)}") from exc
    result: dict[str, Any] = {}
    current: str | None = None
    for raw in lines[1:end]:
        if not raw.strip() or raw.lstrip().startswith("#"):
            continue
        item = re.match(r"^\s+-\s+(.+?)\s*$", raw)
        if item and current:
            if not isinstance(result.get(current), list):
                result[current] = []
            result[current].append(parse_scalar(item.group(1)))
            continue
        field = re.match(r"^([A-Za-z0-9_-]+):\s*(.*?)\s*$", raw)
        if not field:
            raise MemoryError(f"unsupported frontmatter line in {relative(path)}: {raw}")
        current = field.group(1)
        result[current] = [] if not field.group(2) else parse_scalar(field.group(2))
    return result


def expand_patterns(patterns: Iterable[str]) -> list[Path]:
    paths: set[Path] = set()
    for pattern in patterns:
        exact = ROOT / pattern
        if exact.is_file():
            candidates = [exact]
        elif any(token in pattern for token in ("*", "?", "[")):
            candidates = [path for path in repository_files() if fnmatch.fnmatch(relative(path), pattern)]
        else:
            candidates = list(ROOT.glob(pattern))
        for path in candidates:
            if path.is_file() and not is_excluded(path) and not is_sensitive(path):
                paths.add(path)
    return sorted(paths, key=relative)


def tree_for_paths(paths: Iterable[Path]) -> tuple[str, list[dict[str, Any]]]:
    records = [file_record(path) for path in paths]
    return records_tree_hash(records), records


def knowledge_documents() -> list[tuple[Path, dict[str, Any]]]:
    out = []
    for path in sorted((ROOT / "docs").rglob("*.md")):
        frontmatter = parse_frontmatter(path)
        if frontmatter and "knowledge_type" in frontmatter:
            out.append((path, frontmatter))
    return out


def build_change_impact() -> dict[str, Any]:
    documents = []
    reverse: dict[str, list[str]] = defaultdict(list)
    for path, frontmatter in knowledge_documents():
        covers = list(frontmatter.get("covers", []))
        depends = list(frontmatter.get("depends_on", []))
        validated = list(frontmatter.get("validated_by", []))
        resolved = expand_patterns(covers + depends)
        doc_rel = relative(path)
        for source in resolved:
            reverse[relative(source)].append(doc_rel)
        documents.append(
            {
                "document": doc_rel,
                "knowledge_type": frontmatter["knowledge_type"],
                "status": frontmatter.get("status"),
                "covers": covers,
                "depends_on": depends,
                "validated_by": validated,
                "resolved_source_count": len(resolved),
                "resolved_tests": [relative(item) for item in expand_patterns(validated)],
            }
        )
    return {
        **generated_header(),
        "documents": documents,
        "source_to_knowledge": {
            source: sorted(documents) for source, documents in sorted(reverse.items())
        },
        "algorithm": "frontmatter glob expansion; source or dependency changes affect every reverse-mapped document",
    }


def existing_staleness() -> dict[str, dict[str, Any]]:
    path = MEMORY_DIR / "knowledge-staleness.json"
    if not path.exists():
        return {}
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return {}
    return {item["document"]: item for item in data.get("documents", [])}


def build_knowledge_staleness(refresh: bool) -> dict[str, Any]:
    previous = existing_staleness()
    documents = []
    for path, frontmatter in knowledge_documents():
        covered = expand_patterns(list(frontmatter.get("covers", [])) + list(frontmatter.get("depends_on", [])))
        current_hash, records = tree_for_paths(covered)
        doc_rel = relative(path)
        old = previous.get(doc_rel)
        baseline_hash = current_hash if refresh or not old else old.get("verified_tree_sha256")
        documents.append(
            {
                "document": doc_rel,
                "knowledge_type": frontmatter["knowledge_type"],
                "knowledge_status": frontmatter.get("status"),
                "last_verified_commit": frontmatter.get("last_verified_commit"),
                "verification_basis": "sha256_tree_no_git" if not (ROOT / ".git").exists() else "git_and_sha256_tree",
                "verified_tree_sha256": baseline_hash,
                "current_tree_sha256": current_hash,
                "stale": baseline_hash != current_hash,
                "covered_files": records,
            }
        )
    return {
        **generated_header(),
        "documents": documents,
        "stale_count": sum(1 for item in documents if item["stale"]),
    }


def build_maps(refresh_staleness: bool) -> dict[str, dict[str, Any]]:
    metadata = cargo_metadata()
    return {
        "repository-index.json": build_repository_index(metadata),
        "agent-registry.json": build_agent_registry(),
        "tool-registry.json": parse_tool_catalog(),
        "workflow-registry.json": build_workflow_registry(),
        "prompt-registry.json": build_prompt_registry(),
        "model-registry.json": build_model_registry(),
        "dependency-graph.json": build_dependency_graph(metadata),
        "source-to-test-map.json": build_source_to_test_map(metadata),
        "contract-coverage.json": build_contract_coverage(),
        "evaluation-coverage.json": build_evaluation_coverage(),
        "change-impact.json": build_change_impact(),
        "knowledge-staleness.json": build_knowledge_staleness(refresh_staleness),
    }


def canonical_json(value: Any) -> str:
    return json.dumps(value, indent=2, sort_keys=True, ensure_ascii=False) + "\n"


def write_maps(maps: dict[str, dict[str, Any]]) -> None:
    MEMORY_DIR.mkdir(parents=True, exist_ok=True)
    for name in GENERATED_NAMES:
        (MEMORY_DIR / name).write_text(canonical_json(maps[name]), encoding="utf-8", newline="\n")


def local_link_problems(path: Path) -> list[str]:
    text = path.read_text(encoding="utf-8", errors="replace")
    problems = []
    for target in re.findall(r"\[[^\]]*\]\(([^)]+)\)", text):
        target = target.strip().split("#", 1)[0]
        if not target or re.match(r"^[a-z]+://", target) or target.startswith("mailto:"):
            continue
        decoded = target.replace("%20", " ")
        resolved = (path.parent / decoded).resolve()
        if not resolved.exists():
            problems.append(f"{relative(path)} -> {target}")
    return problems


def reference_problems(value: Any, key: str = "") -> list[str]:
    problems = []
    path_keys = {"source", "sources", "evidence", "validated_by", "source_path", "manifest_path"}
    if isinstance(value, dict):
        for child_key, child in value.items():
            if child_key in path_keys:
                values = child if isinstance(child, list) else [child]
                for item in values:
                    if not isinstance(item, str):
                        continue
                    if "::" in item or item.startswith("OPTIMUS_") or item.startswith("JobBudget"):
                        continue
                    if any(token in item for token in ("*", "?", "[")):
                        if not expand_patterns([item]):
                            problems.append(f"missing referenced glob {item}")
                        continue
                    candidate = ROOT / item
                    if ("/" in item or "\\" in item) and not candidate.exists():
                        problems.append(f"missing referenced path {item}")
            problems.extend(reference_problems(child, child_key))
    elif isinstance(value, list):
        for child in value:
            problems.extend(reference_problems(child, key))
    return problems


def adr_warnings() -> list[str]:
    warnings = []
    required = (
        "Context",
        "Decision",
        "Alternatives considered",
        "Reasons",
        "Consequences",
        "Risks",
        "Evaluation evidence",
        "Conditions for reconsideration",
        "Relevant code",
        "Relevant tests",
    )
    numbers: dict[str, list[str]] = defaultdict(list)
    for path in sorted((ROOT / "docs/decisions").glob("*.md")):
        number = path.name.split("-", 1)[0]
        if not number.isdigit() or int(number) < 17:
            continue
        numbers[number].append(path.name)
        text = path.read_text(encoding="utf-8", errors="replace")
        missing = [section for section in required if f"## {section}" not in text]
        if missing:
            warnings.append(f"ADR fields missing in {relative(path)}: {', '.join(missing)}")
    for number, files in numbers.items():
        if len(files) > 1:
            warnings.append(f"duplicate ADR number {number}: {', '.join(files)}")
    return warnings


def current_architecture_semantic_errors(
    workflows: list[dict[str, Any]], contracts: list[dict[str, Any]]
) -> list[str]:
    """Reject generated/current claims that contradict ADR-0019/ADR-0020 invariants."""
    errors: list[str] = []
    workflow_by_id = {row.get("id"): row for row in workflows}
    campaign = workflow_by_id.get("durable-campaign")
    if campaign is None:
        errors.append("missing durable-campaign workflow")
    else:
        if campaign.get("dependencies") != ["work-graph-job", "optimus.db"]:
            errors.append("durable-campaign must depend on work-graph-job and optimus.db")
        if "schema v4" not in str(campaign.get("validation", "")):
            errors.append("durable-campaign must declare campaign schema v4 validation")
        if "job-derived" not in str(campaign.get("state_transitions", "")):
            errors.append("durable-campaign state must be job-derived")
        if any("stores can diverge" in str(item) for item in campaign.get("failure", [])):
            errors.append("durable-campaign resurrects superseded split-store divergence")

    contract_by_id = {row.get("id"): row for row in contracts}
    required_status = {
        "C-01": "implemented",
        "C-02": "implemented",
        "C-03": "implemented",
        "C-04": "implemented",
        "C-05": "implemented",
        "C-06": "implemented",
        "C-15": "implemented",
        "C-16": "implemented",
    }
    for contract_id, status in required_status.items():
        actual = contract_by_id.get(contract_id, {}).get("implementation_status")
        if actual != status:
            errors.append(f"{contract_id} must be {status}, got {actual}")

    stale_claims = [
        "runtime path confinement is not handle-relative",
        "runtime path checks are not handle-relative",
        "write secret policy is not unified",
        "runtime writes also do not apply",
        "campaign/job handoffs are not atomic",
        "approval is durable and job-scoped",
        "approval scope is the job id",
        "development http mode and the loopback gateway are unauthenticated",
        "desktop http mode has no authentication",
        "later job/node projection updates and event appends are separate",
        "concurrent campaign ownership/leases are not defined",
    ]
    for rel in (
        "docs/architecture/system-overview.md",
        "docs/maps/security-and-approvals.md",
    ):
        text = (ROOT / rel).read_text(encoding="utf-8").lower()
        for claim in stale_claims:
            if claim in text:
                errors.append(f"superseded ADR-0019 claim in {rel}: {claim}")
    return errors


def validate_maps(strict: bool = False) -> dict[str, Any]:
    errors: list[str] = []
    warnings: list[str] = []
    expected = build_maps(refresh_staleness=True)
    loaded: dict[str, Any] = {}
    for name in GENERATED_NAMES:
        path = MEMORY_DIR / name
        if not path.exists():
            errors.append(f"missing generated file {relative(path)}")
            continue
        try:
            loaded[name] = json.loads(path.read_text(encoding="utf-8"))
        except json.JSONDecodeError as exc:
            errors.append(f"invalid JSON {relative(path)}: {exc}")
            continue
        if loaded[name] != expected[name]:
            errors.append(f"generated file drift: {relative(path)} (run generate; do not edit manually)")
        if loaded[name].get("generated_by") != GENERATOR or loaded[name].get("do_not_edit") is not True:
            errors.append(f"missing generated marker: {relative(path)}")

    for rel in IMPORTANT_DOCS:
        path = ROOT / rel
        if not path.exists():
            errors.append(f"missing important Engineering Memory document {rel}")
            continue
        frontmatter = parse_frontmatter(path)
        if not frontmatter:
            errors.append(f"missing frontmatter: {rel}")
            continue
        for field in ("knowledge_type", "status", "covers", "depends_on", "validated_by", "last_verified_commit"):
            if field not in frontmatter:
                errors.append(f"frontmatter missing {field}: {rel}")
        if frontmatter.get("status") not in {"current", "planned", "historical", "stale"}:
            errors.append(f"invalid frontmatter status in {rel}: {frontmatter.get('status')}")
        covered = expand_patterns(frontmatter.get("covers", []))
        if not covered:
            errors.append(f"frontmatter covers no files: {rel}")

    for path in sorted((ROOT / "docs").rglob("*.md")):
        errors.extend(f"broken local link: {problem}" for problem in local_link_problems(path))

    if loaded:
        for name, data in loaded.items():
            for problem in sorted(set(reference_problems(data))):
                errors.append(f"{name}: {problem}")

    tools = loaded.get("tool-registry.json", expected["tool-registry.json"]).get("tools", [])
    tool_ids = [item["id"] for item in tools]
    if len(tool_ids) != len(set(tool_ids)):
        errors.append("duplicate tool identifiers")
    for tool in tools:
        for field in (
            "id", "version", "owner", "description", "input_schema", "output_schema",
            "side_effects", "permissions", "timeout", "supports_cancellation", "retry",
            "idempotency", "determinism", "replay", "error_taxonomy", "logging", "validated_by",
        ):
            if field not in tool:
                errors.append(f"tool {tool.get('id')} missing field {field}")
        if tool["available"] and tool["input_schema"] is None:
            errors.append(f"available tool {tool['id']} missing input schema")
        if tool["available"] and tool["output_schema"].get("envelope_schema") is None:
            warnings.append(f"available tool {tool['id']} lacks canonical ToolOutcome envelope")
        if tool["policy"] == "process" and tool["available"] and tool["approval"].get("status") != "required":
            errors.append(f"high-risk process tool {tool['id']} lacks approval requirement")

    for registry_name, array_name in (
        ("agent-registry.json", "agents"),
        ("workflow-registry.json", "workflows"),
        ("prompt-registry.json", "prompts"),
    ):
        rows = loaded.get(registry_name, expected[registry_name]).get(array_name, [])
        ids = [row.get("id") for row in rows]
        if len(ids) != len(set(ids)):
            errors.append(f"duplicate IDs in {registry_name}")
    agent_registry = loaded.get("agent-registry.json", expected["agent-registry.json"])
    agents = agent_registry["agents"]
    if not agents and agent_registry.get("contract_substrate", {}).get("status") != "implemented":
        warnings.append("no implemented specialist agents or universal agent schema")
    workflows = loaded.get("workflow-registry.json", expected["workflow-registry.json"])["workflows"]
    for workflow in workflows:
        if workflow.get("cancellation", {}).get("status") != "implemented":
            warnings.append(f"workflow {workflow['id']} lacks implemented cancellation contract")
        if not workflow.get("completion") or not workflow.get("failure"):
            errors.append(f"workflow {workflow['id']} lacks terminal outcome declarations")

    contracts = loaded.get("contract-coverage.json", expected["contract-coverage.json"])["contracts"]
    errors.extend(current_architecture_semantic_errors(workflows, contracts))

    warnings.extend(adr_warnings())
    staleness = loaded.get("knowledge-staleness.json", expected["knowledge-staleness.json"])
    for document in staleness.get("documents", []):
        if document.get("stale"):
            errors.append(f"stale Engineering Memory: {document['document']}")

    gpu_sources = [
        relative(path)
        for path in repository_files()
        if path.suffix == ".rs" and re.search(r"(?:^|[-_/])(gpu|cuda)(?:[-_/]|$)", relative(path), re.I)
    ]
    if gpu_sources:
        warnings.append(f"GPU source requires CPU-fallback declaration review: {gpu_sources}")

    if strict and warnings:
        errors.extend(f"strict: {warning}" for warning in warnings)
    return {
        "ok": not errors,
        "errors": sorted(set(errors)),
        "warnings": sorted(set(warnings)),
        "generated_files": len(GENERATED_NAMES),
        "tool_count": len(tools),
        "available_tool_count": sum(1 for tool in tools if tool["available"]),
        "workflow_count": len(workflows),
        "agent_count": len(agents),
    }


def check_staleness() -> dict[str, Any]:
    current = build_knowledge_staleness(refresh=False)
    old_index_path = MEMORY_DIR / "repository-index.json"
    changed_files: list[str] = []
    if old_index_path.exists():
        try:
            old = json.loads(old_index_path.read_text(encoding="utf-8"))
            old_files = {item["path"]: item["sha256"] for item in old.get("files", [])}
            new_files = {item["path"]: item["sha256"] for item in build_repository_index(cargo_metadata())["files"]}
            changed_files = sorted(
                path for path in set(old_files) | set(new_files) if old_files.get(path) != new_files.get(path)
            )
        except (OSError, json.JSONDecodeError, MemoryError) as exc:
            changed_files = [f"unable to compare repository index: {exc}"]
    else:
        changed_files = ["no generated repository baseline"]
    stale_documents = [item["document"] for item in current["documents"] if item["stale"]]
    impact = build_change_impact()
    affected: dict[str, list[str]] = {}
    reverse = impact["source_to_knowledge"]
    for path in changed_files:
        if path in reverse:
            affected[path] = reverse[path]
    return {
        "ok": not stale_documents and not changed_files,
        "changed_files": changed_files,
        "stale_documents": stale_documents,
        "affected_knowledge": affected,
    }


def print_result(result: dict[str, Any], as_json: bool) -> None:
    if as_json:
        print(canonical_json(result), end="")
        return
    if "errors" in result:
        print("ENGINEERING_MEMORY_VALID" if result["ok"] else "ENGINEERING_MEMORY_INVALID")
        for error in result["errors"]:
            print(f"ERROR: {error}")
        for warning in result["warnings"]:
            print(f"WARNING: {warning}")
        if "generated_files" in result:
            print(
                f"generated={result['generated_files']} agents={result['agent_count']} "
                f"tools={result['tool_count']} available_tools={result['available_tool_count']} "
                f"workflows={result['workflow_count']}"
            )
    else:
        print("ENGINEERING_MEMORY_CURRENT" if result["ok"] else "ENGINEERING_MEMORY_STALE")
        for path in result.get("changed_files", []):
            print(f"CHANGED: {path}")
        for document in result.get("stale_documents", []):
            print(f"STALE: {document}")
        for path, documents in result.get("affected_knowledge", {}).items():
            print(f"IMPACT: {path} -> {', '.join(documents)}")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("generate", "check", "validate"))
    parser.add_argument("--strict", action="store_true", help="treat known gaps/warnings as failures")
    parser.add_argument("--json", action="store_true", help="emit machine-readable result")
    args = parser.parse_args(argv)
    try:
        if args.command == "generate":
            maps = build_maps(refresh_staleness=True)
            write_maps(maps)
            result = validate_maps(strict=args.strict)
        elif args.command == "check":
            result = check_staleness()
        else:
            result = validate_maps(strict=args.strict)
    except (MemoryError, OSError, subprocess.SubprocessError) as exc:
        result = {"ok": False, "errors": [str(exc)], "warnings": []}
    print_result(result, args.json)
    return 0 if result.get("ok") else 1


if __name__ == "__main__":
    raise SystemExit(main())
