#!/usr/bin/env python3
"""Validate, index, search, and benchmark Optimus documentation authority."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import re
import sys
import urllib.parse
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable


ROOT = Path(__file__).resolve().parents[1]
DOCS = ROOT / "docs"
ROUTES = DOCS / "authority-routes.json"
CATALOG_JSON = DOCS / "catalog.json"
CATALOG_MD = DOCS / "CATALOG.md"
LOCK = DOCS / "verification-lock.json"
BENCHMARK = ROOT / "evals" / "docs-authority" / "questions-v1.json"
GENERATED = {CATALOG_JSON.resolve(), CATALOG_MD.resolve(), LOCK.resolve()}

REQUIRED = {
    "doc_id", "doc_type", "plane", "status", "authority", "summary",
    "reviewed_on", "review_by",
}
DOC_TYPES = {"tutorial", "how-to", "reference", "explanation", "decision", "evidence", "history"}
PLANES = {"current", "work", "decision", "evidence", "history"}
STATUSES = {"current", "planned", "historical", "stale"}
AUTHORITIES = {"canonical", "supporting", "record", "historical"}
STOP_WORDS = {
    "a", "an", "and", "are", "as", "at", "be", "by", "can", "does", "for", "from",
    "how", "in", "is", "it", "of", "on", "or", "the", "this", "to", "what", "when",
    "where", "which", "with",
}

SPECIAL_DOCUMENTS = (
    {
        "doc_id": "development-instructions", "path": "AGENTS.md",
        "title": "Optimus Agent engineering rules", "doc_type": "reference",
        "plane": "current", "status": "current", "authority": "canonical",
        "summary": "Repository development law for humans and coding agents.",
    },
    {
        "doc_id": "runtime-constitution", "path": "OPTIMUS_AGENTS.md",
        "title": "Optimus Agent runtime constitution", "doc_type": "reference",
        "plane": "current", "status": "current", "authority": "canonical",
        "summary": "Installed Optimus product-session constitution.",
    },
    {
        "doc_id": "repository-readme", "path": "README.md",
        "title": "Optimus Agent", "doc_type": "how-to", "plane": "current",
        "status": "current", "authority": "supporting",
        "summary": "Product landing, installation, build, and test entrypoint.",
    },
)


class DocsError(RuntimeError):
    """The documentation contract is incomplete or contradictory."""


@dataclass(frozen=True)
class Document:
    path: Path
    relative: str
    metadata: dict[str, Any]
    title: str
    headings: tuple[str, ...]
    content_sha256: str

    def entry(self) -> dict[str, Any]:
        return {
            "id": self.metadata["doc_id"],
            "path": self.relative,
            "title": self.title,
            "type": self.metadata["doc_type"],
            "plane": self.metadata["plane"],
            "status": self.metadata["status"],
            "authority": self.metadata["authority"],
            "summary": self.metadata["summary"],
            "reviewed_on": self.metadata["reviewed_on"],
            "review_by": self.metadata["review_by"],
            "headings": list(self.headings),
            "content_sha256": self.content_sha256,
        }


def parse_scalar(value: str) -> Any:
    value = value.strip()
    if value in {"null", "~"}:
        return None
    if value in {"true", "false"}:
        return value == "true"
    if len(value) >= 2 and value[0] == value[-1] and value[0] in {'"', "'"}:
        return value[1:-1]
    return value


def parse_frontmatter(path: Path) -> dict[str, Any]:
    lines = path.read_text(encoding="utf-8").splitlines()
    if not lines or lines[0] != "---":
        return {}
    try:
        end = lines.index("---", 1)
    except ValueError as error:
        raise DocsError(f"unterminated frontmatter: {path.relative_to(ROOT)}") from error
    result: dict[str, Any] = {}
    index = 1
    while index < end:
        raw = lines[index]
        if not raw.strip() or raw.lstrip().startswith("#"):
            index += 1
            continue
        if raw.startswith(" ") or ":" not in raw:
            raise DocsError(f"unsupported frontmatter syntax: {path.relative_to(ROOT)}:{index + 1}")
        key, value = raw.split(":", 1)
        key = key.strip()
        if value.strip():
            result[key] = parse_scalar(value)
            index += 1
            continue
        values: list[Any] = []
        index += 1
        while index < end and lines[index].startswith("  - "):
            values.append(parse_scalar(lines[index][4:]))
            index += 1
        result[key] = values
    return result


def markdown_body(text: str) -> str:
    if text.startswith("---\n"):
        marker = text.find("\n---\n", 4)
        if marker >= 0:
            return text[marker + 5:]
    return text


def heading_slug(value: str) -> str:
    value = re.sub(r"<[^>]+>", "", value.casefold())
    value = re.sub(r"[^\w\s-]", "", value, flags=re.UNICODE)
    return re.sub(r"[\s-]+", "-", value).strip("-")


def heading_anchors(text: str) -> set[str]:
    anchors: set[str] = set()
    counts: dict[str, int] = {}
    fenced = False
    for line in markdown_body(text).splitlines():
        if line.lstrip().startswith("```") or line.lstrip().startswith("~~~"):
            fenced = not fenced
            continue
        if fenced:
            continue
        match = re.match(r"^#{1,6}\s+(.+?)\s*#*\s*$", line)
        if not match:
            continue
        base = heading_slug(match.group(1))
        if not base:
            continue
        count = counts.get(base, 0)
        anchor = base if count == 0 else f"{base}-{count}"
        counts[base] = count + 1
        anchors.add(anchor)
    return anchors


def document_paths() -> list[Path]:
    return sorted(
        path for path in DOCS.rglob("*.md")
        if path.resolve() not in GENERATED
    )


def load_documents() -> list[Document]:
    documents: list[Document] = []
    errors: list[str] = []
    ids: dict[str, str] = {}
    today = dt.date.today()
    for path in document_paths():
        relative = path.relative_to(ROOT).as_posix()
        metadata = parse_frontmatter(path)
        missing = sorted(REQUIRED - set(metadata))
        if missing:
            errors.append(f"{relative}: missing metadata {', '.join(missing)}")
            continue
        if metadata["doc_type"] not in DOC_TYPES:
            errors.append(f"{relative}: invalid doc_type {metadata['doc_type']!r}")
        if metadata["plane"] not in PLANES:
            errors.append(f"{relative}: invalid plane {metadata['plane']!r}")
        if metadata["status"] not in STATUSES:
            errors.append(f"{relative}: invalid status {metadata['status']!r}")
        if metadata["authority"] not in AUTHORITIES:
            errors.append(f"{relative}: invalid authority {metadata['authority']!r}")
        doc_id = str(metadata["doc_id"])
        if not re.fullmatch(r"[a-z0-9][a-z0-9.-]{1,95}", doc_id):
            errors.append(f"{relative}: invalid doc_id {doc_id!r}")
        elif doc_id in ids:
            errors.append(f"{relative}: duplicate doc_id also used by {ids[doc_id]}")
        ids[doc_id] = relative
        status = metadata["status"]
        plane = metadata["plane"]
        authority = metadata["authority"]
        summary = str(metadata["summary"]).strip()
        if len(summary) < 24:
            errors.append(f"{relative}: summary is too short to support retrieval")
        if plane == "history" and status != "historical":
            errors.append(f"{relative}: history plane must be historical")
        if status == "historical" and authority not in {"historical", "record"}:
            errors.append(f"{relative}: historical document cannot claim current authority")
        if plane == "current" and status not in {"current", "planned"}:
            errors.append(f"{relative}: current plane has non-current lifecycle")
        reviewed_on = str(metadata["reviewed_on"])
        try:
            reviewed = dt.date.fromisoformat(reviewed_on)
        except ValueError:
            errors.append(f"{relative}: reviewed_on must be an ISO date")
            reviewed = None
        else:
            if reviewed > today:
                errors.append(f"{relative}: reviewed_on cannot be in the future")
        review_by = str(metadata["review_by"])
        if status in {"current", "planned"}:
            try:
                deadline = dt.date.fromisoformat(review_by)
            except ValueError:
                errors.append(f"{relative}: current document needs ISO review_by")
            else:
                if deadline < today:
                    errors.append(f"{relative}: semantic review expired on {deadline}")
                if reviewed is not None and deadline < reviewed:
                    errors.append(f"{relative}: review_by precedes reviewed_on")
        elif review_by != "never":
            errors.append(f"{relative}: non-current review_by must be never")
        text = path.read_text(encoding="utf-8")
        title_match = re.search(r"^#\s+(.+?)\s*$", markdown_body(text), re.MULTILINE)
        if not title_match:
            errors.append(f"{relative}: missing H1 title")
            title = relative
        else:
            title = title_match.group(1)
        headings = tuple(
            match.group(1).strip()
            for match in re.finditer(r"^#{1,6}\s+(.+?)\s*#*\s*$", markdown_body(text), re.MULTILINE)
        )
        documents.append(Document(
            path, relative, metadata, title, headings,
            hashlib.sha256(text.encode()).hexdigest(),
        ))
    if errors:
        raise DocsError("\n".join(errors))
    return documents


def load_routes() -> dict[str, Any]:
    try:
        payload = json.loads(ROUTES.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise DocsError(f"cannot load authority routes: {error}") from error
    if payload.get("schema_version") != 1 or not payload.get("routes"):
        raise DocsError("authority routes must be schema version 1 and non-empty")
    return payload


def special_entries() -> list[dict[str, Any]]:
    entries = []
    for raw in SPECIAL_DOCUMENTS:
        path = ROOT / raw["path"]
        if not path.is_file():
            raise DocsError(f"special authority is missing: {raw['path']}")
        text = path.read_text(encoding="utf-8")
        entries.append({
            "id": raw["doc_id"], "path": raw["path"], "title": raw["title"],
            "type": raw["doc_type"], "plane": raw["plane"], "status": raw["status"],
            "authority": raw["authority"], "summary": raw["summary"],
            "reviewed_on": "2026-07-31", "review_by": "2026-10-31",
            "headings": [], "content_sha256": hashlib.sha256(text.encode()).hexdigest(),
        })
    return entries


def validate_routes(entries: list[dict[str, Any]], routes: dict[str, Any]) -> None:
    by_id = {entry["id"]: entry for entry in entries}
    seen: set[str] = set()
    primary_owner: dict[str, str] = {}
    errors: list[str] = []
    for route in routes["routes"]:
        route_id = route.get("id", "")
        if not re.fullmatch(r"[a-z0-9][a-z0-9.-]+", route_id) or route_id in seen:
            errors.append(f"invalid or duplicate route id: {route_id!r}")
        seen.add(route_id)
        primary = route.get("primary", [])
        if len(primary) != 1:
            errors.append(f"{route_id}: exactly one primary authority is required")
            continue
        for doc_id in primary + route.get("supporting", []):
            if doc_id not in by_id:
                errors.append(f"{route_id}: unknown document id {doc_id}")
        owner = primary[0]
        if owner in by_id:
            entry = by_id[owner]
            if entry["status"] != "current" or entry["plane"] in {"history", "evidence"}:
                errors.append(f"{route_id}: primary authority is not current: {owner}")
            if entry["authority"] != "canonical":
                errors.append(f"{route_id}: primary authority must be canonical: {owner}")
        if route_id in primary_owner:
            errors.append(f"duplicate exclusive authority scope: {route_id}")
        primary_owner[route_id] = owner
        if not route.get("keywords"):
            errors.append(f"{route_id}: keywords are required for bounded retrieval")
    required = set(routes.get("required_routes", []))
    missing = sorted(required - seen)
    if missing:
        errors.append(f"missing required authority routes: {', '.join(missing)}")
    primary_ids = set(primary_owner.values())
    orphaned = sorted(
        entry["id"] for entry in entries
        if entry["status"] == "current"
        and entry["authority"] == "canonical"
        and entry["id"] not in primary_ids
    )
    if orphaned:
        errors.append(f"canonical documents without an authority route: {', '.join(orphaned)}")
    if errors:
        raise DocsError("\n".join(errors))


def strip_fenced(text: str) -> str:
    output: list[str] = []
    fenced = False
    for line in text.splitlines():
        if line.lstrip().startswith("```") or line.lstrip().startswith("~~~"):
            fenced = not fenced
            output.append("")
        elif fenced:
            output.append("")
        else:
            output.append(line)
    return "\n".join(output)


def link_targets(text: str) -> Iterable[str]:
    text = strip_fenced(text)
    definitions = {
        key.casefold(): target.strip("<>")
        for key, target in re.findall(r"^\s*\[([^]]+)\]:\s*(\S+)", text, re.MULTILINE)
    }
    for target in re.findall(r"!?\[[^]]*\]\(([^)]+)\)", text):
        yield target.split(maxsplit=1)[0].strip("<>")
    for key in re.findall(r"!?\[[^]]+\]\[([^]]+)\]", text):
        if key.casefold() in definitions:
            yield definitions[key.casefold()]


def validate_links(documents: list[Document]) -> None:
    anchor_cache: dict[Path, set[str]] = {}
    errors: list[str] = []
    for document in documents:
        text = document.path.read_text(encoding="utf-8")
        for raw in link_targets(text):
            if raw.startswith(("http://", "https://", "mailto:", "data:")):
                continue
            decoded = urllib.parse.unquote(raw)
            target_text, _, fragment = decoded.partition("#")
            if not target_text:
                target = document.path
            else:
                candidate = Path(target_text)
                if candidate.is_absolute():
                    errors.append(f"{document.relative}: absolute local link is forbidden: {raw}")
                    continue
                target = (document.path.parent / candidate).resolve()
                try:
                    target.relative_to(ROOT)
                except ValueError:
                    errors.append(f"{document.relative}: link escapes repository: {raw}")
                    continue
            if not target.exists():
                errors.append(f"{document.relative}: missing link target: {raw}")
                continue
            if fragment and target.is_file() and target.suffix.casefold() == ".md":
                anchors = anchor_cache.setdefault(
                    target, heading_anchors(target.read_text(encoding="utf-8"))
                )
                if fragment.casefold() not in anchors:
                    errors.append(f"{document.relative}: missing heading anchor: {raw}")
    if errors:
        raise DocsError("\n".join(errors))


def glob_files(pattern: str) -> list[Path]:
    pattern = pattern.strip()
    candidate = ROOT / pattern
    if not any(char in pattern for char in "*?["):
        return [candidate] if candidate.is_file() else []
    return sorted(path for path in ROOT.glob(pattern) if path.is_file())


def binding_digest(document: Document) -> tuple[str, int] | None:
    if document.metadata["status"] != "current" or "knowledge_type" not in document.metadata:
        return None
    patterns: list[str] = []
    for key in ("owns", "covers", "depends_on", "validated_by"):
        value = document.metadata.get(key, [])
        if isinstance(value, list):
            patterns.extend(str(item) for item in value)
    files: dict[str, Path] = {}
    for pattern in patterns:
        for path in glob_files(pattern):
            # Generated catalog and lock files are outputs of this system. A
            # lock that hashes itself can never converge, and generated views
            # are already checked byte-for-byte against their source inputs.
            if path.resolve() in GENERATED:
                continue
            files[path.relative_to(ROOT).as_posix()] = path
    if not files:
        raise DocsError(f"{document.relative}: current source-bound document resolves no bindings")
    digest = hashlib.sha256()
    for relative, path in sorted(files.items()):
        digest.update(relative.encode())
        digest.update(b"\0")
        digest.update(hashlib.sha256(path.read_bytes()).digest())
    return digest.hexdigest(), len(files)


def expected_lock(documents: list[Document]) -> dict[str, dict[str, Any]]:
    result: dict[str, dict[str, Any]] = {}
    for document in documents:
        if document.metadata["status"] not in {"current", "planned"}:
            continue
        binding = binding_digest(document)
        result[document.metadata["doc_id"]] = {
            "path": document.relative,
            "document_sha256": document.content_sha256,
            "binding_sha256": binding[0] if binding else None,
            "resolved_files": binding[1] if binding else 0,
        }
    return result


def load_lock() -> dict[str, Any]:
    try:
        payload = json.loads(LOCK.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise DocsError(f"cannot load durable documentation verification lock: {error}") from error
    if payload.get("schema_version") != 1 or not isinstance(payload.get("documents"), dict):
        raise DocsError("documentation verification lock has invalid schema")
    return payload


def validate_lock(documents: list[Document]) -> None:
    expected = expected_lock(documents)
    actual = load_lock()["documents"]
    if expected != actual:
        missing = sorted(set(expected) - set(actual))
        extra = sorted(set(actual) - set(expected))
        changed = sorted(key for key in set(expected) & set(actual) if expected[key] != actual[key])
        raise DocsError(
            "documentation source-binding lock is stale; run explicit docs-refresh. "
            f"missing={missing} extra={extra} changed={changed}"
        )


def catalog_payload(documents: list[Document], routes: dict[str, Any]) -> dict[str, Any]:
    entries = [document.entry() for document in documents] + special_entries()
    entries.sort(key=lambda item: item["path"])
    digest = hashlib.sha256()
    for entry in entries:
        digest.update(entry["path"].encode())
        digest.update(bytes.fromhex(entry["content_sha256"]))
    return {
        "schema_version": 1,
        "documents_sha256": digest.hexdigest(),
        "default_planes": ["current", "decision", "work"],
        "documents": entries,
        "routes": routes["routes"],
    }


def catalog_markdown(payload: dict[str, Any]) -> str:
    groups: dict[str, list[dict[str, Any]]] = {plane: [] for plane in PLANES}
    for entry in payload["documents"]:
        groups.setdefault(entry["plane"], []).append(entry)
    lines = [
        "<!-- Generated by scripts/docs_system.py; do not edit manually. -->",
        "# Optimus Agent documentation catalog", "",
        f"Catalog identity: `{payload['documents_sha256']}`", "",
        "Start with [docs/README.md](README.md). Historical and evidence records are excluded from default retrieval.",
    ]
    for plane in ("current", "work", "decision", "evidence", "history"):
        lines.extend(["", f"## {plane.title()}", ""])
        for entry in sorted(groups.get(plane, []), key=lambda item: (item["title"].casefold(), item["path"])):
            path = entry["path"]
            link = f"../{path}" if not path.startswith("docs/") else path.removeprefix("docs/")
            lines.append(
                f"- [{entry['title']}]({link}) — {entry['summary']} "
                f"`{entry['status']}` `{entry['authority']}`"
            )
    return "\n".join(lines) + "\n"


def canonical_json(payload: Any) -> str:
    return json.dumps(payload, indent=2, sort_keys=True, ensure_ascii=False) + "\n"


def generate(documents: list[Document], routes: dict[str, Any]) -> None:
    payload = catalog_payload(documents, routes)
    CATALOG_JSON.write_text(canonical_json(payload), encoding="utf-8")
    CATALOG_MD.write_text(catalog_markdown(payload), encoding="utf-8")


def refresh(documents: list[Document], doc_ids: list[str]) -> None:
    payload = load_lock()
    expected = expected_lock(documents)
    unknown = sorted(set(doc_ids) - set(expected))
    if unknown:
        raise DocsError(f"cannot refresh unknown or non-source-bound doc ids: {unknown}")
    for doc_id in doc_ids:
        payload["documents"][doc_id] = expected[doc_id]
    LOCK.write_text(canonical_json(payload), encoding="utf-8")


def load_catalog_or_build(documents: list[Document], routes: dict[str, Any]) -> dict[str, Any]:
    return catalog_payload(documents, routes)


def tokens(value: str) -> set[str]:
    return {
        token for token in re.findall(r"[a-z0-9][a-z0-9_-]+", value.casefold())
        if token not in STOP_WORDS
    }


def search(payload: dict[str, Any], query: str, include_records: bool = False) -> list[dict[str, Any]]:
    query_tokens = tokens(query)
    boosts: dict[str, float] = {}
    route_matches: list[tuple[int, dict[str, Any]]] = []
    for route in payload["routes"]:
        id_overlap = len(query_tokens & tokens(route["id"]))
        keyword_overlap = len(query_tokens & tokens(" ".join(route.get("keywords", []))))
        relevance = (3 * id_overlap) + keyword_overlap
        if relevance:
            route_matches.append((relevance, route))
    if route_matches:
        strongest = max(score for score, _ in route_matches)
        for relevance, route in route_matches:
            if relevance != strongest:
                continue
            for doc_id in route["primary"]:
                boosts[doc_id] = boosts.get(doc_id, 0) + 30 * relevance
            for doc_id in route.get("supporting", []):
                boosts[doc_id] = boosts.get(doc_id, 0) + 5 * relevance
    scored = []
    for entry in payload["documents"]:
        if not include_records and entry["plane"] in {"history", "evidence"}:
            continue
        title = tokens(entry["title"])
        summary = tokens(entry["summary"])
        path = tokens(entry["path"])
        headings = tokens(" ".join(entry.get("headings", [])))
        score = (
            4 * len(query_tokens & title)
            + 3 * len(query_tokens & summary)
            + 2 * len(query_tokens & headings)
            + len(query_tokens & path)
            + boosts.get(entry["id"], 0)
            + (2 if entry["authority"] == "canonical" else 0)
        )
        if score > 0:
            scored.append({**entry, "score": score})
    return sorted(scored, key=lambda item: (-item["score"], item["path"]))


def benchmark(payload: dict[str, Any], routes: dict[str, Any]) -> dict[str, Any]:
    suite = json.loads(BENCHMARK.read_text(encoding="utf-8"))
    route_by_id = {route["id"]: route for route in routes["routes"]}
    failures = []
    top_one = 0
    results = []
    for case in suite["questions"]:
        expected = route_by_id[case["route"]]["primary"][0]
        found = search(payload, case["question"])
        top = [entry["id"] for entry in found[:3]]
        if top and top[0] == expected:
            top_one += 1
        if expected not in top:
            failures.append({"id": case["id"], "expected": expected, "top": top})
        results.append({"id": case["id"], "expected": expected, "top": top})
    total = len(suite["questions"])
    top_one_rate = top_one / total if total else 0
    if failures or top_one_rate < 0.95:
        raise DocsError(f"documentation authority benchmark failed: top_one={top_one_rate:.1%} failures={failures}")
    return {"questions": total, "top_three": total - len(failures), "top_one_rate": top_one_rate, "results": results}


def check_generated(documents: list[Document], routes: dict[str, Any]) -> None:
    expected = catalog_payload(documents, routes)
    try:
        actual = json.loads(CATALOG_JSON.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise DocsError(f"generated catalog is missing or invalid: {error}") from error
    if actual != expected:
        raise DocsError("generated docs/catalog.json is stale; run just docs-generate")
    if CATALOG_MD.read_text(encoding="utf-8") != catalog_markdown(expected):
        raise DocsError("generated docs/CATALOG.md is stale; run just docs-generate")


def validate_all(documents: list[Document], routes: dict[str, Any]) -> dict[str, Any]:
    entries = [document.entry() for document in documents] + special_entries()
    validate_routes(entries, routes)
    validate_links(documents)
    validate_lock(documents)
    check_generated(documents, routes)
    result = benchmark(catalog_payload(documents, routes), routes)
    return {
        "documents": len(documents),
        "routes": len(routes["routes"]),
        "benchmark_questions": result["questions"],
        "benchmark_top_one": result["top_one_rate"],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("check")
    sub.add_parser("generate")
    refresh_parser = sub.add_parser("refresh")
    refresh_parser.add_argument("doc_id", nargs="+")
    search_parser = sub.add_parser("search")
    search_parser.add_argument("query")
    search_parser.add_argument("--include-records", action="store_true")
    context_parser = sub.add_parser("context")
    context_parser.add_argument("route")
    sub.add_parser("benchmark")
    args = parser.parse_args()
    try:
        documents = load_documents()
        routes = load_routes()
        entries = [document.entry() for document in documents] + special_entries()
        validate_routes(entries, routes)
        if args.command == "generate":
            generate(documents, routes)
            print(f"DOCS_GENERATED documents={len(documents)}")
        elif args.command == "refresh":
            refresh(documents, args.doc_id)
            print(f"DOCS_REFRESHED ids={','.join(args.doc_id)}")
        elif args.command == "search":
            payload = load_catalog_or_build(documents, routes)
            for entry in search(payload, args.query, args.include_records)[:10]:
                print(f"{entry['score']:>5} {entry['id']} {entry['path']} — {entry['summary']}")
        elif args.command == "context":
            route = next((item for item in routes["routes"] if item["id"] == args.route), None)
            if route is None:
                raise DocsError(f"unknown authority route: {args.route}")
            by_id = {entry["id"]: entry for entry in entries}
            for role, ids in (("PRIMARY", route["primary"]), ("SUPPORTING", route.get("supporting", []))):
                for doc_id in ids:
                    entry = by_id[doc_id]
                    print(f"{role} {entry['path']} — {entry['summary']}")
        elif args.command == "benchmark":
            result = benchmark(catalog_payload(documents, routes), routes)
            print(f"DOCS_BENCHMARK_OK questions={result['questions']} top_one={result['top_one_rate']:.1%}")
        else:
            result = validate_all(documents, routes)
            print(
                "DOCS_CHECK_OK "
                f"documents={result['documents']} routes={result['routes']} "
                f"benchmark={result['benchmark_questions']} top_one={result['benchmark_top_one']:.1%}"
            )
    except (DocsError, OSError, json.JSONDecodeError) as error:
        print(f"DOCS_CHECK_FAILED\n{error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
