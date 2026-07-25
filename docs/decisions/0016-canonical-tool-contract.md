# ADR-0016-A: Canonical tool and pack contract

> **Alias:** ADR-0016-A (file historically numbered `0016-canonical-tool-contract.md`).
> Distinct from ADR-0016-B (FS sandbox allowlist). See decisions index.

- **Status:** Accepted (PF-04); current behaviour Confirmed for pack/`ToolDesc` contract
- **Date:** historical (PF-04); identity alias formalized 2026-07-25 (P16)

## Context

Tool identity, provider schema, pack membership, kernel dispatch, policy classification, desktop catalog data, and eval matching previously drifted across separate string matches and projections. The kernel would execute known handlers even when their pack was not loaded, unknown handlers returned an in-band pseudo-success, and provider adapters replaced every input schema with an unconstrained object.

## Decision

`optimus-packs` owns the canonical `ToolDesc` contract:

- `ToolId`: stable serialized identity used by providers, transcripts, UI, and evals;
- `input_schema`: the provider-visible and runtime-validated JSON object schema;
- `ToolPolicy`: the security/effect classification;
- `ToolInvocation`: the exhaustive kernel invocation target or `unavailable`;
- `schema_tokens`: provider-visible schema budget contribution;
- pack ownership, description, and availability.

`CapabilitySession` validates catalogs before use. Duplicate tool IDs, pack/descriptor drift, invocation/ID drift, policy drift, malformed schemas, availability/schema-token drift, schema-token arithmetic overflow, and a core pack that exceeds the configured schema budget fail closed. Available tools must have nonzero schema cost; unavailable tools must have zero cost. Pack and loaded-state totals use checked aggregation. Persisted pack restoration rejects unknown/retired pack IDs and re-applies the current pack count and schema-token budgets.

Before any effect, kernel dispatch must:

1. resolve the tool from the catalog;
2. prove its owning pack is loaded;
3. prove the descriptor is available;
4. validate arguments against the same schema sent to providers;
5. dispatch exhaustively by `ToolInvocation`;
6. require the runtime policy decision before any high-risk effect.

Unknown, unloaded, unavailable, malformed, or invalid calls return typed errors. Each model step freezes the exact advertised `ToolId` set and prevalidates the entire returned call batch—including non-empty names, non-empty and sibling-unique call IDs, and arguments—before any sibling effect; pack activation exposes tools only on the next model step. Provider calls require a non-empty call ID, non-empty canonical name, recognized call-type discriminator, and an explicit string `arguments` field; zero-argument tools must still send valid `{}`. Present tool-call/output containers must have the declared array shape. SSE `data:` fields are recognized with or without the optional post-colon space; malformed JSON and empty outcomes are rejected in both streaming paths. Missing, non-string, or malformed JSON arguments are rejected rather than normalized. Every non-empty Codex completed output is parsed strictly in both streaming paths, including after item-level calls, so invalid completed calls cannot be swallowed behind text or another call. Legacy string-only invocation aliases are not canonical identities and are not accepted by dispatch.

`terminal` creates a durable command job, but SmartDeny leaves it awaiting explicit approval and kernel dispatch returns a typed `NeedsApproval` stop. A model-originated tool call cannot manufacture its own approval grant. Desktop `term_run` follows the same pending-first boundary: it returns `AwaitingApproval`, and `approvals_grant` separately grants, resumes, and returns command capture. The command submit path cannot self-grant.

`read_file` resolves through a workspace-root `FsRoots` sandbox. Traversal, absolute paths outside the workspace, symlink escape, and secret basenames are denied before content is read; returned text is bounded and reports truncation.

Provider adapters serialize `ToolDesc.input_schema` unchanged. Human-readable `tool_trace` remains for diagnostics; eval assertions use exact `ToolId` values. Desktop doctor IPC serializes the same pack descriptors for the Capabilities catalog.

Catalogued future tools may remain visible with `ToolInvocation::Unavailable`, zero schema-token cost, and no provider exposure. Availability must become real through a new validated invocation target before a future adapter can expose or execute the tool.

## Supported schema subset

Runtime validation covers the object schemas emitted by the built-in catalog: required properties, `additionalProperties`, primitive types, string-array items, enums, and integer minimums. Catalog construction enforces this same subset, including declared/unique required properties, supported property keywords and types, typed enum values, integer minimums, and string-array item schemas. Unsupported constraints fail catalog construction rather than being sent to providers and ignored at runtime.

## Consequences

- Adding a built-in kernel tool requires one descriptor and one exhaustive invocation arm; catalog validation catches mismatched identity or policy.
- Dynamic MCP tools can extend the same descriptor seam later instead of introducing a parallel schema type.
- Placeholder packs remain catalog-visible but cannot consume model context or execute.
- Session restore can now fail when a persisted pack set violates current catalog or budget policy; this is intentional fail-closed behavior.
- ADR-0012's earlier kernel-issued terminal grant is superseded by this decision.
