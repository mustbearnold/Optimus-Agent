---
doc_id: spec-020-integrations-breadth
doc_type: reference
plane: work
status: current
authority: canonical
summary: External integrations for Optimus — a real MCP client (streamable HTTP + stdio) mapping MCP tools into ToolDesc under the pack allowlist (never a second catalog), a first-party token-gated GitHub REST integration (issues + repo metadata, least-privilege), a Home Assistant REST integration filling the Home pack, and a read-only PostgreSQL query tool with protocol-enforced read-only transactions — all config-gated, fail-closed, secret-disciplined, and mock-server tested in CI.
reviewed_on: 2026-08-08
review_by: 2026-11-08
knowledge_type: specification
covers:
  - crates/optimus-host/src/extensibility.rs
  - crates/optimus-packs/src/catalog.rs
  - crates/optimus-packs/src/signed.rs
  - apps/optimus-cli/src/doctor.rs
depends_on:
  - docs/decisions/0068-a-catalog-row-must-dispatch-or-not-exist.md
  - specs/019-tool-catalog-breadth/spec.md
---

# Spec-020: Integrations breadth — external capability sources

Status: current
Owner: optimus-agent-development (prompt-only owner)

## Revision table

| Round | Verdict | Findings | Fixes |
|---|---|---|---|
| 1 | APPROVED | 4 non-blocking nits (doctor contract wording, MCP read/write classification, GitHub scopes doc target, db_query bound safety) | All 4 applied 2026-08-08 (doctor exit-1 wording, classification mechanism pinned, integrations-github.md named, db_unbounded_query) |

## Purpose

Optimus has no product-level external integrations: no real MCP client
(the P27 MCP surface loads a mock session by default), no GitHub,
no Home Assistant (the Home pack is an empty shell), and no database
connectivity. Hermes, the parity target, ships MCP support and
first-party integrations (Google Workspace, GitHub, Notion, …).

This spec makes external capability sources a governed mechanism
instead of a list of ad-hoc tools: a REAL MCP client whose tools map
into `ToolDesc` under the pack allowlist (never a second tool
catalog — the existing `extensibility.rs` design note is made law), a
first-party GitHub REST integration for issues + repo metadata, a
Home Assistant integration that finally fills the Home pack, and a
read-only PostgreSQL query tool with protocol-level read-only
enforcement. Every integration is config-gated, fail-closed,
secret-disciplined, and covered by mock-server tests in CI.

## Current state (Confirmed behaviour)

- `crates/optimus-host/src/extensibility.rs` (program P27) exposes
  `mcp_status` and `mcp_tools` IPC that load an MCP session via
  `load_mcp_session(home)` and fall back to `default_mock_session()`;
  transports are `McpTransportKind::Http` and `Stdio`; the design
  note in the source is explicit: "MCP maps to ToolDesc under pack
  allowlist; never a second tool catalog" (Confirmed: source).
- Signed pack scaffolding exists (`crates/optimus-packs/src/signed.rs`
  validates pack signatures; test fixture `example.mcp` with
  `mcp_echo` tool) — the signing/verification path for external packs
  is in place (Confirmed: source).
- The Home pack is empty ("no tools until integrated; ADR-0068") and
  no GitHub/database integration code exists in the pack catalog or
  CLI (Confirmed: catalog + grep).
- ADR-0068 requires every catalog row to dispatch or not exist
  (Confirmed: ADR).
- Secrets law: secrets never enter the repo, specs, or reports; the
  doctor (P18) inventories durability state and flags issues, exiting
  1 on any issue (Confirmed: constitution, `apps/optimus-cli/src/doctor.rs`).

## Requirements

### R1. Real MCP client

- Optimus MUST implement a real MCP client over streamable HTTP and
  stdio transports (both `McpTransportKind` variants), replacing the
  mock-default fallback for CONFIGURED servers (MUST; the mock remains
  only for CI fixtures, never as a runtime default when a server is
  configured).
- MCP servers MUST be declared in the Optimus config as a registry
  entry: name, transport kind, URL (HTTP) or command + args (stdio),
  allowlisted tool ids, and a policy default (`read-only` |
  `write-approval`) (MUST).
- MCP tools MUST map into `ToolDesc` under the pack allowlist and MUST
  NOT create a second tool catalog — the `extensibility.rs` design
  note is normative (MUST).
- An MCP server that is not configured, or a tool id not on its
  allowlist, MUST fail closed with the named diagnostic
  `mcp_server_unconfigured` / `mcp_tool_not_allowlisted` (MUST).
- MCP calls MUST have a bounded timeout and bounded response size;
  overflow or timeout MUST produce a named diagnostic, never a
  hang or a truncated silent success (MUST).
- Server tokens/headers MUST live only in the config home with mode
  0600 (MUST; R6).

### R2. MCP policy enforcement

- A server declared `read-only` MUST be enforced: write-capable MCP
  tools from that server are refused with
  `mcp_server_read_only` before any network call (MUST).
- Read/write classification of MCP tools MUST be pinned by mechanism:
  the per-server config policy is the authority, defaulting to
  `write-approval` for tools the server does not classify; MCP's
  `toolAnnotations.readOnlyHint` is honored where the server offers
  it. A server declared `write-approval` MUST route every potentially
  mutating tool call through SmartDeny approval (MUST).
- MCP tool results MUST conform to the canonical tool output schema
  with a replay class (external_nondeterministic for server calls)
  and provenance recording the server + tool id (MUST).

### R3. First-party GitHub integration

- Optimus MUST ship a token-gated GitHub REST integration with tools:
  `github_issue_list` (read), `github_issue_create` (write),
  `github_issue_comment` (write), `github_repo_view` (read)
  (MUST).
- The token MUST be least-privilege (scopes documented per tool in
  `docs/architecture/integrations-github.md`), stored in the config
  home with mode 0600, and MUST never appear in tool output, logs, or
  diagnostics (MUST).
- HTTP 401/403/429 MUST map to named diagnostics
  (`github_unauthorized` / `github_forbidden` /
  `github_rate_limited`) with retry-after guidance on 429 (MUST).
- The integration MUST use the GitHub REST API directly (no external
  `gh` binary dependency) so it works in the product's sandbox (MUST).
- PR mutation and repository administration are out of scope for v1
  (MAY later) (MUST NOT in v1).

### R4. Home Assistant integration (fills the Home pack)

- Optimus MUST ship a Home Assistant REST integration with tools:
  `ha_entity_state` (read), `ha_entity_set` (write),
  `ha_service_call` (write) (MUST).
- `ha_entity_set` and `ha_service_call` MUST require SmartDeny
  approval (device control is a high-risk effect) (MUST).
- Config: HA base URL + long-lived access token in the config home
  mode 0600; unavailable/unauthenticated HA MUST fail closed with
  named diagnostics `ha_unreachable` / `ha_unauthorized` (MUST).
- The Home pack's catalog shell ("no tools until integrated;
  ADR-0068") MUST be replaced by the real tool rows in the same change
  that lands the handlers (MUST; ADR-0068).
- Landing the Home pack tools MUST also widen the activation enum
  (`ActivatePack`/`ReleasePack` name enums in
  `crates/optimus-packs/src/catalog.rs`, currently
  `["browser","desktop","media","devex","social"]` — `home` and
  `office` are deliberately outside) and MUST replace the
  unreachable-pack rejection pins in
  `crates/optimus-kernel/tests/tool_coverage.rs` with coverage for
  what the widening unlocks, in the same commit (MUST; spec-019 R2
  ceremony applies to every tool in this spec, including the Home
  pack).
- Entity-state reads MUST return the raw entity JSON bounded to the
  requested entities, with provenance (MUST).

### R5. Read-only database query

- Optimus MUST ship `db_query`, a PostgreSQL client tool that runs a
  single query in a protocol-enforced read-only transaction:
  `BEGIN READ ONLY` (MUST).
- The tool MUST refuse statements that are not a read: a statement
  parser rejects `INSERT/UPDATE/DELETE/DDL/COPY` up front with
  `db_write_refused`, and the read-only transaction is the backstop
  (MUST).
- Row counts MUST be bounded: reject any query whose SQL structure
  cannot provably respect the bound (queries already containing
  LIMIT/OFFSET/UNION or outer subqueries are rejected with
  `db_unbounded_query`), and append a server-side `LIMIT` only where
  provably safe; the bound is recorded in the tool output (MUST).
- Databases MUST be declared in the config (name, host, db, user,
  TLS mode) with the password in the config home mode 0600; connect,
  auth, and parse failures MUST be named diagnostics
  (`db_unreachable` / `db_auth_failed` / `db_parse_failed`) (MUST).
- Secrets MUST NOT appear in output or diagnostics (MUST).

### R6. Registry and doctor discipline

- Every integration MUST be a config-gated registry entry; an
  integration with no config MUST fail closed with its named
  diagnostic and MUST NOT attempt network calls (MUST).
- `optimus doctor` MUST report per-integration configured state
  (configured / unconfigured / error) WITHOUT leaking tokens or
  connection details (MUST; doctor's existing issue contract — a
  misconfigured integration is a named issue, exit 1).
- Integration config changes MUST be reflected without a restart
  where the mechanism allows (re-read on use), or the refresh
  behavior MUST be documented (MAY).

### R7. Testing discipline

- Every integration MUST have a mock-server integration test suite in
  CI (the existing mock-transport pattern: `stdio_mock_bind` /
  `http_mock_bind` / fixture tokens) exercising: happy path, fail-
  closed unconfigured, auth failure, and timeout paths (MUST).
- A mutation test MUST assert that a disabled/unconfigured integration
  refuses with the named diagnostic (delete the config → the tool
  fails, not passes) (MUST).
- Live-transport smoke tests (real GitHub/HA/Postgres) are
  manual/flagged, never CI-blocking (MUST).

## Acceptance criteria

- [ ] A1. Given an MCP server declared in config (stdio fixture in
  CI), when `mcp_tools` lists tools and a tool call executes, then the
  allowlisted tools appear under the pack and the call round-trips;
  given an undeclared server, then `mcp_server_unconfigured` is
  returned (R1).
- [ ] A2. Given a read-only-declared MCP server, when a write-capable
  tool is invoked, then `mcp_server_read_only` is returned with zero
  network calls; given a write-approval server, then SmartDeny gates
  the call (R2).
- [ ] A3. Given a fixture GitHub token against a mock REST server,
  when `github_issue_create` runs, then the created issue is returned
  with provenance and the token never appears in output; given a 403
  fixture, then `github_forbidden` is returned (R3).
- [ ] A4. Given a mock HA server, when `ha_entity_state` reads, then
  the entity JSON returns; when `ha_entity_set` runs without
  approval, then SmartDeny blocks it; with approval, then the call
  round-trips (R4).
- [ ] A5. Given a scratch read-only PostgreSQL fixture, when
  `db_query` runs a SELECT, then bounded rows return; when a write
  statement is attempted, then `db_write_refused` is returned and the
  connection shows no write (R5).
- [ ] A6. Given integrations with a misconfigured entry, when
  `optimus doctor` runs, then the entry is reported as a named issue
  (exit 1) without leaking its token (R6).
- [ ] A7. Given the full implementation, when the mock-server suites
  run in CI, then every integration's happy/fail-closed/auth/timeout
  paths pass and the mutation test refuses on deleted config (R7).
- [ ] A8. Given the Home pack tools landing, when `just verify` runs,
  then the activation enum includes `home`, the tool_coverage
  rejection pins are replaced by coverage, and the gate passes with
  zero skips (R4).

## Out of scope

- OAuth flows and browser-based token acquisition (v1 tokens are
  config-pasted; MAY later).
- First-party Google Workspace / Notion / calendar / email
  integrations (reachable via MCP servers in v1; first-party later).
- GitHub PR mutation, repo administration, and webhooks.
- Non-PostgreSQL databases (v1 is Postgres; the tool contract is
  extensible).
- Complex ETL or long-running queries (bounded single queries only).

## Open questions

- MCP auth beyond bearer-token headers (OAuth2 for remote servers) —
  v1 supports static bearer tokens; OAuth2 MAY follow.
- Whether `db_query` should also support a schema-introspection tool
  (`db_schema`) in v1 — default: yes, read-only `information_schema`
  queries via the same tool.
- GitHub write scope expansion to PRs in v2 — explicitly deferred.

## Links

- `crates/optimus-host/src/extensibility.rs` — the P27 MCP surface
  this spec makes real; its "never a second tool catalog" note is
  normative (R1).
- `crates/optimus-packs/src/signed.rs` — the signed-pack
  verification path external servers ride on.
- `docs/decisions/0068-a-catalog-row-must-dispatch-or-not-exist.md`
  — the no-placeholder law (R4 Home pack).
- `docs/decisions/0060-owned-localhost-is-a-process-bound-lease.md` —
  operator-services lineage (registry discipline pattern).
- `specs/019-tool-catalog-breadth/spec.md` — the pack-content spec
  (ceremony + activation-enum widening discipline); this spec designs
  the Home pack tools.
- `apps/optimus-cli/src/doctor.rs` — the doctor issue contract (R6).
