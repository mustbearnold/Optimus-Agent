---
doc_id: decisions-0084-websocket-ticket-and-process-secret-security-model
doc_type: decision
plane: decision
status: current
authority: record
summary: The WebSocket carrier's security model — loopback-only bind, per-launch CSPRNG dial ticket (>= 32 chars) minted by the spawning shell and written to the user-only host-runtime record (the record token IS the dial ticket for renderer/tui/cli kinds; manual-start mint fallback), a per-launch minted process secret authenticating the shell kind with env delivery and server-side injection into the staging call (overriding any client-supplied token), credential-class enforcement on both carriers, renderer-brokered single ticket delivery with reload re-issue, an Origin allowlist with the credential as authorization, bearer-gated health unchanged, no stderr/argv/URL ticket logging, and the wry-era optimus://localhost origin retired for good.
reviewed_on: 2026-08-05
review_by: 2026-11-05
knowledge_type: decision
covers:
  - specs/015-surface-protocol/spec.md
  - crates/optimus-host/src/os.rs
  - apps/optimus-desktop/src/host_runtime.rs
  - apps/optimus-desktop/src/main.rs
depends_on:
  - docs/decisions/0020-work-graph-integrity-and-loopback-security.md
validated_by:
  - apps/optimus-desktop/e2e/**
---

# ADR-0084: WebSocket ticket + process-secret security model

- **Status:** Accepted
- **Date:** 2026-08-05

## Context

The WebSocket carrier is the desktop renderer's path to the runtime and
must bind loopback only (ADR-0020). The renderer cannot hold the HTTP
bearer token (`OPTIMUS_HTTP_TOKEN` stays renderer-inaccessible, spec-001
R4), so a separate dial credential is required. The staging path
(`project_root_stage_native`) already requires an env-delivered process
secret compared constant-time (`crates/optimus-host/src/os.rs:9,88-118`) —
but today NOTHING in the tree mints or sets that env; the constant and the
read are the only references. The wry-era desktop shell accepted the
`optimus://localhost` origin (`apps/optimus-desktop/src/main.rs:43-45`);
the Tauri v2 webview presents `tauri://localhost` on Linux WebKitGTK and
`http://tauri.localhost` on Windows WebView2.

## Decision

1. **Loopback-only, credential-gated WebSocket**: serve binds the WS
   listener on loopback only; authentication is per-credential-class at
   the `hello` handshake.
2. **Dial ticket (renderer/tui/cli kinds)**: the spawning shell mints a
   per-launch CSPRNG ticket (>= 32 chars) and passes it to serve via
   environment (never argv — argv is ps-visible). Serve writes it to the
   user-only `host-runtime.json` record (0600); the record token IS the
   accepted WS dial ticket, and the record is the attach credential for
   every surface. A serve started manually (no env ticket) mints its own
   per-launch ticket. Serve never prints the ticket or the process secret
   to stderr (divergence from the HTTP-token stderr pairing).
3. **Renderer delivery**: the renderer receives the dial ticket exactly
   once, in memory only, through the shell broker (a Tauri command setting
   a broker-owned global the transport reads); the broker re-issues on
   webview reload (which loses in-memory state). Dev mode uses the same
   global as the test-ticket injection point (`addInitScript`), never the
   URL.
4. **Process secret (shell kind)**: the spawning shell mints a per-launch
   secret (CSPRNG, >= 32 chars) and delivers it via environment; serve
   validates shell-kind hellos against the env secret constant-time and,
   for `project_root_stage_native` on shell-kind connections, injects the
   secret into the method params server-side so the existing per-call
   constant-time check passes unchanged — the injection OVERRIDES any
   client-supplied token. A manual serve (no env secret) rejects all
   shell-kind connections.
5. **Credential-class enforcement on both carriers**: the class matrix is
   complete and pinned — a shell-kind claim presenting the record token is
   rejected, a renderer/tui/cli-kind claim presenting the process secret
   is rejected, and the same rules apply over stdio (pipe ownership is
   NOT a shell credential; stdio's ticket omission covers only
   renderer/tui/cli kinds).
6. **Origin allowlist (defense-in-depth, not authorization)**: accepted
   origins = `{tauri://localhost, http://tauri.localhost}` (packaged Tauri
   v2 webview origins) ∪ `{http://127.0.0.1:<any>, http://localhost:<any>,
   http://[::1]:<any>}` (dev server, e2e harness, any loopback origin,
   IPv4 and IPv6). Missing-Origin (raw non-browser clients) and
   `Origin: null` (custom-scheme webviews, sandboxed iframes) are accepted
   with a valid credential — the credential is the authorization; the
   Origin check only blocks non-loopback pages, which cannot present a
   loopback Origin. The wry-era `optimus://localhost` origin is retired
   and MUST NOT be re-admitted.
7. **CSP**: the packaged webview's CSP extends `connect-src` with
   `ws://127.0.0.1:*`.
8. **Health unchanged**: HTTP `GET /api/health` stays Bearer-gated exactly
   as today; the record token is the Bearer, so the health endpoint is
   protected by the same credential as the WS handshake.

## Consequence

- At the spec-landing commit this ADR's `covers`/`validated_by` bind
  only files existing at landing; bindings extend to `wsTransport.ts`
  and `serve_protocol.rs` at the Phase-A implementation commit.

- spec-001 R4 (renderer-inaccessible `OPTIMUS_HTTP_TOKEN`) is preserved;
  the renderer never holds the staging credential.
- The security-critical conformance cases (credential-class matrix,
  rejection semantics, close codes `4001`/`4002`/`4003`) live in
  `serve_protocol.rs`, pinned by spec-015 R5/R7 and A2.
