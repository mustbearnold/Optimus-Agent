#!/usr/bin/env python3
"""Generate and validate repository-local Optimus Engineering Memory indexes.

Engineering Memory is a three-plane system:
1. Authority plane — curated docs/skills/laws
2. Fact plane — compact deterministic generated indexes
3. Lens plane — budgeted query views for agents (`context`, `impact`, ...)

The generator intentionally uses only Python's standard library plus `cargo
metadata`. It treats Rust source as canonical for the current tool catalog and
fails closed when the expected catalog shape cannot be reconciled.

Agents should prefer query lenses over loading raw generated JSON into prompts.
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
import time
from collections import Counter, defaultdict
from pathlib import Path
from typing import Any, Iterable

ROOT = Path(__file__).resolve().parents[1]
MEMORY_DIR = ROOT / ".engineering-memory"
HASH_CACHE_PATH = MEMORY_DIR / ".hash-cache.json"
GENERATOR = "scripts/engineering_memory.py"
SCHEMA_VERSION = 2
DEFAULT_CONTEXT_BUDGET_TOKENS = 3000
GENERATED_NAMES = (
    "manifest.json",
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
COMMANDS = (
    "generate",
    "check",
    "validate",
    "binding",
    "impact",
    "stale",
    "tools",
    "owner",
    "context",
    "report",
    "stat",
)
# Must stay a superset of the generated/ignored directories in .gitignore.
# Anything gitignored that lands here would make Engineering Memory stale as a
# side effect of running the test suites, which makes the staleness gate
# unsatisfiable: run tests -> artifacts appear -> EM stale -> gate fails.
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
    "test-results",
    "playwright-report",
}
EXCLUDED_SUFFIXES = {
    ".tsbuildinfo",
    ".pyc",
    ".pyo",
    ".pdb",
    ".rs.bk",
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


def canonical_file_bytes(path: Path) -> bytes:
    data = path.read_bytes()
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError:
        return data
    if "\0" in text:
        return data
    return text.replace("\r\n", "\n").replace("\r", "\n").encode("utf-8")


def sha256_file(path: Path) -> str:
    return sha256_bytes(canonical_file_bytes(path))


_HASH_CACHE: dict[str, Any] | None = None
_HASH_CACHE_DIRTY = False


def _load_hash_cache() -> dict[str, Any]:
    global _HASH_CACHE
    if _HASH_CACHE is not None:
        return _HASH_CACHE
    if HASH_CACHE_PATH.exists():
        try:
            payload = json.loads(HASH_CACHE_PATH.read_text(encoding="utf-8"))
            if isinstance(payload, dict) and isinstance(payload.get("entries"), dict):
                _HASH_CACHE = payload
                return _HASH_CACHE
        except (OSError, json.JSONDecodeError):
            pass
    _HASH_CACHE = {"version": 1, "entries": {}}
    return _HASH_CACHE


def _save_hash_cache() -> None:
    global _HASH_CACHE_DIRTY
    if not _HASH_CACHE_DIRTY or _HASH_CACHE is None:
        return
    MEMORY_DIR.mkdir(parents=True, exist_ok=True)
    HASH_CACHE_PATH.write_text(
        json.dumps(_HASH_CACHE, sort_keys=True, separators=(",", ":")) + "\n",
        encoding="utf-8",
        newline="\n",
    )
    _HASH_CACHE_DIRTY = False


def _file_cache_fingerprint(path: Path) -> str:
    stat = path.stat()
    return f"{stat.st_mtime_ns}:{stat.st_size}:{getattr(stat, 'st_ino', 0)}"


def generated_header() -> dict[str, Any]:
    return {
        "schema_version": SCHEMA_VERSION,
        "generated_by": GENERATOR,
        "do_not_edit": True,
    }


def estimate_tokens(text: str) -> int:
    return max(1, (len(text) + 3) // 4)


def ownership_patterns(frontmatter: dict[str, Any]) -> list[str]:
    """Patterns whose source changes make the document stale."""
    if "owns" in frontmatter and frontmatter["owns"] is not None:
        owns = frontmatter["owns"]
        return list(owns) if isinstance(owns, list) else [owns]
    covers = frontmatter.get("covers", [])
    return list(covers) if isinstance(covers, list) else [covers]


def watch_patterns(frontmatter: dict[str, Any]) -> list[str]:
    """Patterns that warrant inspection but do not auto-stale."""
    watches = frontmatter.get("watches", [])
    if watches is None:
        return []
    return list(watches) if isinstance(watches, list) else [watches]


def depends_patterns(frontmatter: dict[str, Any]) -> list[str]:
    depends = frontmatter.get("depends_on", [])
    if depends is None:
        return []
    return list(depends) if isinstance(depends, list) else [depends]


def validated_patterns(frontmatter: dict[str, Any]) -> list[str]:
    validated = frontmatter.get("validated_by", [])
    if validated is None:
        return []
    return list(validated) if isinstance(validated, list) else [validated]


def pattern_matches(path: str, pattern: str) -> bool:
    if path == pattern:
        return True
    return fnmatch.fnmatch(path, pattern)


def is_excluded(path: Path) -> bool:
    try:
        parts = path.relative_to(ROOT).parts
    except ValueError:
        return True
    if any(part in EXCLUDED_PARTS for part in parts):
        return True
    name = path.name.lower()
    if name.endswith(tuple(EXCLUDED_SUFFIXES)):
        return True
    return False


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
            if is_sensitive(path) or is_excluded(path):
                continue
            out.append(path)
    return tuple(sorted(out, key=relative))


def file_record(path: Path) -> dict[str, Any]:
    global _HASH_CACHE_DIRTY
    rel = relative(path)
    fingerprint = _file_cache_fingerprint(path)
    cache = _load_hash_cache()
    entries = cache.setdefault("entries", {})
    hit = entries.get(rel)
    if (
        isinstance(hit, dict)
        and hit.get("fingerprint") == fingerprint
        and isinstance(hit.get("record"), dict)
        and hit["record"].get("path") == rel
    ):
        return dict(hit["record"])

    data = canonical_file_bytes(path)
    try:
        lines = len(data.decode("utf-8").splitlines())
    except UnicodeDecodeError:
        lines = None
    record = {
        "path": rel,
        "sha256": sha256_bytes(data),
        "bytes": len(data),
        "lines": lines,
        "language": LANGUAGE_BY_SUFFIX.get(path.suffix.lower(), "Other"),
    }
    entries[rel] = {"fingerprint": fingerprint, "record": record}
    _HASH_CACHE_DIRTY = True
    return dict(record)


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


def build_repository_index(metadata: dict[str, Any]) -> dict[str, Any]:
    records = [file_record(path) for path in repository_files()]
    language_counts: dict[str, dict[str, int]] = defaultdict(lambda: {"files": 0, "bytes": 0})
    for record in records:
        bucket = language_counts[record["language"]]
        bucket["files"] += 1
        bucket["bytes"] += record["bytes"]
    indexed_paths = [record["path"] for record in records]
    top_entries = sorted({path.split("/", 1)[0] for path in indexed_paths})
    independent = []
    for manifest in (path for path in repository_files() if path.name == "Cargo.toml"):
        if manifest == ROOT / "Cargo.toml" or is_excluded(manifest):
            continue
        text = manifest.read_text(encoding="utf-8", errors="replace")
        if re.search(r"(?m)^\[workspace\]\s*$", text):
            independent.append(relative(manifest.parent))
    domains = {}
    for name in ("agents", "workflows", "tools", "prompts", "evals", "fixtures", "packages"):
        domains[name] = any(path == name or path.startswith(f"{name}/") for path in indexed_paths)
    return {
        **generated_header(),
        "verification_basis": "sha256_tree",
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


def build_priority2_candidate_binding() -> dict[str, str]:
    repository = build_repository_index(cargo_metadata())
    records = {record["path"]: record["sha256"] for record in repository["files"]}
    authorities = {
        "contract_sha256": "crates/optimus-eval/src/evaluation.rs",
        "tool_catalog_sha256": "crates/optimus-packs/src/lib.rs",
        "route_policy_sha256": "crates/optimus-kernel/src/routing.rs",
    }
    missing = sorted(path for path in authorities.values() if path not in records)
    if missing:
        raise MemoryError(f"candidate binding authorities are not indexed: {missing}")
    return {
        "source_tree_sha256": repository["tree_sha256"],
        **{field: records[path] for field, path in authorities.items()},
        "provider": "offline",
        "model": "offline-scripted",
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


TOOL_TEMPLATE_FIELDS = (
    "side_effects",
    "permissions",
    "timeout",
    "supports_cancellation",
    "cancellation_status",
    "retry",
    "idempotency",
    "determinism",
    "replay",
    "observability_contract",
    "approval",
    "error_taxonomy",
    "logging",
    "output_schema",
)


def compress_tool_registry(registry: dict[str, Any]) -> dict[str, Any]:
    """Factor repeated operational envelopes into templates."""
    templates: dict[str, dict[str, Any]] = {}
    template_ids: dict[str, str] = {}
    compressed_tools: list[dict[str, Any]] = []
    for tool in registry.get("tools", []):
        operational = {field: tool[field] for field in TOOL_TEMPLATE_FIELDS if field in tool}
        fingerprint = sha256_bytes(canonical_json(operational).encode("utf-8"))[:16]
        template_id = template_ids.get(fingerprint)
        if template_id is None:
            template_id = f"op_{len(templates) + 1:02d}_{fingerprint}"
            template_ids[fingerprint] = template_id
            templates[template_id] = operational
        slim = {key: value for key, value in tool.items() if key not in TOOL_TEMPLATE_FIELDS}
        slim["template"] = template_id
        compressed_tools.append(slim)
    out = dict(registry)
    out["templates"] = dict(sorted(templates.items()))
    out["tools"] = compressed_tools
    out["storage"] = "templated_v2"
    return out


def expand_tool_registry(registry: dict[str, Any]) -> dict[str, Any]:
    """Expand templated tool records for validation and lenses."""
    templates = registry.get("templates") or {}
    if not templates:
        return registry
    expanded = []
    for tool in registry.get("tools", []):
        row = dict(tool)
        template_id = row.pop("template", None)
        if template_id:
            if template_id not in templates:
                raise MemoryError(f"tool {row.get('id')} references missing template {template_id}")
            merged = dict(templates[template_id])
            merged.update(row)
            expanded.append(merged)
        else:
            expanded.append(row)
    out = dict(registry)
    out["tools"] = expanded
    return out


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
    # Core available tools grow with product programs (P21: 10, P22: 14 with mutate).
    if len(invocation_ids) < 10:
        raise MemoryError(
            f"expected at least 10 available ToolInvocation IDs, found {len(invocation_ids)}"
        )

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
                "effector_owner": (
                    "optimus-runtime"
                    if tool_id
                    in {
                        "write_file",
                        "mkdir",
                        "delete_path",
                        "rename_path",
                        "patch_file",
                        "terminal",
                    }
                    else "optimus-kernel"
                ),
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
        # Catalog grows with product programs (P21: 22, P22: 26 with mutate tools).
        if len(ids) < 22 or len(ids) != len(set(ids)):
            raise MemoryError(
                f"expected ≥22 unique canonical tools, found {len(ids)}/{len(set(ids))}"
            )
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
    contract = ROOT / "crates/optimus-agent/src/lib.rs"
    vertical = ROOT / "crates/optimus-workflow/src/specialist_vertical.rs"
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
    if not vertical.is_file():
        raise MemoryError("specialist vertical module is missing")
    vertical_text = vertical.read_text(encoding="utf-8")
    for symbol in (
        "pub fn workspace_writer_descriptor",
        "pub fn workspace_reader_descriptor",
        "pub fn write_file_handoff_workflow",
        "pub fn read_file_handoff_workflow",
        "pub fn write_then_read_handoff_workflow",
        "pub fn run_write_file_handoff",
        "pub fn run_read_file_handoff",
        "pub fn run_write_then_read_handoff",
        "pub fn run_registered_workflow",
        'WORKSPACE_WRITER_ID: &str = "workspace_writer"',
        'WORKSPACE_READER_ID: &str = "workspace_reader"',
    ):
        if symbol not in vertical_text:
            raise MemoryError(f"specialist vertical is stale; missing {symbol}")
    run_module = ROOT / "crates/optimus-workflow/src/workflow_run.rs"
    if not run_module.is_file():
        raise MemoryError("workflow_run module is missing")
    run_text = run_module.read_text(encoding="utf-8")
    for symbol in (
        "pub struct WorkflowRunStore",
        "pub fn claim_lease",
        "pub fn ready_nodes",
        "pub fn settle_terminal",
    ):
        if symbol not in run_text:
            raise MemoryError(f"workflow_run is stale; missing {symbol}")
    agents = [
        {
            "id": "workspace_writer",
            "version": "1.0.0",
            "status": "implemented",
            "owner": "optimus-agent",
            "responsibility": "Write a single relative-path workspace file through durable SmartDeny effects",
            "required_tools": ["write_file"],
            "permissions": {"filesystem_roots": ["workspace"], "effects": ["write_file"]},
            "source": relative(vertical),
            "validated_by": [
                "crates/optimus-kernel/tests/specialist_vertical.rs",
                "crates/optimus-kernel/tests/workflow_dag.rs",
            ],
            "workflow": "write_file_handoff@1.0.0",
        },
        {
            "id": "workspace_reader",
            "version": "1.0.0",
            "status": "implemented",
            "owner": "optimus-agent",
            "responsibility": "Read a single relative-path workspace file and publish a content-addressed handoff artifact",
            "required_tools": ["read_file"],
            "permissions": {"filesystem_roots": ["workspace"], "effects": ["read_file"]},
            "source": relative(vertical),
            "validated_by": ["crates/optimus-kernel/tests/workflow_dag.rs"],
            "workflow": "read_file_handoff@1.0.0",
        },
    ]
    return {
        **generated_header(),
        "agents": agents,
        "implemented_specialist_agent_count": len(agents),
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
                "durable workflow run ledger and registered DAG scheduler",
            ],
            "validated_by": [
                "crates/optimus-kernel/tests/agent_contracts.rs",
                "crates/optimus-kernel/tests/specialist_vertical.rs",
                "crates/optimus-kernel/tests/workflow_dag.rs",
                "crates/optimus-eval/tests/integrity_integration.rs",
            ],
        },
        "status": "implemented_multi_agent_dag_verticals",
        "statement": "Typed agent contracts plus two built-in specialists (workspace_writer, workspace_reader) executed by registered handoff workflows and a durable DAG run store.",
        "not_agents": [
            {"symbol": "optimus_kernel::ModelProvider", "reason": "provider adapter interface"},
            {"symbol": "optimus_kernel::ScriptedModel", "reason": "offline/test model adapter"},
            {"symbol": "optimus_runtime::CampaignStep", "reason": "deterministic effect step, not typed agent invocation"},
        ],
        "required_before_next_agent": [
            "purpose and non-responsibilities",
            "overlap/routing check against workspace_writer",
            "evaluation case for the specialist path",
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
            validated_by=["crates/optimus-kernel/tests/kernel_turn.rs", "crates/optimus-eval/src/eval.rs"],
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
            id="write-file-handoff",
            owner="optimus-workflow",
            trigger="CLI/API run_write_file_handoff after seeding registries",
            inputs=["relative_path", "contents", "policy", "auto_grant"],
            outputs=["WriteFileHandoffReport", "workspace file", "handoff artifact", "agent invocation terminal", "workflow run id"],
            stages=[
                "seed agent+workflow registries",
                "begin durable workflow run",
                "begin workspace_writer invocation as child",
                "create WriteFile Work Graph job",
                "SmartDeny gate or auto-grant",
                "link exact effect provenance",
                "publish content-addressed artifact",
                "settle one agent terminal; settle run terminal",
            ],
            dependencies=["work-graph-job", "workspace_writer specialist", "artifact store", "workflow-runs.db"],
            state_transitions="workflow run + agent invocation running→terminal; job pending→awaiting_approval|succeeded|failed|cancelled",
            timeout="workflow node timeout_ms (60s default)",
            approvals="SmartDeny exact-effect grant for WriteFile unless unrestricted or auto_grant",
            cancellation={"status": "implemented", "contract": "run cancel fans out to child invocations/jobs; late success fenced"},
            validation="path shape preflight; immutable registry descriptors; effect provenance match",
            completion=["succeeded", "failed", "cancelled"],
            failure=["approval_required without grant", "effect failure", "invalid path/contents"],
            observability=["workflow run events", "agent invocation events", "job events", "artifact index"],
            validated_by=["crates/optimus-kernel/tests/specialist_vertical.rs", "crates/optimus-kernel/tests/workflow_dag.rs"],
            source=["crates/optimus-workflow/src/specialist_vertical.rs", "crates/optimus-workflow/src/workflow_run.rs"],
        ),
        workflow_record(
            id="read-file-handoff",
            owner="optimus-workflow",
            trigger="CLI/API run_read_file_handoff",
            inputs=["relative_path"],
            outputs=["WorkflowDagReport", "handoff artifact", "agent invocation terminal"],
            stages=[
                "seed registries",
                "begin workflow run",
                "workspace_reader reads bounded workspace file",
                "publish content-addressed artifact",
                "settle agent + run terminals",
            ],
            dependencies=["workspace_reader specialist", "artifact store", "workflow-runs.db"],
            state_transitions="workflow run + agent invocation running→terminal",
            timeout="workflow node timeout_ms (60s default)",
            approvals="none (read-only specialist)",
            cancellation={"status": "implemented", "contract": "run cancel and child invocation cancel fence"},
            validation="path shape preflight; file must exist; read size bound",
            completion=["succeeded", "failed", "cancelled"],
            failure=["file_not_found", "read_too_large", "invalid path"],
            observability=["workflow run events", "agent invocation events", "artifact index"],
            validated_by=["crates/optimus-kernel/tests/workflow_dag.rs"],
            source=["crates/optimus-workflow/src/specialist_vertical.rs"],
        ),
        workflow_record(
            id="write-then-read-handoff",
            owner="optimus-workflow",
            trigger="CLI/API run_write_then_read_handoff / run_registered_workflow",
            inputs=["relative_path", "contents", "policy", "auto_grant"],
            outputs=["WorkflowDagReport", "two handoff artifacts", "two child invocations"],
            stages=[
                "begin durable DAG run with lease",
                "execute write node (workspace_writer)",
                "when write succeeded, execute read node (workspace_reader)",
                "settle exactly one run terminal",
            ],
            dependencies=["write-file-handoff stages", "read-file-handoff stages", "WorkflowRunStore"],
            state_transitions="node pending→running→succeeded|failed|cancelled; run terminal uniqueness",
            timeout="per-node timeout_ms",
            approvals="SmartDeny on write node only",
            cancellation={"status": "implemented", "contract": "parent run cancel blocks new children and fans out to linked invocations/jobs"},
            validation="registered definition only; topological readiness; no model-free DAG",
            completion=["succeeded", "failed", "cancelled"],
            failure=["approval_required", "node failure", "cancel"],
            observability=["workflow_run_events", "per-node projections", "child links"],
            validated_by=["crates/optimus-kernel/tests/workflow_dag.rs"],
            source=["crates/optimus-workflow/src/specialist_vertical.rs", "crates/optimus-workflow/src/workflow_run.rs"],
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
            validated_by=["crates/optimus-ops/src/cron.rs"],
            source=["crates/optimus-ops/src/cron.rs", "apps/optimus-cli/src/main.rs"],
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
            validated_by=["crates/optimus-ops/src/gateway.rs", "apps/optimus-cli/tests/gateway_http.rs"],
            source=["crates/optimus-ops/src/gateway.rs", "apps/optimus-cli/src/gateway_http.rs"],
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
            validated_by=["crates/optimus-kernel/tests/workflow_contracts.rs", "crates/optimus-eval/tests/integrity_integration.rs"],
            source=["crates/optimus-workflow/src/workflow.rs"],
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
                "evidence": ["crates/optimus-kernel/src/lib.rs", "crates/optimus-eval/src/eval.rs"],
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
    execution_source = (ROOT / "crates/optimus-kernel/src/execution.rs").read_text(
        encoding="utf-8"
    )
    kernel_source = (ROOT / "crates/optimus-kernel/src/lib.rs").read_text(
        encoding="utf-8"
    )
    for symbol in ["pub fn begin_traced", "execution_trace_links"]:
        if symbol not in execution_source:
            raise MemoryError(f"execution trace authority is missing {symbol}")
    for symbol in [
        "pub trace_context: TraceContext",
        "interrupted execution manifest is missing trace evidence",
    ]:
        if symbol not in kernel_source:
            raise MemoryError(f"kernel trace authority is missing {symbol}")
    contracts = [
        ("C-01", "cancellation", "implemented", ["crates/optimus-store/src/lib.rs", "crates/optimus-runtime/src/lib.rs", "crates/optimus-runtime/src/campaign.rs"], ["crates/optimus-runtime/tests/cancellation.rs", "crates/optimus-runtime/src/campaign.rs"]),
        ("C-02", "exactly-one-terminal-outcome", "implemented", ["crates/optimus-store/src/lib.rs", "crates/optimus-graph/src/lib.rs"], ["crates/optimus-store/src/lib.rs", "crates/optimus-runtime/tests/cancellation.rs"]),
        ("C-03", "action-bound-approval", "implemented", ["crates/optimus-runtime/src/lib.rs", "crates/optimus-store/src/lib.rs"], ["crates/optimus-runtime/tests/approvals_surface.rs", "crates/optimus-runtime/tests/skill_bridge.rs"]),
        ("C-04", "runtime-filesystem-confinement", "implemented", ["crates/optimus-runtime/src/lib.rs", "crates/optimus-kernel/src/fs_sandbox.rs"], ["crates/optimus-runtime/tests/path_confinement.rs", "crates/optimus-kernel/tests/kernel_turn.rs"]),
        ("C-05", "loopback-api-authorization", "implemented", ["apps/optimus-desktop/src/server.rs", "apps/optimus-cli/src/gateway_http.rs"], ["apps/optimus-desktop/src/server.rs", "apps/optimus-cli/tests/gateway_http.rs", "apps/optimus-desktop/e2e"]),
        ("C-06", "fail-closed-workflow-decoding", "implemented", ["crates/optimus-runtime/src/campaign.rs"], ["crates/optimus-runtime/src/campaign.rs"]),
        ("C-07", "provider-call-envelope-and-batch-authorization", "implemented", ["crates/optimus-kernel/src/lib.rs", "crates/optimus-kernel/src/openai_compat.rs", "crates/optimus-kernel/src/codex_oauth.rs", "crates/optimus-packs/src/lib.rs"], ["crates/optimus-kernel/tests/kernel_turn.rs", "crates/optimus-kernel/tests/codex_oauth.rs", "crates/optimus-packs/tests/packs_budget.rs"]),
        ("C-08", "canonical-tool-result", "implemented", ["crates/optimus-packs/src/lib.rs", "crates/optimus-kernel/src/lib.rs", "crates/optimus-kernel/src/execution.rs"], ["crates/optimus-packs/tests/packs_budget.rs", "crates/optimus-kernel/tests/kernel_turn.rs", "crates/optimus-kernel/tests/session_resume.rs"]),
        ("C-09", "agent-lifecycle", "implemented", ["crates/optimus-agent/src/lib.rs"], ["crates/optimus-kernel/tests/agent_contracts.rs", "crates/optimus-eval/tests/integrity_integration.rs"]),
        ("C-10", "workflow-lifecycle", "implemented", ["crates/optimus-workflow/src/workflow.rs", "crates/optimus-runtime/src/campaign.rs", "crates/optimus-ops/src/cron.rs", "crates/optimus-ops/src/gateway.rs"], ["crates/optimus-kernel/tests/workflow_contracts.rs", "crates/optimus-eval/tests/integrity_integration.rs"]),
        ("C-11", "model-routing", "implemented", ["crates/optimus-kernel/src/routing.rs", "apps/optimus-cli/src/main.rs", "apps/optimus-desktop/src/ipc/chat.rs"], ["crates/optimus-kernel/src/routing.rs", "crates/optimus-eval/tests/integrity_integration.rs"]),
        ("C-12", "credential-and-local-transport-security", "implemented", ["crates/optimus-kernel/src/credential.rs", "crates/optimus-kernel/src/codex_oauth.rs", "apps/optimus-desktop/src/server.rs"], ["crates/optimus-kernel/tests/codex_oauth.rs", "apps/optimus-cli/tests/gateway_http.rs"]),
        ("C-13", "deterministic-replay-and-provenance", "implemented", ["crates/optimus-kernel/src/execution.rs", "crates/optimus-eval/src/replay.rs", "crates/optimus-kernel/src/trace.rs", "crates/optimus-kernel/src/lib.rs"], ["crates/optimus-eval/tests/replay_contracts.rs", "crates/optimus-kernel/tests/trace_contracts.rs", "crates/optimus-kernel/tests/kernel_turn.rs", "crates/optimus-kernel/tests/session_resume.rs"]),
        ("C-14", "memory-clock-retention-erasure", "implemented", ["crates/optimus-memory/src/lib.rs"], ["crates/optimus-memory/tests/metamemory_mvp.rs", "crates/optimus-eval/tests/integrity_integration.rs"]),
        ("C-15", "atomic-projection-and-event-transitions", "implemented", ["crates/optimus-store/src/lib.rs", "crates/optimus-graph/src/lib.rs"], ["crates/optimus-store/src/lib.rs", "crates/optimus-runtime/tests/cancellation.rs"]),
        ("C-16", "campaign-job-consistency-and-recovery", "implemented", ["crates/optimus-runtime/src/campaign.rs", "crates/optimus-runtime/src/lib.rs", "crates/optimus-store/src/lib.rs"], ["crates/optimus-runtime/src/campaign.rs", "crates/optimus-store/src/lib.rs"]),
        ("C-17", "cron-gateway-claim-and-delivery", "implemented", ["crates/optimus-ops/src/cron.rs", "crates/optimus-ops/src/gateway.rs"], ["crates/optimus-ops/src/cron.rs", "crates/optimus-ops/src/gateway.rs", "apps/optimus-cli/tests/gateway_http.rs"]),
        ("C-18", "session-causality-around-durable-effects", "implemented", ["crates/optimus-store/src/lib.rs", "crates/optimus-runtime/src/lib.rs", "crates/optimus-kernel/src/lib.rs", "crates/optimus-kernel/src/session.rs"], ["crates/optimus-kernel/tests/session_resume.rs", "crates/optimus-eval/tests/integrity_integration.rs"]),
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
    path = ROOT / "crates/optimus-eval/src/eval.rs"
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
    trajectory_test_path = ROOT / "crates/optimus-eval/tests/evaluation_contracts.rs"
    trajectory_test = trajectory_test_path.read_text(encoding="utf-8")
    trajectory_symbols = [
        "pub fn run_offline_trajectory_suite",
        "pub invoked_tools: Vec<ToolId>",
        "pub terminal_status: Option<ExecutionStatus>",
        "pub replay: Option<ReplayClassification>",
        "pub trace_context: Option<TraceContext>",
    ]
    missing_trajectory = [symbol for symbol in trajectory_symbols if symbol not in text]
    if missing_trajectory or "run_offline_trajectory_suite(" not in trajectory_test:
        raise MemoryError(
            f"typed offline trajectory evidence is stale; missing: {missing_trajectory}"
        )
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
    integration_path = ROOT / "crates/optimus-eval/tests/integrity_integration.rs"
    integration = integration_path.read_text(encoding="utf-8")
    if "pub fn run_offline_integrity_suite" not in text or not re.search(
        r"run_offline_integrity_suite\s*\(", integration
    ):
        raise MemoryError("reusable offline integrity executor is not implemented and exercised")
    integrity_trace_symbols = [
        'TraceStore::open(run_home.join("integrity-traces.db"))',
        "fn traced_integrity_observation",
        "fn finish_integrity_trace",
        "pub terminal_status: Option<ExecutionStatus>",
        "pub replay: Option<ReplayClassification>",
        "pub trace_context: Option<TraceContext>",
        "(None, None, None) => !observation.passed",
    ]
    missing_integrity_trace = [
        symbol for symbol in integrity_trace_symbols if symbol not in text
    ]
    if missing_integrity_trace or "TraceStore::open(run_home.join" not in integration:
        raise MemoryError(
            f"integrity trace evidence is stale; missing: {missing_integrity_trace}"
        )
    typed_path = ROOT / "crates/optimus-eval/src/evaluation.rs"
    typed = typed_path.read_text(encoding="utf-8")
    required_typed_symbols = [
        "pub struct EvaluationDataset",
        "pub struct CandidateBinding",
        "pub struct EvaluationReportV1",
        "pub struct EvaluationResourceMeasurement",
        "pub struct BaselineStore",
        "pub fn build_evaluation_report",
        "pub fn compare_evaluation_reports",
        "pub fn project_evaluation_observations",
        "pub fn priority2_offline_candidate_binding",
        "pub fn run_priority2_offline_evaluation",
        "pub trace_present: bool",
        "case.trace_required && !observation.trace_present",
    ]
    missing = [symbol for symbol in required_typed_symbols if symbol not in typed]
    if missing:
        raise MemoryError(f"typed evaluation extraction is stale; missing symbols: {missing}")
    preflight_symbols = [
        "binding.validate()?",
        "priority2_offline_candidate_binding(binding.source_tree_sha256.clone())?",
        "validated_measurements(&dataset, measurements)?",
        "validate_threshold_policy(thresholds)?",
    ]
    run_start = typed.index("pub fn run_priority2_offline_evaluation")
    run_body = typed[run_start:]
    ownership = run_body.index('join("evaluation-runs")')
    if any(symbol not in run_body[:ownership] for symbol in preflight_symbols):
        raise MemoryError("Priority-2 report inputs are not preflighted before run ownership")
    cli_path = ROOT / "apps/optimus-cli/src/main.rs"
    cli = cli_path.read_text(encoding="utf-8")
    cli_test_path = ROOT / "apps/optimus-cli/tests/eval_report.rs"
    cli_test = cli_test_path.read_text(encoding="utf-8")
    compare_test_path = ROOT / "apps/optimus-cli/tests/eval_compare.rs"
    compare_test = compare_test_path.read_text(encoding="utf-8")
    if (
        "EvalCmd::Report" not in cli
        or "fn read_bounded_json" not in cli
        or "run_priority2_offline_evaluation" not in cli
        or "eval_report_command_prints_the_exact_candidate_report" not in cli_test
    ):
        raise MemoryError("bounded Priority-2 CLI report operation is not implemented and exercised")
    comparison_symbols = [
        "EvalCmd::Compare",
        "fn run_read_only_eval",
        'read_bounded_json(baseline, "baseline report")?',
        'read_bounded_json(candidate, "candidate report")?',
        "compare_evaluation_reports(&baseline, &candidate)?",
    ]
    if (
        any(symbol not in cli for symbol in comparison_symbols)
        or cli.index("if let Some(result) = run_read_only_eval(&cli)")
        > cli.index("std::fs::create_dir_all(&cli.home)?")
        or "eval_compare_prints_exact_read_only_comparison_for_distinct_source_trees"
        not in compare_test
        or "eval_compare_rejects_bounded_invalid_or_incompatible_evidence_without_mutation"
        not in compare_test
    ):
        raise MemoryError("read-only bounded evaluation comparison CLI is not implemented and exercised")
    source_text = Path(__file__).read_text(encoding="utf-8")
    if (
        "def build_priority2_candidate_binding" not in source_text
        or "COMMANDS = (" not in source_text
        or '"binding"' not in source_text
        or '"context"' not in source_text
    ):
        raise MemoryError("authoritative Priority-2 binding generation is not implemented")
    return {
        **generated_header(),
        "framework_status": "produced_priority2_offline_reports_and_immutable_baselines",
        "builtin_cases": [
            {"id": case_id, "source": "crates/optimus-eval/src/eval.rs"}
            for case_id in ids
        ],
        "integrity_cases": [
            {
                "id": case_id,
                "source": "crates/optimus-eval/src/eval.rs",
                "executed_by": "crates/optimus-eval/tests/integrity_integration.rs",
            }
            for case_id in integrity_ids
        ],
        "integrity_executor": {
            "source": "crates/optimus-eval/src/eval.rs",
            "validated_by": "crates/optimus-eval/tests/integrity_integration.rs",
            "case_count": len(integrity_ids),
            "isolated_runs": True,
            "trace_store": "per_run_integrity-traces.db",
            "typed_evidence": ["terminal_status", "replay", "trace_context"],
            "retry_identity": "fresh_trace_ids_stable_normalized_semantics",
        },
        "trajectory_executor": {
            "source": "crates/optimus-eval/src/eval.rs",
            "validated_by": "crates/optimus-eval/tests/evaluation_contracts.rs",
            "case_count": len(ids),
            "typed_evidence": [
                "assistant_text",
                "invoked_tools",
                "terminal_status",
                "replay",
                "trace_context",
            ],
        },
        "priority2_report_executor": {
            "source": "crates/optimus-eval/src/evaluation.rs",
            "validated_by": "crates/optimus-eval/tests/evaluation_contracts.rs",
            "case_count": len(ids) + len(integrity_ids),
            "resource_measurements": "explicit_caller_supplied_per_case",
            "retry_identity": "fresh_run_and_trace_ids_stable_report_bytes",
            "preflight_before_mutation": True,
            "cli": "optimus eval report",
            "cli_validated_by": "apps/optimus-cli/tests/eval_report.rs",
            "json_input_limit_bytes": 1048576,
            "binding_generator": "python scripts/engineering_memory.py binding",
            "binding_context": "compiled_offline_sources_enforced_before_mutation",
        },
        "comparison_cli": {
            "command": "optimus eval compare",
            "source": "apps/optimus-cli/src/main.rs",
            "validated_by": "apps/optimus-cli/tests/eval_compare.rs",
            "json_input_limit_bytes": 1048576,
            "mutation": "none_including_home",
            "valid_regressions_exit": "success_with_complete_comparison",
        },
        "dimensions": {
            "canonical_tool_trace": "covered_by_builtin_cases_and_tests",
            "assistant_text": "exact_output_retained_by_typed_trajectory_executor; dataset_metric_is_exact",
            "workflow_completion": "contract_adapter_and_cross_contract_tests",
            "security": "six_case_observed_integrity_suite_plus_focused_tests",
            "memory_precision_recall": "missing",
            "retrieval_relevance": "missing",
            "source_grounding": "missing",
            "citation_correctness": "missing",
            "cost": "checked_integer_mean_from_explicit_per_case_measurements",
            "latency": "checked_integer_mean_from_explicit_per_case_measurements",
            "replay": "persisted_fixture_replay_classification_plus_accuracy_metric",
            "trace": "required_case_trace_presence_enforced_before_metrics",
            "gpu_cpu_correctness": "not_applicable_no_gpu_component",
        },
        "typed_dataset": {
            "id": "priority2-integrity",
            "version": 1,
            "case_count": 10,
            "source": "crates/optimus-eval/src/evaluation.rs",
            "validated_by": "crates/optimus-eval/tests/evaluation_contracts.rs",
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


def expand_patterns_against_sha_map(
    patterns: Iterable[str], sha_map: dict[str, str]
) -> list[str]:
    """Resolve ownership globs against an in-memory path→sha map."""
    paths: set[str] = set()
    all_paths = list(sha_map)
    for pattern in patterns:
        if any(token in pattern for token in ("*", "?", "[")):
            paths.update(path for path in all_paths if fnmatch.fnmatch(path, pattern))
        elif pattern in sha_map:
            paths.add(pattern)
        else:
            # Fall back to filesystem for exact paths missing from the index snapshot.
            exact = ROOT / pattern
            if exact.is_file() and not is_excluded(exact) and not is_sensitive(exact):
                paths.add(relative(exact))
    return sorted(paths)


def tree_hash_for_patterns(
    patterns: Iterable[str], sha_map: dict[str, str] | None = None
) -> tuple[str, int]:
    if sha_map is None:
        paths = expand_patterns(patterns)
        digest, records = tree_for_paths(paths)
        return digest, len(records)
    matched = expand_patterns_against_sha_map(patterns, sha_map)
    records = [{"path": path, "sha256": sha_map[path]} for path in matched if path in sha_map]
    # Include any exact paths resolved outside the map.
    missing = [path for path in matched if path not in sha_map]
    for rel in missing:
        records.append(file_record(ROOT / rel))
    return records_tree_hash(records), len(records)


def knowledge_documents() -> list[tuple[Path, dict[str, Any]]]:
    out = []
    for path in sorted((ROOT / "docs").rglob("*.md")):
        frontmatter = parse_frontmatter(path)
        if frontmatter and "knowledge_type" in frontmatter:
            out.append((path, frontmatter))
    return out


def build_change_impact() -> dict[str, Any]:
    """Compact impact index: patterns only; path expansion is query-time."""
    documents = []
    pattern_to_knowledge: dict[str, list[dict[str, str]]] = defaultdict(list)
    for path, frontmatter in knowledge_documents():
        owns = ownership_patterns(frontmatter)
        watches = watch_patterns(frontmatter)
        depends = depends_patterns(frontmatter)
        validated = validated_patterns(frontmatter)
        covers = list(frontmatter.get("covers", [])) if isinstance(frontmatter.get("covers", []), list) else []
        resolved = expand_patterns(owns + depends)
        doc_rel = relative(path)
        for pattern in owns:
            pattern_to_knowledge[pattern].append({"document": doc_rel, "relation": "owns"})
        for pattern in depends:
            pattern_to_knowledge[pattern].append({"document": doc_rel, "relation": "depends_on"})
        for pattern in watches:
            pattern_to_knowledge[pattern].append({"document": doc_rel, "relation": "watches"})
        documents.append(
            {
                "document": doc_rel,
                "knowledge_type": frontmatter["knowledge_type"],
                "status": frontmatter.get("status"),
                "owns": owns,
                "covers": covers or owns,
                "watches": watches,
                "depends_on": depends,
                "validated_by": validated,
                "resolved_source_count": len(resolved),
                "resolved_test_count": len(expand_patterns(validated)),
            }
        )
    compact_patterns = {
        pattern: sorted(entries, key=lambda item: (item["document"], item["relation"]))
        for pattern, entries in sorted(pattern_to_knowledge.items())
    }
    return {
        **generated_header(),
        "documents": documents,
        "pattern_to_knowledge": compact_patterns,
        "algorithm": (
            "frontmatter owns/covers+depends_on stale on match; watches warn only; "
            "path expansion is query-time against repository index"
        ),
    }


def impact_for_paths(paths: Iterable[str], impact: dict[str, Any] | None = None) -> dict[str, list[dict[str, str]]]:
    impact = impact or build_change_impact()
    patterns = impact.get("pattern_to_knowledge", {})
    affected: dict[str, list[dict[str, str]]] = {}
    for path in paths:
        hits: list[dict[str, str]] = []
        seen: set[tuple[str, str]] = set()
        for pattern, entries in patterns.items():
            if not pattern_matches(path, pattern):
                continue
            for entry in entries:
                key = (entry["document"], entry["relation"])
                if key in seen:
                    continue
                seen.add(key)
                hits.append(dict(entry))
        if hits:
            affected[path] = sorted(hits, key=lambda item: (item["document"], item["relation"]))
    return affected


def existing_staleness() -> dict[str, dict[str, Any]]:
    path = MEMORY_DIR / "knowledge-staleness.json"
    if not path.exists():
        return {}
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return {}
    return {item["document"]: item for item in data.get("documents", [])}


def build_knowledge_staleness(
    refresh: bool, sha_map: dict[str, str] | None = None
) -> dict[str, Any]:
    """Hash-only staleness. Covered path lists are derived at query time."""
    previous = existing_staleness()
    if sha_map is None:
        sha_map = live_file_sha_map()
    documents = []
    for path, frontmatter in knowledge_documents():
        owns = ownership_patterns(frontmatter)
        depends = depends_patterns(frontmatter)
        current_hash, covered_count = tree_hash_for_patterns(owns + depends, sha_map)
        doc_rel = relative(path)
        old = previous.get(doc_rel)
        baseline_hash = current_hash if refresh or not old else old.get("verified_tree_sha256")
        documents.append(
            {
                "document": doc_rel,
                "knowledge_type": frontmatter["knowledge_type"],
                "knowledge_status": frontmatter.get("status"),
                "last_verified_commit": frontmatter.get("last_verified_commit"),
                "verification_basis": "sha256_tree",
                "verified_tree_sha256": baseline_hash,
                "current_tree_sha256": current_hash,
                "stale": baseline_hash != current_hash,
                "covered_file_count": covered_count,
                "owns_patterns": owns,
                "depends_on_patterns": depends,
            }
        )
    return {
        **generated_header(),
        "documents": documents,
        "stale_count": sum(1 for item in documents if item["stale"]),
        "storage": "hash_only_v2",
    }


def build_manifest(maps: dict[str, dict[str, Any]]) -> dict[str, Any]:
    repository = maps["repository-index.json"]
    tools = expand_tool_registry(maps["tool-registry.json"]).get("tools", [])
    workflows = maps["workflow-registry.json"].get("workflows", [])
    agents = maps["agent-registry.json"].get("agents", [])
    staleness = maps["knowledge-staleness.json"]
    impact = maps["change-impact.json"]
    artifact_sha = {
        name: sha256_bytes(canonical_json(maps[name]).encode("utf-8"))
        for name in GENERATED_NAMES
        if name != "manifest.json" and name in maps
    }
    return {
        **generated_header(),
        "tree_sha256": repository.get("tree_sha256"),
        "verification_basis": "sha256_tree",
        "artifact_sha256": artifact_sha,
        "counts": {
            "files": repository.get("file_count"),
            "knowledge_documents": len(impact.get("documents", [])),
            "stale_documents": staleness.get("stale_count", 0),
            "tools": len(tools),
            "available_tools": sum(1 for tool in tools if tool.get("available")),
            "workflows": len(workflows),
            "agents": len(agents),
            "impact_patterns": len(impact.get("pattern_to_knowledge", {})),
        },
        "serving": {
            "agent_interface": [
                "check",
                "context",
                "impact",
                "stale",
                "tools",
                "owner",
                "report",
                "stat",
            ],
            "raw_json_not_for_prompt_loading": True,
            "default_context_budget_tokens": DEFAULT_CONTEXT_BUDGET_TOKENS,
            "schema_version": SCHEMA_VERSION,
        },
    }


def build_maps(refresh_staleness: bool) -> dict[str, dict[str, Any]]:
    metadata = cargo_metadata()
    maps = {
        "repository-index.json": build_repository_index(metadata),
        "agent-registry.json": build_agent_registry(),
        "tool-registry.json": compress_tool_registry(parse_tool_catalog()),
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
    maps["manifest.json"] = build_manifest(maps)
    # Recompute artifact hashes now that manifest payload shape is fixed without self-hash.
    maps["manifest.json"] = build_manifest(maps)
    return maps


def canonical_json(value: Any) -> str:
    """Compact deterministic JSON for token-efficient on-disk facts."""
    return json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":")) + "\n"


def write_maps(maps: dict[str, dict[str, Any]]) -> None:
    MEMORY_DIR.mkdir(parents=True, exist_ok=True)
    for name in GENERATED_NAMES:
        (MEMORY_DIR / name).write_text(canonical_json(maps[name]), encoding="utf-8", newline="\n")
    _save_hash_cache()


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

    required_source_fragments = {
        # The tool-call budget constant is declared with the kernel types.
        "crates/optimus-kernel/src/lib.rs": ("HARD_MAX_TOOL_CALLS_PER_STEP",),
        # Loop-behaviour authority moved here when lib.rs was split under
        # architectural law 21. The invariant is unchanged: these suppression
        # and terminal-timing behaviours must exist and stay locatable.
        "crates/optimus-kernel/src/turn_loop.rs": (
            "HARD_MAX_TOOL_CALLS_PER_STEP",
            "duplicate_tool_call_suppressed",
            "tool_call_budget_suppressed",
            "TimingEventKind::TurnFinished",
        ),
        "crates/optimus-kernel/src/execution.rs": (
            "execution_timing_events",
            "duration_ms INTEGER NOT NULL",
            "timing_summary",
        ),
        "apps/optimus-desktop/ui/app.js": (
            "sessionTimer",
            "turnTimer",
            "first_response_ms",
            "suppressedCount",
        ),
    }
    for rel, fragments in required_source_fragments.items():
        text = (ROOT / rel).read_text(encoding="utf-8")
        for fragment in fragments:
            if fragment not in text:
                errors.append(f"timing/loop authority missing from {rel}: {fragment}")
    return errors


def load_generated_maps() -> dict[str, Any]:
    loaded: dict[str, Any] = {}
    for name in GENERATED_NAMES:
        path = MEMORY_DIR / name
        if not path.exists():
            continue
        loaded[name] = json.loads(path.read_text(encoding="utf-8"))
    return loaded


def validate_maps(strict: bool = False, mode: str = "full") -> dict[str, Any]:
    if mode not in {"full", "quick"}:
        raise MemoryError(f"unknown validate mode: {mode}")
    errors: list[str] = []
    warnings: list[str] = []
    expected: dict[str, Any] | None = None
    if mode == "full":
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
        if expected is not None and loaded[name] != expected[name]:
            errors.append(f"generated file drift: {relative(path)} (run generate; do not edit manually)")
        if loaded[name].get("generated_by") != GENERATOR or loaded[name].get("do_not_edit") is not True:
            errors.append(f"missing generated marker: {relative(path)}")
        if loaded[name].get("schema_version") != SCHEMA_VERSION:
            errors.append(
                f"schema_version mismatch in {relative(path)}: "
                f"{loaded[name].get('schema_version')} != {SCHEMA_VERSION}"
            )

    live_sha_map: dict[str, str] | None = None
    docs_changed = True
    if mode == "quick" and "repository-index.json" in loaded:
        try:
            live_sha_map = live_file_sha_map()
            live_tree = records_tree_hash(
                [{"path": path, "sha256": digest} for path, digest in live_sha_map.items()]
            )
            if live_tree != loaded["repository-index.json"].get("tree_sha256"):
                errors.append("quick validate: repository tree_sha256 drift (run generate)")
            old_files = {
                item["path"]: item["sha256"]
                for item in loaded["repository-index.json"].get("files", [])
            }
            changed = sorted(
                path
                for path in set(old_files) | set(live_sha_map)
                if old_files.get(path) != live_sha_map.get(path)
            )
            docs_changed = any(path.startswith("docs/") and path.endswith(".md") for path in changed)
            live_staleness = build_knowledge_staleness(refresh=False, sha_map=live_sha_map)
            for document in live_staleness.get("documents", []):
                if document.get("stale"):
                    errors.append(f"stale Engineering Memory: {document['document']}")
            live_impact = build_change_impact()
            if live_impact.get("pattern_to_knowledge") != loaded.get("change-impact.json", {}).get(
                "pattern_to_knowledge"
            ):
                errors.append("quick validate: change-impact pattern drift (run generate)")
        except (MemoryError, OSError, subprocess.SubprocessError) as exc:
            errors.append(f"quick validate failed: {exc}")

    for rel in IMPORTANT_DOCS:
        path = ROOT / rel
        if not path.exists():
            errors.append(f"missing important Engineering Memory document {rel}")
            continue
        frontmatter = parse_frontmatter(path)
        if not frontmatter:
            errors.append(f"missing frontmatter: {rel}")
            continue
        for field in ("knowledge_type", "status", "depends_on", "validated_by", "last_verified_commit"):
            if field not in frontmatter:
                errors.append(f"frontmatter missing {field}: {rel}")
        if "covers" not in frontmatter and "owns" not in frontmatter:
            errors.append(f"frontmatter missing covers/owns: {rel}")
        if frontmatter.get("status") not in {"current", "planned", "historical", "stale"}:
            errors.append(f"invalid frontmatter status in {rel}: {frontmatter.get('status')}")
        if live_sha_map is not None:
            covered_count = tree_hash_for_patterns(ownership_patterns(frontmatter), live_sha_map)[1]
            covered_ok = covered_count > 0
        else:
            covered_ok = bool(expand_patterns(ownership_patterns(frontmatter)))
        if not covered_ok:
            errors.append(f"frontmatter covers no files: {rel}")

    if mode == "full" or docs_changed:
        for path in sorted((ROOT / "docs").rglob("*.md")):
            errors.extend(f"broken local link: {problem}" for problem in local_link_problems(path))

    if loaded and mode == "full":
        for name, data in loaded.items():
            for problem in sorted(set(reference_problems(data))):
                errors.append(f"{name}: {problem}")
    elif loaded and mode == "quick":
        # Quick mode still checks path refs on compact registries without full rebuild.
        for name in ("tool-registry.json", "workflow-registry.json", "contract-coverage.json"):
            if name in loaded:
                for problem in sorted(set(reference_problems(loaded[name]))):
                    errors.append(f"{name}: {problem}")

    fallback_tools_registry = (expected or {}).get("tool-registry.json", {})
    tools_registry = loaded.get("tool-registry.json", fallback_tools_registry)
    try:
        tools = expand_tool_registry(tools_registry).get("tools", [])
    except MemoryError as exc:
        errors.append(str(exc))
        tools = tools_registry.get("tools", [])
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
        fallback_rows = (expected or {}).get(registry_name, {}).get(array_name, [])
        rows = loaded.get(registry_name, {}).get(array_name, fallback_rows)
        ids = [row.get("id") for row in rows]
        if len(ids) != len(set(ids)):
            errors.append(f"duplicate IDs in {registry_name}")
    fallback_agents = (expected or {}).get("agent-registry.json", {})
    agent_registry = loaded.get("agent-registry.json", fallback_agents)
    agents = agent_registry.get("agents", [])
    if not agents and agent_registry.get("contract_substrate", {}).get("status") != "implemented":
        warnings.append("no implemented specialist agents or universal agent schema")
    fallback_workflows = (expected or {}).get("workflow-registry.json", {}).get("workflows", [])
    workflows = loaded.get("workflow-registry.json", {}).get("workflows", fallback_workflows)
    for workflow in workflows:
        if workflow.get("cancellation", {}).get("status") != "implemented":
            warnings.append(f"workflow {workflow['id']} lacks implemented cancellation contract")
        if not workflow.get("completion") or not workflow.get("failure"):
            errors.append(f"workflow {workflow['id']} lacks terminal outcome declarations")

    fallback_contracts = (expected or {}).get("contract-coverage.json", {}).get("contracts", [])
    contracts = loaded.get("contract-coverage.json", {}).get("contracts", fallback_contracts)
    errors.extend(current_architecture_semantic_errors(workflows, contracts))

    warnings.extend(adr_warnings())
    if mode == "full":
        staleness = loaded.get(
            "knowledge-staleness.json",
            (expected or {}).get("knowledge-staleness.json", {}),
        )
        for document in staleness.get("documents", []):
            if document.get("stale"):
                errors.append(f"stale Engineering Memory: {document['document']}")
        for document in staleness.get("documents", []):
            if "covered_files" in document:
                errors.append(
                    f"legacy covered_files payload present in staleness for {document.get('document')}"
                )
        impact = loaded.get("change-impact.json", (expected or {}).get("change-impact.json", {}))
        if "source_to_knowledge" in impact:
            errors.append("legacy source_to_knowledge payload present in change-impact")
        if "pattern_to_knowledge" not in impact:
            errors.append("change-impact missing pattern_to_knowledge")

    gpu_sources = [
        relative(path)
        for path in repository_files()
        if path.suffix == ".rs" and re.search(r"(?:^|[-_/])(gpu|cuda)(?:[-_/]|$)" , relative(path), re.I)
    ]
    if gpu_sources:
        warnings.append(f"GPU source requires CPU-fallback declaration review: {gpu_sources}")

    if strict and warnings:
        errors.extend(f"strict: {warning}" for warning in warnings)
    _save_hash_cache()
    return {
        "ok": not errors,
        "mode": mode,
        "errors": sorted(set(errors)),
        "warnings": sorted(set(warnings)),
        "generated_files": len(GENERATED_NAMES),
        "tool_count": len(tools),
        "available_tool_count": sum(1 for tool in tools if tool.get("available")),
        "workflow_count": len(workflows),
        "agent_count": len(agents),
    }


def live_file_sha_map() -> dict[str, str]:
    """Path→sha256 for the live tree without cargo metadata."""
    return {record["path"]: record["sha256"] for record in (file_record(path) for path in repository_files())}


def check_staleness() -> dict[str, Any]:
    new_files = live_file_sha_map()
    current = build_knowledge_staleness(refresh=False, sha_map=new_files)
    old_index_path = MEMORY_DIR / "repository-index.json"
    changed_files: list[str] = []
    if old_index_path.exists():
        try:
            old = json.loads(old_index_path.read_text(encoding="utf-8"))
            old_files = {item["path"]: item["sha256"] for item in old.get("files", [])}
            changed_files = sorted(
                path
                for path in set(old_files) | set(new_files)
                if old_files.get(path) != new_files.get(path)
            )
        except (OSError, json.JSONDecodeError, MemoryError) as exc:
            changed_files = [f"unable to compare repository index: {exc}"]
    else:
        changed_files = ["no generated repository baseline"]
    stale_documents = [item["document"] for item in current["documents"] if item["stale"]]
    impact = None
    impact_path = MEMORY_DIR / "change-impact.json"
    if impact_path.exists():
        try:
            impact = json.loads(impact_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            impact = None
    if not impact or "pattern_to_knowledge" not in impact:
        impact = build_change_impact()
    affected_raw = impact_for_paths(
        [path for path in changed_files if not path.startswith("unable to ")],
        impact,
    )
    affected: dict[str, list[str]] = {}
    watch_hits: dict[str, list[str]] = {}
    for path, entries in affected_raw.items():
        stale_docs = sorted(
            {
                entry["document"]
                for entry in entries
                if entry["relation"] in {"owns", "depends_on"}
            }
        )
        watch_docs = sorted(
            {entry["document"] for entry in entries if entry["relation"] == "watches"}
        )
        if stale_docs:
            affected[path] = stale_docs
        if watch_docs:
            watch_hits[path] = watch_docs
    _save_hash_cache()
    return {
        "ok": not stale_documents and not changed_files,
        "changed_files": changed_files,
        "stale_documents": stale_documents,
        "affected_knowledge": affected,
        "watch_knowledge": watch_hits,
    }


def tool_card(tool: dict[str, Any]) -> dict[str, Any]:
    return {
        "id": tool.get("id"),
        "available": tool.get("available"),
        "policy": tool.get("policy"),
        "pack": tool.get("pack"),
        "description": tool.get("description"),
        "approval": (tool.get("approval") or {}).get("status"),
        "cancellation": tool.get("cancellation_status"),
        "schema_tokens": tool.get("schema_tokens"),
    }


def query_tools(available_only: bool = False) -> dict[str, Any]:
    loaded = load_generated_maps()
    registry = loaded.get("tool-registry.json")
    if registry is None:
        registry = compress_tool_registry(parse_tool_catalog())
    tools = expand_tool_registry(registry).get("tools", [])
    cards = [tool_card(tool) for tool in tools if tool.get("available") or not available_only]
    return {
        "ok": True,
        "count": len(cards),
        "tools": cards,
    }


def query_owner(path: str) -> dict[str, Any]:
    impact = None
    impact_path = MEMORY_DIR / "change-impact.json"
    if impact_path.exists():
        try:
            impact = json.loads(impact_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            impact = None
    if not impact or "pattern_to_knowledge" not in impact:
        impact = build_change_impact()
    hits = impact_for_paths([path], impact).get(path, [])
    return {
        "ok": True,
        "path": path,
        "owners": [row for row in hits if row["relation"] in {"owns", "depends_on"}],
        "watches": [row for row in hits if row["relation"] == "watches"],
    }


def query_impact(paths: list[str]) -> dict[str, Any]:
    impact = None
    impact_path = MEMORY_DIR / "change-impact.json"
    if impact_path.exists():
        try:
            impact = json.loads(impact_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            impact = None
    if not impact or "pattern_to_knowledge" not in impact:
        impact = build_change_impact()
    return {
        "ok": True,
        "paths": paths,
        "affected": impact_for_paths(paths, impact),
    }


def query_stale() -> dict[str, Any]:
    current = build_knowledge_staleness(refresh=False)
    stale = [item for item in current["documents"] if item.get("stale")]
    _save_hash_cache()
    return {
        "ok": not stale,
        "stale_count": len(stale),
        "documents": [
            {
                "document": item["document"],
                "verified_tree_sha256": item.get("verified_tree_sha256"),
                "current_tree_sha256": item.get("current_tree_sha256"),
                "covered_file_count": item.get("covered_file_count"),
                "owns_patterns": item.get("owns_patterns"),
            }
            for item in stale
        ],
    }


def _heading_snippets(path: Path, limit_chars: int = 900) -> list[str]:
    text = path.read_text(encoding="utf-8", errors="replace")
    body = text
    if text.startswith("---"):
        end = text.find("\n---", 3)
        if end > 0:
            body = text[end + 4 :]
    snippets: list[str] = []
    current_heading = "(intro)"
    current_lines: list[str] = []
    for line in body.splitlines():
        if line.startswith("#"):
            if current_lines:
                block = f"## {current_heading}\n" + "\n".join(current_lines).strip()
                if block.strip():
                    snippets.append(block[:limit_chars])
            current_heading = line.lstrip("#").strip() or "(section)"
            current_lines = []
        else:
            if line.strip():
                current_lines.append(line.rstrip())
    if current_lines:
        block = f"## {current_heading}\n" + "\n".join(current_lines).strip()
        snippets.append(block[:limit_chars])
    return snippets


def build_context_pack(
    budget_tokens: int = DEFAULT_CONTEXT_BUDGET_TOKENS,
    paths: list[str] | None = None,
) -> dict[str, Any]:
    started = time.perf_counter()
    check = check_staleness()
    selected_paths = paths or [
        path for path in check.get("changed_files", []) if not path.startswith("unable to ")
    ]
    impact = query_impact(selected_paths) if selected_paths else {"affected": {}}
    stale = query_stale()
    loaded = load_generated_maps()
    tree = loaded.get("repository-index.json", {}).get("tree_sha256", "unknown")
    lines = [
        f"EM_CONTEXT v2 tree={tree} budget={budget_tokens}",
        f"STATUS: {'CURRENT' if check.get('ok') and stale.get('ok') else 'STALE_OR_CHANGED'}",
    ]
    if stale.get("documents"):
        lines.append("STALE:")
        for item in stale["documents"]:
            lines.append(f"  - {item['document']} files={item.get('covered_file_count')}")
    if selected_paths:
        lines.append("CHANGED:")
        for path in selected_paths[:40]:
            lines.append(f"  - {path}")
        if len(selected_paths) > 40:
            lines.append(f"  - ... ({len(selected_paths) - 40} more)")
    owner_docs: list[str] = []
    watch_docs: list[str] = []
    if impact.get("affected"):
        lines.append("IMPACT:")
        for path, entries in sorted(impact["affected"].items()):
            owns = [e["document"] for e in entries if e["relation"] in {"owns", "depends_on"}]
            watches = [e["document"] for e in entries if e["relation"] == "watches"]
            owner_docs.extend(owns)
            watch_docs.extend(watches)
            if owns:
                lines.append(f"  {path}")
                lines.append(f"    owns/depends -> {', '.join(sorted(set(owns)))}")
            if watches:
                lines.append(f"    watches -> {', '.join(sorted(set(watches)))}")
    # Fact cards for touched tools/workflows when source paths suggest them.
    tools = expand_tool_registry(loaded.get("tool-registry.json", {})).get("tools", [])
    if any("optimus-packs" in path or path.endswith("packs/src/lib.rs") for path in selected_paths):
        available = [tool_card(tool) for tool in tools if tool.get("available")]
        lines.append("TOOLS_AVAILABLE:")
        for card in available:
            lines.append(
                f"  - {card['id']} policy={card['policy']} approval={card['approval']} "
                f"cancel={card['cancellation']} tokens={card['schema_tokens']}"
            )
    workflows = loaded.get("workflow-registry.json", {}).get("workflows", [])
    if any(
        any(token in path for token in ("campaign", "gateway", "cron", "runtime", "graph"))
        for path in selected_paths
    ):
        lines.append("WORKFLOWS:")
        for workflow in workflows:
            lines.append(
                f"  - {workflow.get('id')} cancel={workflow.get('cancellation', {}).get('status')} "
                f"owner={workflow.get('owner')}"
            )
    lines.append("READ:")
    stale_docs = [item["document"] for item in stale.get("documents", [])]
    read_docs = list(dict.fromkeys([*stale_docs, *owner_docs]))
    if not read_docs:
        read_docs = [
            "docs/engineering-memory/README.md",
            "docs/architecture/system-overview.md",
        ]
    used_tokens = estimate_tokens("\n".join(lines))
    for doc in read_docs:
        path = ROOT / doc if isinstance(doc, str) else None
        if path is None or not path.exists():
            continue
        snippets = _heading_snippets(path)
        # Prefer first two non-empty sections.
        for snippet in snippets[:2]:
            candidate = f"  DOC {doc}\n{snippet}"
            cost = estimate_tokens(candidate)
            if used_tokens + cost > budget_tokens:
                lines.append(f"  DOC {doc} (truncated for budget)")
                break
            lines.append(candidate)
            used_tokens += cost
        else:
            continue
        break
    gaps = []
    agent_registry = loaded.get("agent-registry.json", {})
    if agent_registry.get("implemented_specialist_agent_count") == 0:
        gaps.append("no specialist agents registered")
    model_registry = loaded.get("model-registry.json", {})
    gaps.extend(model_registry.get("known_gaps", [])[:2])
    if gaps:
        gap_block = ["GAPS:", *[f"  - {gap}" for gap in gaps]]
        candidate_lines = lines + gap_block
        if estimate_tokens("\n".join(candidate_lines) + "\n") <= budget_tokens:
            lines.extend(gap_block)
        else:
            lines.append("GAPS: (omitted for budget)")
    # Hard-clamp: never exceed the declared budget for agent prompt loading.
    while len(lines) > 4 and estimate_tokens("\n".join(lines) + "\n") > budget_tokens:
        lines.pop()
    text = "\n".join(lines) + "\n"
    used = estimate_tokens(text)
    if used > budget_tokens:
        # Pathological tiny budgets: keep header only.
        header = lines[:3] if len(lines) >= 3 else lines
        text = "\n".join(header) + "\n# truncated_to_budget\n"
        used = estimate_tokens(text)
    return {
        "ok": True,
        "budget_tokens": budget_tokens,
        "used_tokens": used,
        "elapsed_ms": int((time.perf_counter() - started) * 1000),
        "tree_sha256": tree,
        "text": text,
        "stale_documents": [item["document"] for item in stale.get("documents", [])],
        "changed_files": selected_paths,
        "watch_documents": sorted(set(watch_docs)),
    }


def build_report() -> dict[str, Any]:
    loaded = load_generated_maps()
    check = check_staleness()
    stale = query_stale()
    manifest = loaded.get("manifest.json", {})
    tools = expand_tool_registry(loaded.get("tool-registry.json", {})).get("tools", [])
    return {
        "ok": bool(check.get("ok") and stale.get("ok")),
        "tree_sha256": loaded.get("repository-index.json", {}).get("tree_sha256"),
        "manifest_counts": manifest.get("counts", {}),
        "changed_files": check.get("changed_files", []),
        "stale_documents": stale.get("documents", []),
        "agents": loaded.get("agent-registry.json", {}).get("implemented_specialist_agent_count"),
        "tools": len(tools),
        "available_tools": sum(1 for tool in tools if tool.get("available")),
        "workflows": len(loaded.get("workflow-registry.json", {}).get("workflows", [])),
        "serving": manifest.get("serving", {}),
        "recommendation": (
            "no knowledge refresh required"
            if check.get("ok") and stale.get("ok")
            else "run context lens, update owned docs, then generate+validate"
        ),
    }


def build_stat() -> dict[str, Any]:
    total = 0
    per_file: dict[str, dict[str, int]] = {}
    for name in GENERATED_NAMES:
        path = MEMORY_DIR / name
        if not path.exists():
            continue
        raw = path.read_bytes()
        total += len(raw)
        per_file[name] = {
            "bytes": len(raw),
            "approx_tokens": max(1, (len(raw) + 3) // 4),
        }
    return {
        "ok": True,
        "schema_version": SCHEMA_VERSION,
        "total_bytes": total,
        "approx_tokens_if_fully_loaded": max(1, (total + 3) // 4),
        "files": per_file,
        "hash_cache_entries": len(_load_hash_cache().get("entries", {})),
        "note": "Agents should use context/report lenses; do not load raw JSON into prompts.",
    }


def print_result(result: dict[str, Any], as_json: bool) -> None:
    if as_json:
        print(canonical_json(result), end="")
        return
    if "text" in result and result.get("text"):
        print(result["text"], end="" if str(result["text"]).endswith("\n") else "\n")
        print(
            f"# used_tokens={result.get('used_tokens')} budget={result.get('budget_tokens')} "
            f"elapsed_ms={result.get('elapsed_ms')}"
        )
        return
    if "errors" in result:
        print("ENGINEERING_MEMORY_VALID" if result["ok"] else "ENGINEERING_MEMORY_INVALID")
        if result.get("mode"):
            print(f"mode={result['mode']}")
        for error in result["errors"]:
            print(f"ERROR: {error}")
        for warning in result.get("warnings", []):
            print(f"WARNING: {warning}")
        if "generated_files" in result:
            print(
                f"generated={result['generated_files']} agents={result['agent_count']} "
                f"tools={result['tool_count']} available_tools={result['available_tool_count']} "
                f"workflows={result['workflow_count']}"
            )
        return
    if "approx_tokens_if_fully_loaded" in result:
        print("ENGINEERING_MEMORY_STAT")
        print(
            f"total_bytes={result['total_bytes']} approx_tokens={result['approx_tokens_if_fully_loaded']} "
            f"schema={result['schema_version']} hash_cache_entries={result['hash_cache_entries']}"
        )
        for name, info in sorted(result.get("files", {}).items()):
            print(f"FILE: {name} bytes={info['bytes']} tokens~{info['approx_tokens']}")
        print(result.get("note", ""))
        return
    if "recommendation" in result and "manifest_counts" in result:
        print("ENGINEERING_MEMORY_REPORT" if result["ok"] else "ENGINEERING_MEMORY_REPORT_STALE")
        print(f"tree={result.get('tree_sha256')}")
        print(f"counts={result.get('manifest_counts')}")
        print(
            f"agents={result.get('agents')} tools={result.get('tools')} "
            f"available_tools={result.get('available_tools')} workflows={result.get('workflows')}"
        )
        for path in result.get("changed_files", []):
            print(f"CHANGED: {path}")
        for item in result.get("stale_documents", []):
            print(f"STALE: {item.get('document')}")
        print(f"RECOMMENDATION: {result.get('recommendation')}")
        return
    if "tools" in result and isinstance(result.get("tools"), list) and "count" in result:
        print("ENGINEERING_MEMORY_TOOLS")
        for tool in result["tools"]:
            print(
                f"TOOL: {tool.get('id')} available={tool.get('available')} "
                f"policy={tool.get('policy')} approval={tool.get('approval')} "
                f"cancel={tool.get('cancellation')}"
            )
        return
    if "owners" in result and "path" in result:
        print("ENGINEERING_MEMORY_OWNER")
        print(f"PATH: {result['path']}")
        for row in result.get("owners", []):
            print(f"OWNER: {row['document']} relation={row['relation']}")
        for row in result.get("watches", []):
            print(f"WATCH: {row['document']} relation={row['relation']}")
        return
    if "affected" in result and "paths" in result:
        print("ENGINEERING_MEMORY_IMPACT")
        for path, entries in sorted(result.get("affected", {}).items()):
            owns = [e["document"] for e in entries if e["relation"] in {"owns", "depends_on"}]
            watches = [e["document"] for e in entries if e["relation"] == "watches"]
            print(f"PATH: {path}")
            if owns:
                print(f"  OWNS/DEPENDS: {', '.join(sorted(set(owns)))}")
            if watches:
                print(f"  WATCHES: {', '.join(sorted(set(watches)))}")
        return
    if "stale_count" in result and "documents" in result and "changed_files" not in result:
        print("ENGINEERING_MEMORY_STALE_QUERY" if not result["ok"] else "ENGINEERING_MEMORY_NO_STALE")
        for item in result.get("documents", []):
            print(
                f"STALE: {item['document']} files={item.get('covered_file_count')} "
                f"owns={','.join(item.get('owns_patterns') or [])}"
            )
        return
    print("ENGINEERING_MEMORY_CURRENT" if result["ok"] else "ENGINEERING_MEMORY_STALE")
    for path in result.get("changed_files", []):
        print(f"CHANGED: {path}")
    for document in result.get("stale_documents", []):
        print(f"STALE: {document}")
    for path, documents in result.get("affected_knowledge", {}).items():
        print(f"IMPACT: {path} -> {', '.join(documents)}")
    for path, documents in result.get("watch_knowledge", {}).items():
        print(f"WATCH: {path} -> {', '.join(documents)}")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=COMMANDS)
    parser.add_argument("--strict", action="store_true", help="treat known gaps/warnings as failures")
    parser.add_argument("--json", action="store_true", help="emit machine-readable result")
    parser.add_argument(
        "--quick",
        action="store_true",
        help="validate mode: structural + tree/staleness/impact without full rebuild compare",
    )
    parser.add_argument(
        "--budget",
        type=int,
        default=DEFAULT_CONTEXT_BUDGET_TOKENS,
        help="token budget for context lens",
    )
    parser.add_argument(
        "--path",
        action="append",
        default=[],
        help="path for impact/owner/context (repeatable)",
    )
    parser.add_argument(
        "--available",
        action="store_true",
        help="tools lens: only available tools",
    )
    args = parser.parse_args(argv)
    try:
        if args.command == "binding":
            print(canonical_json(build_priority2_candidate_binding()), end="")
            _save_hash_cache()
            return 0
        if args.command == "generate":
            maps = build_maps(refresh_staleness=True)
            write_maps(maps)
            result = validate_maps(strict=args.strict, mode="full")
        elif args.command == "check":
            result = check_staleness()
        elif args.command == "validate":
            result = validate_maps(strict=args.strict, mode="quick" if args.quick else "full")
        elif args.command == "impact":
            paths = args.path or check_staleness().get("changed_files", [])
            paths = [path for path in paths if not str(path).startswith("unable to ")]
            result = query_impact(paths)
        elif args.command == "stale":
            result = query_stale()
        elif args.command == "tools":
            result = query_tools(available_only=args.available)
        elif args.command == "owner":
            if not args.path:
                raise MemoryError("owner requires --path")
            result = query_owner(args.path[0])
        elif args.command == "context":
            result = build_context_pack(budget_tokens=args.budget, paths=args.path or None)
        elif args.command == "report":
            result = build_report()
        elif args.command == "stat":
            result = build_stat()
        else:
            raise MemoryError(f"unknown command: {args.command}")
    except (MemoryError, OSError, subprocess.SubprocessError) as exc:
        if args.command == "binding":
            print(f"ERROR: {exc}", file=sys.stderr)
            return 1
        result = {"ok": False, "errors": [str(exc)], "warnings": []}
    print_result(result, args.json)
    return 0 if result.get("ok") else 1


if __name__ == "__main__":
    raise SystemExit(main())
