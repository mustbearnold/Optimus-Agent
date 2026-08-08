---
doc_id: spec-021-interfaces
doc_type: reference
plane: work
status: current
authority: canonical
summary: External interfaces for Optimus — a real OpenAI-compatible chat-completions server (non-streaming + SSE) backed by the kernel turn loop, a real ACP (Agent Client Protocol) bridge replacing the canned scaffold, a token-gated web dashboard (sessions, gateway status, cron, skills/packs consoles) over the existing 127.0.0.1 HTTP pattern with ADR-0084 websocket tickets, and full interactive multi-tab PTY I/O on Linux.
reviewed_on: 2026-08-08
review_by: 2026-11-08
knowledge_type: specification
covers:
  - crates/optimus-ops/src/surfaces.rs
  - crates/optimus-kernel/tests/openai_http.rs
  - apps/optimus-cli/src/gateway_http.rs
  - apps/optimus-cli/src/main.rs
depends_on:
  - docs/decisions/0083-one-wire-protocol-for-all-surfaces.md
  - docs/decisions/0084-websocket-ticket-and-process-secret-security-model.md
  - specs/015-surface-protocol/spec.md
  - specs/017-gateway-breadth/spec.md
---

# Spec-021: Interfaces — server surfaces, web dashboard, interactive PTY

Status: current
Owner: optimus-agent-development (prompt-only owner)

## Revision table

| Round | Verdict | Findings | Fixes |
|---|---|---|---|
| 1 | REJECTED | B1: R3 one-wire MUST vs polling default contradiction (ADR-0083 = JSON-RPC 2.0 over stdio/WS only); A4 undefined wire shape; B2: serve-subcommand ownership model undefined vs spec-015 R1 one-core-per-home refusal; 4 nits (token source, OpenAI field allowlist, gateway_http.rs description, PTY tab-store scaffold) | B1: R0 attach-as-client model + R3 rewritten (one-wire WS + ADR-0084 tickets, no polling API); B2: core owns home, surfaces attach via loopback WS with coexistence rules; 4 nits applied (round 2) |
| 2 | APPROVED | 3 non-blocking nits (carriers phrasing, exit-code taxonomy, ticket minting authority) | Applied 2026-08-08 (stdio+WS carriers, spec-015 R1 exit codes 2/3, dashboard-minted tickets) |

## Purpose

Optimus's external interface story is scaffolds and claims: the ACP
surface in `crates/optimus-ops/src/surfaces.rs` is a canned smoke
handler, there is no OpenAI-compatible SERVER (only a compatible
client), the only web surface is the bounded token-gated HTTP gateway
surface (health/inbound/drain/inbox/outbox on 127.0.0.1), and
interactive PTY is the scorecard's leading
product loss #2 (Linux multi-tab session store scaffold, no interactive
I/O). Hermes, the parity target, exposes an OpenAI-compatible serve
surface, an ACP/IDE bridge, a web dashboard, and interactive terminal
I/O.

This spec turns each scaffold into a real, conformance-tested
interface: a `/v1/chat/completions` server (streaming + non-streaming)
backed by the kernel turn loop, an ACP bridge implementing the real
protocol's session lifecycle, a token-gated web dashboard, and full
interactive multi-tab PTY on Linux — all consistent with the one-wire
protocol (ADR-0083) and the websocket-ticket security model
(ADR-0084).

## Current state (Confirmed behaviour)

- `crates/optimus-ops/src/surfaces.rs` has `AcpRequest`/`AcpResponse`
  + `acp_handle()` which returns a canned `acp-session-scaffold`
  response and a `proxy_chat_offline()` scaffold; the ACP surface is a
  smoke-test shape, not a protocol bridge (Confirmed: source — the
  response carries a hardcoded session id).
- OpenAI compatibility exists only as a provider CLIENT
  (`OpenAiCompatConfig`/`OpenAiCompatModel` in `crates/optimus-kernel`)
  exercised by `crates/optimus-kernel/tests/openai_http.rs` against a
  mock server; there is no `/v1/chat/completions` SERVER surface
  (Confirmed: source + CLI command enum).
- The existing web surface is `apps/optimus-cli/src/gateway_http.rs`:
  a bounded, token-authorized HTTP server on 127.0.0.1 with rate
  limiting and a public_error facade that never leaks internals
  (Confirmed: source).
- ADR-0084 defines the websocket ticket + process-secret security
  model for surface authorization; ADR-0083 defines the one wire
  protocol for all surfaces; spec-015 is that protocol's living spec
  (Confirmed: ADRs + spec-015).
- PTY: the scorecard records "Terminal PTY: Linux multi-tab session
  store scaffold; full interactive I/O residual" and "Live multi-tab
  ConPTY I/O product UI" as leading product loss #2 (Confirmed:
  scorecard).
- The CLI's `Serve` commands today are: a stdio carrier serve (S7
  leased children) and a 127.0.0.1 webhook server; neither is an
  OpenAI-compatible or ACP server (Confirmed: `apps/optimus-cli/src/main.rs`
  command enum).

## Requirements

### R0. Surface ownership and coexistence

- `optimus serve` is the CORE: it owns the home and serves the
  one-wire protocol (JSON-RPC 2.0) over its existing carriers (stdio
  + loopback WebSocket, per ADR-0083 decision 2); the spec-015 R1
  one-core-per-home record refusal stays (a second plain
  `optimus serve` against a healthily served home refuses) (MUST).
- The OpenAI server, ACP bridge, and web dashboard are ATTACHED
  SURFACES: separate processes (`optimus serve openai` / `optimus
  serve acp` / `optimus serve web`) that connect to a running core
  over the loopback one-wire WS as clients; an attached surface MUST
  refuse to start with `core_unreachable` when no healthy core is
  reachable (MUST).
- Any number of attached surfaces MAY run simultaneously against one
  core; they MUST NOT open the home's SQLite stores directly (the
  core is the single writer; the S7 stdio-carrier serve record is the
  precedent) (MUST).
- Machine clients authenticate with the env-delivered gateway token
  (the `gateway_http.rs` pattern); browser clients receive short-lived
  WS dial tickets minted and validated by the dashboard surface at
  its own WS endpoint via a token-gated HTTP handoff — the core's
  per-launch record-token rule (ADR-0084) is untouched (MUST).
  Surface start refusals MUST use the spec-015 R1 exit-code
  taxonomy: 2 = bind/security/record problem, 3 = refusal
  (`core_unreachable` and port-busy are refusals) (MUST).

### R1. OpenAI-compatible server

- `optimus serve openai` MUST expose `POST /v1/chat/completions`
  backed by the kernel turn loop (session create/resume per request),
  supporting non-streaming JSON and `stream: true` SSE chunks whose
  wire shape matches the OpenAI chat-completions schema (MUST).
- Requests MUST be authorized by the env-delivered gateway token
  (the `gateway_http.rs` pattern, `OPTIMUS_GATEWAY_TOKEN`); requests
  without a valid token MUST be refused with 401 and a public_error
  body (MUST).
- The server MUST bind 127.0.0.1 by default; a non-loopback bind MUST
  require explicit `--host` with a warning (MUST).
- The server MUST accept only the OpenAI schema fields it implements
  and MUST reject unknown top-level fields with 400 — except for a
  documented allowlist of recognized-but-ignored fields (e.g.
  `metadata`, `stop`, `logprobs`) that real OpenAI clients commonly
  send; the allowlist lives in the server's compat notes, and any
  field outside it is a hard 400 (closed input; no silent ignores)
  (MUST).
- Streaming MUST flush each chunk and MUST emit the terminal
  `[DONE]` sentinel; an error mid-stream MUST emit an SSE error event
  and close, never a silent truncation (MUST).
- A conformance suite MUST validate request acceptance, response
  schema, SSE chunk sequence + `[DONE]`, auth failure (401), and
  malformed-body (400) against golden fixtures; the suite MUST run in
  `just verify` with zero skips (MUST).
- `optimus doctor` MUST report the OpenAI surface's bind address and
  auth state without exposing the token (MUST).

### R2. ACP bridge

- `optimus serve acp` MUST implement the Agent Client Protocol
  lifecycle: `initialize` (protocol version + agent info),
  `session/new`, `session/prompt` (non-streaming + streaming
  responses), and `session/close`, over the protocol's JSON-RPC
  framing (MUST).
- The bridge MUST be backed by real kernel sessions (no canned
  responses; `acp_handle`'s scaffold is replaced, not extended)
  (MUST).
- Every ACP method MUST validate its params against the protocol
  schema and return a protocol-shaped error on mismatch (MUST).
- The conformance suite MUST drive the bridge through the full
  lifecycle with the protocol's fixture request/response pairs and
  MUST assert the canned scaffold is gone (a request that would have
  returned `acp-session-scaffold` now returns a real session)
  (MUST).
- Auth: bearer token per R1's pattern (MUST).

### R3. Web dashboard

- `optimus serve web` MUST serve a React dashboard that renders
  exclusively from one-wire JSON-RPC responses (ADR-0083/spec-015)
  received over an ADR-0084 ticket-authenticated WebSocket: the
  dashboard backend attaches to the core over the loopback one-wire
  WS (R0), and the browser dials its own ticket-authenticated WS
  through the dashboard surface's token-gated handoff (MUST).
- The dashboard MUST expose: session list + resume, gateway status
  (spec-017 per-adapter state), cron schedule view,
  skills/packs consoles, and artifact gallery — read-mostly in v1;
  the only v1 mutations are session resume and cron
  pause/resume (MUST).
- The dashboard MUST NOT invent a REST-poll API: every data response
  is a one-wire JSON-RPC message; live updates are WS pushes, never
  polling (MUST).
- The dashboard MUST be served from the built UI bundle (existing
  Tauri/React app assets) with no dev server dependency (MUST).
- Bind + auth: same rules as R1 (127.0.0.1 default, bearer token,
  public_error facade — no internals in errors) (MUST).

### R4. Interactive multi-tab PTY

- The terminal tool MUST gain a real interactive PTY on Linux: a PTY
  is allocated per terminal tab, the agent streams output to it, the
  user's input streams back (cooked or raw per tab policy), and
  resize events propagate (MUST).
- Multi-tab session store MUST persist tab state (cwd, env, history)
  across restarts, matching the existing session-store durability
  pattern; the multi-tab store contract is NEW in v1 — the scorecard's
  "multi-tab session store scaffold" is design-documented, not
  locatable code, so this spec defines the contract A5 asserts on
  (MUST).
- PTY I/O MUST be bounded and rate-limited; runaway output MUST be
  paused with a resume control, never an unbounded buffer (MUST).
- The PTY path MUST have a CPU/CI-friendly fallback: the interactive
  PTY is driven by a scripted input/expect harness in CI, and the
  existing non-interactive command path stays the default for
  scripts (MUST; no regression to the bounded job stream).
- Windows ConPTY is OUT of scope for this spec (R4 is Linux-only in
  v1; the tab/session store contract is transport-agnostic) (MUST).

### R5. Surface coherence

- All server surfaces MUST share the same auth, bind, and error
  facade mechanisms (one implementation, not four copies) (MUST).
- All surfaces MUST emit ordered, durable event rows for
  connect/auth/serve actions into the observability plane (MUST;
  AGENTS.md law 11).
- A surface that fails to start (port busy, bad token) MUST exit with
  a named diagnostic, never a silent partial serve (MUST).

## Acceptance criteria

- [ ] A1. Given the OpenAI server running on 127.0.0.1 with a valid
  token, when a conformance request (non-streaming) and a
  `stream: true` request are sent, then both return schema-valid
  responses, the SSE stream ends with `[DONE]`, and a bad-token
  request returns 401 with a public error (R1).
- [ ] A2. Given a malformed chat-completions body, when it is sent,
  then 400 returns with no internal detail leaked (R1).
- [ ] A3. Given the ACP bridge running, when the fixture lifecycle
  (initialize → session/new → session/prompt → session/close) is
  driven, then every response is protocol-shaped, and a prompt
  returns a real kernel turn output — never `acp-session-scaffold`
  (R2).
- [ ] A4. Given the dashboard served with a ticket, when the UI loads
  session list, gateway status, cron view, and the artifact gallery,
  then each renders from one-wire JSON-RPC responses received over
  the ticket-authenticated WS (no polling API exists); session resume
  and cron pause/resume round-trip over the same channel (R3).
- [ ] A5. Given a scripted input/expect harness, when the interactive
  PTY tab runs a session with input, output, and a resize, then the
  transcript matches the harness expectation and tab state persists
  across a restart (R4).
- [ ] A6. Given the full implementation, when `just verify` runs,
  then the OpenAI conformance suite, the ACP lifecycle suite, and the
  PTY harness pass with zero skips, and the existing bounded-job
  terminal path still passes (R1, R2, R4, R5).

## Out of scope

- Windows ConPTY (R4 is Linux v1; the tab contract is
  transport-agnostic).
- Multi-user / org auth for the dashboard (single-operator bearer
  token in v1).
- Tools-for-humans MCP server side (that is spec-020's client).
- File-edit previews and other IDE-specific ACP features beyond the
  session lifecycle (MAY later).

## Open questions

- Whether the OpenAI server should multiplex existing sessions by
  `session_id` in the request body or mint one per request — default:
  mint per request, `session_id` field respected when present
  (consistent with `--session` in chat).
- Dashboard live-update mechanism: RESOLVED in R3 — ADR-0084
  websocket tickets over the one-wire protocol are the v1 mechanism;
  there is no polling API.
- ACP protocol version pin: the fixture set must name the protocol
  version conformance is tested against.

## Links

- `crates/optimus-ops/src/surfaces.rs` — the ACP scaffold this spec
  replaces.
- `apps/optimus-cli/src/gateway_http.rs` — the auth/bind/error facade
  pattern R1/R3/R5 unify on.
- `crates/optimus-kernel/tests/openai_http.rs` — the client-side
  OpenAI conformance the server side mirrors.
- `docs/decisions/0083-one-wire-protocol-for-all-surfaces.md` and
  `docs/decisions/0084-websocket-ticket-and-process-secret-security-model.md`
  — the wire + security laws.
- `specs/015-surface-protocol/spec.md` — the one-wire protocol spec.
- `specs/017-gateway-breadth/spec.md` — the gateway status the
  dashboard renders.
- `docs/architecture/sota-scorecard.md` — "leading product losses" #2
  (ConPTY) and the material-partial PTY row.
