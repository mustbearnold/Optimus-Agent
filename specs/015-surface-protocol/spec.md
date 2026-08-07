---
doc_id: spec-015-surface-protocol
doc_type: reference
plane: work
status: current
authority: canonical
summary: One wire contract for every Optimus surface — `optimus serve` as the headless agent backend (one core per home), JSON-RPC 2.0 over stdio and loopback WebSocket carriers sharing one dispatch, the host registry as the method vocabulary, and the packaged desktop app as a pure protocol client that spawns or attaches the backend it talks to.
reviewed_on: 2026-08-05
review_by: 2026-11-05
knowledge_type: specification
covers:
  - crates/optimus-host/src/contract.rs
  - crates/optimus-host/src/router.rs
  - crates/optimus-host/src/chat.rs
  - crates/optimus-host/src/runtime_ops.rs
  - crates/optimus-host/src/os.rs
  - crates/optimus-host/src/record.rs
  - apps/optimus-desktop/src/host_runtime.rs
  - apps/optimus-desktop/src/main.rs
  - apps/optimus-desktop/src/server.rs
  - apps/optimus-tauri/src/main.rs
  - apps/optimus-tauri/tauri.conf.json
  - apps/optimus-ui/src/ipc/contracts.ts
  - apps/optimus-ui/src/ipc/wsTransport.ts
  - apps/optimus-ui/src/ipc/contracts.schema.test.ts
  - apps/optimus-tui/src/lib.rs
  - apps/optimus-cli/src/main.rs
  - crates/optimus-host/src/serve.rs
  - crates/optimus-host/src/dispatch.rs
  - crates/optimus-host/src/ws.rs
  - crates/optimus-host/src/handshake.rs
  - crates/optimus-host/src/spawn_decision.rs
  - crates/optimus-host/src/ticket.rs
  - crates/optimus-host/tests/serve_protocol.rs
  - apps/optimus-cli/tests/capability_probe.rs
  - docs/architecture/surface-protocol.schema.json
  - docs/architecture/surface-protocol.registry.json
depends_on:
  - docs/decisions/0020-work-graph-integrity-and-loopback-security.md
  - docs/decisions/0045-agent-host-and-surface-transports.md
  - docs/decisions/0038-ui-ipc-architecture.md
  - docs/decisions/0051-electron-now-tauri-when-the-preview-leaves-the-shell.md
  - specs/001-desktop-shell/spec.md
  - specs/002-host-ipc/spec.md
  - specs/010-surfaces/spec.md
validated_by:
  - scripts/gates/check-surface-contract.py
  - scripts/tests/test_surface_contract.py
  - apps/optimus-desktop/e2e/**
---

# Surface protocol — one wire contract for CLI, TUI, and desktop

Status: current
Owner: optimus-agent-development (prompt-only owner)

## Revision table

| Round | Verdict | Findings | Fixes |
|---|---|---|---|
| 1 | REVISE ×2 (A: 4 MAJOR/11 MINOR; B: 5 MAJOR/7 MINOR) | [R1-A1]–[R1-A14], [R1-B1]–[R1-B12] | Fixed in v2; verified REAL round 2. |
| 2 | REVISE ×2 (A: 1 MAJOR/8 MINOR; B: 4 MAJOR/6 MINOR) | [R2-A1]–[R2-A9], [R2-B1]–[R2-B10] | Fixed in v3; verified REAL round 3. |
| 3 | APPROVE (A) + REVISE (B: 1 MAJOR/5 MINOR) + REVISE (DA: 5 MAJOR/11 MINOR) | [R3-A1]–[R3-A10], [R3-B1]–[R3-B6], [R3-DA1]–[R3-DA16] | Fixed in v4; verified REAL round 4. |
| 4 | APPROVE (B) + REVISE (A: 1 MAJOR/6 MINOR) + REVISE (DA: 6 MAJOR/4 MINOR) | [R4-A1]–[R4-A7], [R4-B1]–[R4-B8], [R4-DA1]–[R4-DA10] | Fixed in v5; verified REAL round 5. |
| 5 | APPROVE (B) + REVISE (A: 1 MAJOR/3 MINOR) + REVISE (DA: 4 MAJOR/6 MINOR) | [R5-A1]–[R5-A4], [R5-B1]–[R5-B8], [R5-DA1]–[R5-DA10] | Fixed in v6; verified REAL round 6. |
| 6 | REVISE ×3 (A: 1 MAJOR/7 MINOR; B: 2 MAJOR/6 MINOR; DA: 7 MAJOR/3 MINOR) | [R6-A1]–[R6-A8], [R6-B1]–[R6-B8], [R6-DA1]–[R6-DA10] | Fixed in v7; verified REAL round 7. |
| 7 | APPROVE (B) + REVISE (A: 2 MAJOR/4 MINOR) + REVISE (DA: 3 MAJOR/4 MINOR) | [R7-A1]–[R7-A6], [R7-B1]–[R7-B6], [R7-DA1]–[R7-DA7] | Fixed in v8: `chat_cancel` taxonomy unified (unknown == already-terminal == `{requested:false}` no-op; `-32602` only for malformed ids) [A1]; staging method pinned as a shell-gated bucket in the gate formula [A2]; shell credential mint/delivery/enforcement mapped (shell mints per-launch secret ≥32, env delivery, serve-side injection into the stage call so os.rs:88-92 passes unchanged, manual serve rejects shell-kind, symmetric rejection case) [DA1]; landing order split (A2 alone, green under the old gate; A3+A5 atomic) [DA2]; stale-CLI discrimination via a positive `--help` capability probe + honest port-occupied diagnostic + 15 s readiness bound + all-exit-code handling [DA3]; disconnect-cancel runs on the connection loop [DA4]; production pool default 4 [DA6]; 12 MINORs folded into the body [A3–A6, B1–B6, DA5, DA7]. |
| 8 | APPROVE (A: 2 MINOR) + APPROVE (B: 2 MINOR) + REVISE (DA: 2 MAJOR/5 MINOR) | [R8-A1]–[R8-A2], [R8-B1]–[R8-B2], [R8-DA1]–[R8-DA7] | Fixed in v9: exit-code capability probe (`cli_binary serve --help`, exit 0 ⟺ capable; clap help flag never disabled; zero text matching) [DA1]; TS-conformance equality reconciled with the protocol-method set (schema wire set == union ∪ protocol set; schema event set == StreamEvent ∪ {host.ready, host.error}) [DA2]; A3-first break-count corrected 2→3 (`uncovered` :164 fires too) [DA3]; atomic commit narrowed to union surgery + gate replacement + six-plane sweep (e2e re-point, launch-check extension, contracts.schema.test.ts, spec-014 touchpoints land as green follow-ups) [DA4]; pre-bind readiness timeout does NOT consume a crash-relaunch attempt, epoch pinned spawn→record [DA5]; attach-first lifecycle — probe only when a spawn is needed [DA6]; R2/R12 staging-bucket phrasing reconciled + injection overrides any client-supplied token [DA7]; exit-2-with-free-port branch defined (generic diagnostic, terminal state) [A1]; shell-kind credential validation pinned on BOTH carriers (pipe ownership is not a shell credential) [A2]; A5 sweep scoped (ADR body prose stays historical record; project-scope comment is not a pin) [B1]; `test_verify_gate_parity.py` no-op noted [B2]. |
| 9 | APPROVE (A: 5 MINOR) + REVISE (B: 1 MAJOR/2 MINOR) + REVISE (DA: 2 MAJOR/4 MINOR) | [R9-A1]–[R9-A5], [R9-B1]–[R9-B3], [R9-DA1]–[R9-DA6] | Fixed in v10: legacy typed shim folded into the atomic bundle (`httpTransport.ts:108`/`fixtureTransport.ts:683` → string-typed path for `chat_approval_resolve` only; exemption recorded in the gate per A14; Out-of-scope amended) [B1]; landing-order prerequisite chain pinned (A1 anytime; A3 remainder after (2); A4 after A1+A2; e2e re-point after A3 remainder; launch extension after A4; auto-detect = WS only when a broker ticket global exists) [B2/DA3]; stale-doc-id sweep named in A6 (33 pre-existing ids) [B3]; TS-conformance equality extended to three terms (union ∪ protocol set ∪ shell-gated set {`project_root_stage_native`}) [DA1]; ADR timing reconciled — 0083/0084 written at the spec-landing commit with frontmatter scoped to existing files [DA2]; exit-2/3 branch list made exhaustive over record-state × port-state [A1]; server-origin-only methods rejected `-32601` [A2]; rate limit scoped to worker-dispatched requests, control plane exempt [A3]; A2 gains the stdio shell-kind rejection case [A4]; unknown `client_kind` → `-32600` [A5]; capability-probe validity pinned by a built-binary conformance test (`CARGO_BIN_EXE_optimus`) [DA4]; `serve_protocol.rs` validates responses/events against the schema (bidirectional) [DA5]; citation re-point `main.rs:33` → `:34` [DA6]. |
| 10 | APPROVE (A: 7 MINOR) + APPROVE (B: 6 MINOR) + REVISE (DA: 3 MAJOR/3 MINOR) | [R10-A1]–[R10-A7], [R10-B1]–[R10-B6], [R10-DA1]–[R10-DA6] | Fixed in v11: capability-probe validity test moved to `apps/optimus-cli/tests/capability_probe.rs` (Cargo sets `CARGO_BIN_EXE_OPTIMUS` only for tests of the bin's own package; `serve_protocol.rs` pinned at `crates/optimus-host/tests/serve_protocol.rs`) [DA1]; staged ADR-0083/0084 frontmatter trimmed to landing-existing files (bindings extend at the Phase-A impl commit) [DA2]; `router.rs:40` → `:38` [DA3]; R12 bucket sentence rewritten cleanly [A6/B1/DA4]; branch list made exhaustive over record-state × port-state × probe-health (unhealthy probe → check-port-17865 class; stale of ANY version → generic terminal) [A1/B4]; id-less client frames pinned (dropped, no reply) [A2]; rate-limit rejection pinned `-32603` + closed-form exempt set ({hello, chat_cancel}; chat_start NOT exempt) [A3/DA5]; A2 gains unknown-kind, server-origin-only, and id-less cases [A4]; kind-violation on shell-gated methods pinned `-32601` [A5]; shim pinned to exactly {`chat_approval_resolve`} [DA6]; e2e re-point pinned after A1 + A3 remainder [B3]; broker ticket awaited before first transport construction [B6]. |
| 11 | APPROVE (A: 3 MINOR) + APPROVE (B: 3 MINOR) + REVISE (DA: 1 MAJOR/6 MINOR) | [R11-A1]–[R11-A3], [R11-B1]–[R11-B3], [R11-DA1]–[R11-DA7] | Fixed in v12: A7's shell-lifecycle criteria gain a named executor — the attach-or-spawn-or-diagnose decision function lands in `apps/optimus-desktop/src/spawn_decision.rs` with unit tests covering the full branch matrix + budget arithmetic, shell-level surfacing explicitly downgraded to launch-gate + manual per the evidence ceiling [DA1]; id-less drop takes precedence over `-32600`/`-32601` (id-ful-only rejections; pre-hello id-less case test-enumerated) [A1/DA3]; post-bind record-write failure pinned FATAL for serve (no unreachable holder, no false check-port diagnostic) [A2]; A1 pinned BEFORE the (2) atomic bundle (capability_probe.rs needs the serve subcommand) [A3]; ADR/spec-015 bindings REPLACE deleted-gate bindings in the same commit as the six-plane deletion [B1]; split Rust suites invoked by pinned commands (`cargo test -p optimus-host --test serve_protocol` / `-p optimus-cli --test capability_probe`) [B2]; built-artifact wording fixed (`optimus` binary installed as `cli_binary`) [B3]; `index.ts:6` full-path citation [DA2]; A2 gains the post-hello kind-violation case [DA4]; packaged-app confirmed-broker-absence selects NO transport (HTTP/fixture fallback dev-only) [DA5]; Links Tests gains capability_probe.rs [DA6]; R2 lead reworded (two vocabulary carve-outs + behavioural exemption class) [DA7]. |
| 12 | APPROVE (A: 2 MINOR) + APPROVE (B: 4 MINOR) + REVISE (DA: 1 MAJOR/11 MINOR) | [R12-A1]–[R12-A2], [R12-B1]–[R12-B4], [R12-DA1]–[R12-DA12] | Fixed in v13: serve's exit-code/diagnostic pins gain a named executor — `capability_probe.rs`'s scope extended to spawn the built binary against occupied-port (exit 2), healthy holder (exit 3 + named diagnostic), record-write failure (exit 2), fresh home (record v2/ws) [DA1]; record-write failure joins the exit-2 class (R1) and the spawner's exit-2 branch already lands it in the no-record/free-port case [DA3]; stdio shell-kind rejection exit 2 [DA4]; connections.log fires post-hello (proves dial AND handshake) [DA5]; `-32600` covers non-object JSON + missing/wrong `jsonrpc` member [DA2]; R2 lead gains the explicit protocol-method set [DA6]; A6 binding list gains test_surface_contract.py + spawn_decision.rs, binding timing pinned per-file-landing (never before) [A1/B2/DA7]; spawn_decision moved to the host crate (lib target — Phase-B reuse, no duplication) with probe-injection seam [B1/DA8/DA9]; module-size plan gains spawn_decision.rs [A1]; packaged-vs-dev discriminator pinned (`__TAURI_INTERNALS__` presence) [DA10]; bare `main.rs:144-155` citation fixed [DA11]; id-less drop scoped to reply-layer only — credential-layer 4001 closes still apply, all three id-less cases test-enumerated [A2]; pinned-suite note (self-containment, not exclusivity) [B4]; ADR-0083 draft wording aligned to REPLACE [B3]; ADR-0083 validated_by verified to already bind both gates (DA12 stale read — no change). |
| 13 | Owner single-reviewer gate R1 (external, architecture/security lens, 2026-08-06): REJECTED — 2 blocking + 6 non-blocking | [G1-B1]–[G1-B2], [G1-N1]–[G1-N6] | Fixed in v14: stdio shell-kind rejection split — rejection pinned by serve_protocol.rs, exit-2 pin by capability_probe.rs case (v) (spawn `serve --stdio` with secret env absent, shell-kind hello, assert stderr + exit 2) [B1]; serve-side refusal diagnostic named for BOTH holder transports ("… in HTTP mode" / "… in ws mode"), pinned in R1/R8 and recorded in ADR-0083; A5(ii) split into (iia) http-holder + (iib) ws-holder fixtures; A6 gains the any-version/transport wording [B2]; tiny_http citation → spec-writing time + name-based [N1]; stale-id count re-derived at landing (34 at review time) [N2]; stdio-EOF exit pinned 0 (normal teardown) [N3]; install-meta wording → "only binary-path fields" [N4]; 30 s hello deadline on unauthenticated WS connections [N5]; Purpose/Out-of-scope spec-014 wording fixed + (issues #128–130) [N6]. |
| 14 | Owner single-reviewer gate R2 (2026-08-06): APPROVED — both round-13 blockers verified fixed in the v14 text (exit-code executor + both-transport diagnostic), all 6 non-blocking fixes verified, ~90 citations re-checked, whole-spec re-audit clean; 4 polish notes only. Record: `Development/tmp/spec015-review-r2.md`. | [R2-B1]–[R2-B2], [R2-N1]–[R2-N6] | No fixes required; the 4 polish notes were carried into round 3 and are folded into v15. |
| 15 | Owner single-reviewer gate R3 (single agent, same profile SOUL, 2026-08-06): REJECTED — 10 blocking + 9 non-blocking. Record: `Development/tmp/spec015-review-single-soul.md`. | [R3-B1]–[R3-B10], [R3-N1]–[R3-N9] | Fixed in v15: 11 dead `host_runtime.rs` citations re-pointed to `record.rs` (the implementation moved; the desktop file is a re-export shim) + `record.rs`/`contracts.schema.test.ts` added to `covers` + A6 binding list [B1]; 10 citations to the deleted gates re-pointed to `check-surface-contract.py` + the Current-state gate paragraph rewritten to describe the live gate [B2]; tauri `main.rs` +8 re-points (host_invoke :71, spawn_blocking :84, trio :89/:141/:191, registry :97-101, terminal removal :122-125, stuck-Approving :127-140, continuation :141-189, cancelled-wins :167-176, chat_cancel :191-201) [B3]; cli `main.rs` re-points (open_session :711, `cron serve` :855, `gateway serve` :1059) [B4]; `contracts.ts` +9 re-points (StreamEvent :410-418, ChatRequest :420-429, ApprovalResolveRequest :438-448, TimingEvent :396) + the removed index signature noted as done [B5]; `contract.rs` envelope re-point :120-135 [B6]; desktop `server.rs` re-points (`/api/health` :239-245, cancel closure :477-481) [B7]; `verify.sh`/shim re-points (`build react ui` :361, playwright tiers :452/:634, legacy shims :125-126/:308-322; old-gate sites marked historical) [B8]; R10's TS type-level conformance clauses (b)/(c) implemented in `contracts.schema.test.ts` [B9]; spec-002 R3/R4/R5/R6 + spec-001 R8 same-wave amendments applied [B10]; 9 non-blocking folded in: revision-table reorder + owner-R2 row (this table) [N1/N2]; inline "exit 2" in A2 [N3]; A1 ws-mode diagnostic pointer [N4]; R12 hello-deadline case named [N5]; R6 id-less-hello rewording to the implemented drop behavior [N6]; staging-secret row marked spec-writing-time snapshot [N7]; R7 ticket-delivery record-leg note [N8]; ADR-0083 + R8 record the WS-upgrade mechanism deviation [N9/N4]; R11 enforcement point reworded [N5/N9]. |
| 16 | Owner single-reviewer gate R4 (single agent, same profile SOUL, 2026-08-06): REJECTED — 1 blocking + 6 non-blocking. Record: `Development/tmp/spec015-review-round4.md`. | [R4-B1], [R4-N1]–[R4-N6] | Fixed in v16: the A5 sweep-narrative parenthetical re-pointed to `specs/002-host-ipc/spec.md:72,87` (the v15 same-wave amendments displaced the A1 criterion + Tests footer from :56,71 — the fix wave's own single dead citation) [B1]; Links "(planned)" markers flipped to landed for serve_protocol.rs / capability_probe.rs / contracts.schema.test.ts / ADRs 0083-0084 [N1]; router.rs pin extended to :396-425 (the scope::enforce behavioral test starts at :415) [N2]; `httpTransport.ts:37` → :39 [N3]; `index.ts:6` → :7 [N4]; `server.rs:533-539` → :540 [N5]; tauri `main.rs:71` → :72 (the `#[tauri::command]` attribute is at :71, the fn at :72; also :71-87 → :72-87) [N6]. |
| 17 | Owner single-reviewer gate R5 (single agent, same profile SOUL, 2026-08-06): REJECTED — 2 blocking + 2 non-blocking. Record: `Development/tmp/spec015-review-round5.md`. | [R5-B1]–[R5-B2], [R5-N1]–[R5-N2] | Fixed in v17: A1's Bearer-gated-health cite re-pointed `server.rs:205-218,232-238` → `:212-221,239-245` (the v15 wave re-pointed Current-state and R8 but missed the A1 phase) [B1]; ADR-0083 Context's four dead cites re-pointed to live lines (tauri `main.rs:64` → `:72`, cli `main.rs:700` → `:711`, `cron serve` :257 → `:855`, `gateway serve` :327 → `:1059`) [B2]; `optimus-runtime/...` cite gains the `crates/` prefix [N1]; dispatch.rs:378-380's drifted comment corrected (id-ful absent-ticket hellos close in the hello handler; id-less frames drop here — matching the pinned R6 behavior) [N2]. |
| 18 | Owner single-reviewer gate R6 (single agent, same profile SOUL, 2026-08-06): REJECTED — 1 blocking + 2 non-blocking. Record: `Development/tmp/spec015-review-round6.md`. | [R6-B1], [R6-N1]–[R6-N2] | Fixed in v18: the conformance suite's rate-limit test no longer asserts FIFO reply order — it collects all 600 replies and asserts the id SET {0..=599} (the 4-worker pool replies in completion order; R6 orders only per-stream events, so the old `assert_eq!(reply["id"], id)` contradicted the pinned dispatch model and flaked ~1-in-3 under the `verify.sh:300` invocation; the 601st `-32603` + exempt `chat_cancel` assertions unchanged; suite re-run 6× green) [B1]; R7's CSP parenthetical marked spec-writing-time (the Phase-A3 landing extended `tauri.conf.json:15` to include `ws://127.0.0.1:*`) [N1]; the optional host.error positive-firing pool-death seam test accepted as deferred (the code path is verified correct and defensive; the negative direction is pinned by `host_error_never_fires_for_client_errors`) [N2]. |
| 19 | Owner single-reviewer gate R7 (single agent, same profile SOUL, 2026-08-06): **APPROVED** — no blocking issues. Record: `Development/tmp/spec015-review-round7.md`. | — | All R6 findings verified REAL (rate-limit test order-insensitive with load-bearing assertions intact; 3/3 independent stress runs green under the `verify.sh:300` invocation; CSP snapshot accurate; host.error deferral sound — emission unit-tested at serve.rs:430-442, negative pinned). All 192 citations resolve; executable pins green incl. a /tmp mutation probe (phantom event + requiredness drift both fail the TS conformance); security model + cross-spec re-derived clean. 3 non-blocking prose suggestions left as recorded polish (R10(a) events-container wording; R9 term_run/campaign_run tracking-set parenthetical; credential.rs/record.rs locator-vs-range cites) — approval stands on the as-is text. |

## Purpose

Owner milestone (verbatim, 2026-08-05): "The desktop app is not a
separate implementation. The packaged app ships a React chat surface, and
the renderer talks to a headless backend it launches for you — a serve
process serving a tui_gateway JSON-RPC/WebSocket API — reusing the agent
runtime rather than embedding the TUI. That means one protocol boundary
covers CLI, TUI, and desktop. Nail tui_gateway and you've captured the
whole surface contract in one artifact."

Interpretation (recorded for future agents, incl. issue #131): the
milestone describes the Hermes reference model — Hermes' packaged app IS
Electron and its local agent server is named `tui_gateway`. Optimus'
packaged app is exclusively Tauri by the owner's own same-day directive
and spec-001 (Electron is gate-forbidden), and the new artifact uses the
`serve`/host vocabulary (ADR-0045's naming plane; ADR-0083 records the
divergence). Neither the milestone's word "Electron" nor its word
"tui_gateway" is an Optimus artifact-name mandate. The protocol boundary
is the point; the Tauri shell is its first packaged client.

**Milestone definition: the milestone is Phase A** — `optimus serve` + the
one wire contract + the desktop as a pure protocol client, gate-pinned.
Phase B (TUI over stdio, CLI client mode) completes the milestone's stated
end-state ("one protocol boundary covers CLI, TUI, and desktop") and is
tracked by follow-on issues created at landing. Phase A alone honestly
leaves four ways to reach the runtime (WS, shrunken host_invoke, in-process
TUI, embedded CLI); Phase B collapses them to one protocol. Phased
delivery is the owner-accepted precedent: spec-014's R1–R3 landed
(commits `78e358f`/`3443118`), R4–R12 pending (issues #128–130).

The desktop app must stop being a separate implementation of the agent
surface. Today every surface embeds the runtime: the TUI links
`optimus-host` in-process (`apps/optimus-tui/src/lib.rs:3-6`), the packaged
Tauri shell links the host in-process and the renderer reaches it over
`host_invoke` (`apps/optimus-tauri/src/main.rs:72`), and the CLI opens a
`Kernel` directly (`apps/optimus-cli/src/main.rs:711`). ADR-0045 already
recorded the gap: "There is no local agent server. Hermes runs two
gateways: `tui_gateway/` is the local agent server and `gateway/` is
remote messaging. Optimus built the second (`optimus-ops`) and never built
the first." This spec builds the first, modeled on Hermes' `tui_gateway`:
a headless backend process owns the runtime, and every surface is a client
of one JSON-RPC protocol with two carriers — stdio for spawned children
and loopback WebSocket for the desktop renderer — sharing the same
dispatch (Hermes `tui_gateway/ws.py` reuses `server.dispatch` verbatim).
The contract lives in one artifact: framing, method vocabulary, event
vocabulary, payload shapes, versioning, and lifecycle, gate-guarded
and documented once.

## Current state (Confirmed behaviour)

| Surface | How it reaches the runtime today | Evidence |
|---|---|---|
| TUI | In-process: links `optimus-host`, calls `handle_ipc` directly | `apps/optimus-tui/src/lib.rs:3-6`, `apps/optimus-tui/src/main.rs:4-6` |
| Desktop (product) | In-process: Tauri `host_invoke` (`main.rs:72`) → host registry via `spawn_blocking` (`main.rs:84`); chat via `chat_start`/`chat_cancel` commands; approval continuation via `chat_approval_resolve_start` (`main.rs:141-189`) | `apps/optimus-tauri/src/main.rs:89,114-120,191` |
| Desktop (HTTP mode) | Loopback HTTP+SSE server, `OPTIMUS_HTTP_TOKEN`, `--host-only` port 17865; bind failure exits 1 (`main.rs:181-183`), security validation failure exits 2 (`main.rs:173-178`), refusal exits 3 (`main.rs:165`) | `apps/optimus-desktop/src/main.rs:34,68,144`; `apps/optimus-desktop/src/server.rs:110-130,164` |
| CLI | Embeds a `Kernel` (`Kernel::open_session`) | `apps/optimus-cli/src/main.rs:711` |
| Attach-or-spawn record | `host-runtime.json` (version/port/pid/token) written only after bind, health-checked before trust (HTTP `GET /api/health`, Bearer token required) | `crates/optimus-host/src/record.rs:29-33,68-95,97-115` (consts/write/read/probe; the desktop shell re-exports at `apps/optimus-desktop/src/host_runtime.rs:14`); `server.rs:212-221,239-245` |
| Staging secret | `stage_native_project_root` requires an env-delivered secret compared constant-time (`NATIVE_SELECTION_TOKEN_ENV`); at spec-writing time NOTHING in the tree minted or set that env — the constant and the read were the only references (`os.rs:9,88`). The landing mints and delivers it: `ticket.rs:45-51` (`process_secret`, env-only read; manual serve → `None` → shell-kind rejected) and `apps/optimus-tauri/src/serve_lifecycle.rs:207` (spawn env `PROCESS_SECRET_ENV`) | `crates/optimus-host/src/os.rs:88-118`; spec-002 R7 |

The host registry is the frozen method surface: `METHOD_DOMAINS`
(`crates/optimus-host/src/router.rs:27`) behind `handle_ipc`
(`router.rs:210`), with `scope::enforce` on every call, pinned by tests
(`router.rs:396-425`). `Domain::Chat` holds `chat` (`router.rs:150`),
`chat_offline` (`router.rs:151`), and `chat_approval_resolve`
(`router.rs:152`). These three are BLOCKING and non-cancellable:
`chat_turn(home, params, None)` and the resolve path pass `on_event=None`
with a throwaway token (`crates/optimus-host/src/chat.rs:34-51`) — no
events, no cancellation handle; the blocking resolve is the documented
stuck-"Approving…" bug class (`apps/optimus-tauri/src/main.rs:127-140`).
The streaming trio (`chat_start`, `chat_cancel`,
`chat_approval_resolve_start`) exists as Tauri commands precisely to fix
that; it is Tauri-command-only today, calls host functions directly
(bypassing the registry, `main.rs:89-125`), and removes streams from its
registry at terminal (`main.rs:122-125`). Two further registry methods
are long-running and synchronous: `term_run` (`router.rs:146`) executes a
job with no CancellationToken, bounded by `JobBudget::default()`
(`command_timeout_ms: 30_000`, `crates/optimus-graph/src/lib.rs:251-258`),
and `campaign_run` (`router.rs:132`) runs campaigns step-by-step, each
step job bounded by `JobBudget::default()`
(`crates/optimus-runtime/src/campaign.rs:1454`) — an aggregate of N×30 s
with no aggregate cap. The runtime already has job cancellation machinery
(`crates/optimus-runtime/src/lib.rs:61-66,405,595-707`
`request_job_cancellation` + `CancellationToken`, a `SeqCst` atomic flip
observed cooperatively at step boundaries). The host implements
`browser_*` headless-capable (`crates/optimus-host/src/runtime_ops.rs:414-466`:
`with_preview_browser` → `best_effector`), and the renderer calls
`browser_navigate` through the transport today
(`apps/optimus-ui/src/components/workspace/BrowserSurface.tsx:141`;
`contracts.ts:48-50`).

The transport-internal envelope is JSON-RPC-shaped
(`crates/optimus-host/src/contract.rs:120-135`: `{id, method, params}` /
`{id, ok, result?, error?}` — not strict 2.0: no `jsonrpc` member, string
errors). The renderer's typed surface is the `DesktopMethod` union and the
`StreamEvent` union (`apps/optimus-ui/src/ipc/contracts.ts:1,410-418`),
with exactly one terminal event per chat stream
(`done|cancelled|error`, `contracts.ts:416-418`; spec-002 R6). The shell
native surface is the gate's non-wire bucket — `NON_WIRE_CHANNELS`
(`contract.rs:35-49`: `window_*`, `pick_folder`, `open_path`/`open_url`):
the live surface-contract gate (`scripts/gates/check-surface-contract.py`)
owns the full formula (registry − non-wire − SUPERSEDED + streaming trio +
protocol-method set, with `project_root_stage_native` as the shell-gated
bucket, `check-surface-contract.py:7-10`), its renderer-union rules
(CRITICAL − SUPERSEDED ⊆ union; union ⊆ wire set ∪ shell allowlist;
staging methods shell-kind only; `CRITICAL_INVOKE_METHODS` at :53, the
`missing_critical` check at :250, the legacy-transport exemption at :86),
and the registry/schema parses (`parse_rust_registry` :89-97,
`parse_react_desktop_methods` :110-118). The HTTP mode's `/api/` paths are ALL
bearer-gated (`server.rs:212-221`), including `GET /api/health`
(`server.rs:239-245`), and the probe sends the Bearer
(`record.rs:110-115`). The gate is hard-pinned by
`check-architecture-marks.py:103-110` (`required_paths` for the "UI
architecture" mark, `docs/runbooks/architecture-marks.md:68`) and bound
by frontmatter in ADR-0038 (`docs/decisions/0038-ui-ipc-architecture.md:12-13,21-22`)
and ADR-0051 (`0051-electron-now-tauri-when-the-preview-leaves-the-shell.md:22`);
`docs/architecture.md:1757` carries a hand-maintained gate-table row.

## Requirements

- R1. `optimus serve` MUST exist as a top-level CLI command that runs the
  headless agent backend: one core per home (criterion C3 — the
  one-core-per-home rule is live code: `host_runtime.rs:1` comment and
  the refuse-on-healthy rule at `apps/optimus-desktop/src/main.rs:156-166`).
  It MUST own the SQLite home, sessions, approvals, filesystem scopes,
  and every durable effect; a second `optimus serve` against a healthily
  served home MUST refuse to start. Exit codes (serve pins these as a
  CHANGE from the HTTP mode, recorded in ADR-0083): 2 = bind,
  security-validation, or record-write failure — today only the
  security-validation path exits 2
  (`main.rs:173-178`, `HttpSecurity::new`, `server.rs:110-130`); the
  actual TCP bind (`Server::http`, `server.rs:164`) failure currently
  exits 1 (`main.rs:181-183`), so serve's bind-failure exit 2 is a NEW
  pin, not an inherited one; 3 = refusal — home already served
  (`main.rs:165`, existing). Serve's refusal diagnostic NAMES the
  holder's transport: "a host is already serving this home in HTTP
  mode" (http holder) or "a host is already serving this home in ws
  mode" (v2/ws holder) — serve-side text, distinct from the desktop's
  existing "refusing second core: home … probe and attach (C3)"
  (`apps/optimus-desktop/src/main.rs:160-164`); recorded in ADR-0083.
  The spawner MUST parse both codes. The verb
  is deliberately distinct from the existing subcommand-scoped `cron
  serve` (`apps/optimus-cli/src/main.rs:855`) and `gateway serve`
  (`main.rs:1059`); ADR-0083 records the disambiguation. [inferred: the
  record-based single-instance rule is existing code]
- R2. The wire method vocabulary MUST be the host registry (`handle_ipc`
  over `METHOD_DOMAINS`, `router.rs:27,210`) minus two vocabulary
  carve-outs (non-wire channels; the superseded blocking chat family),
  plus a behavioural cancellation-exemption class (the bounded
  synchronous effects — wire-reachable, exempt only from streaming
  cancellation, never subtracted from the vocabulary), plus one
 wire-only addition set (the streaming trio) and the explicit
 protocol-method set (`hello`, `event`, `host.ready`, `host.error` —
 R12's formula carries it as its own bucket) (spec-002 R4 amended in
  Phase A6 to: "every registry method is either wire-reachable, a
  documented non-wire channel, or explicitly superseded"):
  - non-wire channels (shell/main-only, documented, not wire methods):
    `window_*` (`router.rs:30-36`), `pick_folder`, `open_path`/`open_url`.
    `project_root_stage_native` is a shell-gated wire method —
    reachable on the wire ONLY from `client_kind:"shell"` connections
    presenting the staging process secret (R5/R7 — the staging relay),
    yet NOT part of the renderer/tui/cli-callable wire set; the gate's
    formula carries it as its own bucket (R12). "Wire-reachable from
    shell-kind only" and "not in the renderer wire set" are the single
    reconciled reading — R2 and R12 describe the same bucket from the
    vocabulary and the formula perspectives;
  - superseded blocking chat family: `chat`, `chat_offline`, and
    `chat_approval_resolve` MUST NOT be wire-reachable and MUST NOT be
    renderer-callable — they are blocking and non-cancellable
    (`chat.rs:34-51`), which contradicts R9 and re-exposes the
    stuck-approval bug class; they are superseded by the streaming trio
    below. The registry keeps them (in-process users — CLI
    `main.rs:711`, TUI `lib.rs:3-6`, optimus-ops — are unaffected), but
    the gate MUST carry a `SUPERSEDED` bucket for them;
  - bounded synchronous effect methods (wire-reachable, R9 exemption):
    `term_run` (`router.rs:146`) and `campaign_run` (`router.rs:132`)
    remain wire methods but are exempt from streaming cancellation —
    they are budget-bounded by construction (`term_run` ≤
    `JobBudget::default()` 30 s, `crates/optimus-graph/src/lib.rs:251-258`;
    `campaign_run` per-step jobs each bounded by
    `JobBudget::default()`, `crates/optimus-runtime/src/campaign.rs:1454`,
    an aggregate of N×30 s with no aggregate cap — stated honestly) and
    MUST be dispatched so a blocking call occupies only its worker,
    never the connection's read/event loop (R3);
  - wire-only additions: `chat_start`, `chat_cancel`, and
    `chat_approval_resolve_start` (the Tauri-command trio,
    `apps/optimus-tauri/src/main.rs:89,141,191`, promoted to first-class
    wire methods over the existing `chat_turn`/cancellable-resolve
    pipelines with `on_event` wired, `chat.rs:34-51`).
  `browser_navigate/click/reload` (`router.rs:147-149`) ARE wire-reachable
  (the host runs them headless — `runtime_ops.rs:414-466` — and the
  renderer calls them through the transport today, `BrowserSurface.tsx:141`).
  The surface-contract gate MUST pin the exact wire set and fail on any
  deviation.
- R3. The protocol MUST have two carriers and one dispatch: JSON-RPC 2.0
  over stdio (spawned children) and over loopback WebSocket (desktop
  renderer, attached clients). A method, approval flow, or agent event
  MUST behave identically on both carriers; both carriers MUST dispatch
  through the same `handle_ipc`/chat-stream pipeline. Dispatch classes
  (pinned; the starvation conformance case A2/A4 enforces them):
  - control-plane operations — `hello`, `host.ready`, `chat_cancel`,
    stream-registry operations, and the disconnect cleanup path
    (incl. `request_job_cancellation` for tracked job ids, R9) — MUST
    execute on the connection's own read/event loop, never on the worker
    pool (a cancel is a `SeqCst` token flip observed cooperatively at
    step boundaries, `crates/optimus-runtime/src/lib.rs:64,71-73`; the DB write
    is bounded — no deadlock, no pool dependency; `chat_cancel` needs no
    worker — `apps/optimus-tauri/src/main.rs:191-201` precedent). A
    control-plane op MUST complete even while every worker is busy;
  - chat turns (`chat_start` streams) and registry/effect methods
    (`term_run`, `campaign_run`, and all other registry methods) share a
    bounded worker pool whose PRODUCTION default is 4 workers (tunable
    constant in `serve.rs`; changing it requires re-running the
    conformance suite) with a bounded request queue (64 pending;
    queue-full rejects the NEW request with `-32603` "server busy" and
    the connection stays healthy). A blocking call occupies only its
    worker — never the connection loop, never another connection's
    control-plane ops;
  - saturation rule: 4 concurrent long turns must not prevent a 5th
    connection's `hello`/`chat_cancel` from completing (tested, A2/A4).
  The WebSocket substrate MUST be RFC 6455 (masking, fragmentation,
  ping/pong, close handshake) via the tungstenite crate (0.29.0 already
  in `Cargo.lock` via headless_chrome); close codes `4001`/`4002`/`4003`
  are private-use range (4000-4999). `optimus-host` currently has no
  network-server dependencies; gaining tiny_http + tungstenite is a
  deliberate architecture change recorded in ADR-0083, with the
  module-size plan (R10) attached.
- R4. Framing MUST be JSON-RPC 2.0 (the external 2.0 spec: requests carry
  `jsonrpc:"2.0", id, method, params`; responses carry `jsonrpc:"2.0", id,
  result|error{code,message}`; notifications are method-only), one JSON
  object per line, in both directions, on both carriers. The existing
  transport-internal `IpcEnvelope`/`IpcReply` (`contract.rs:120-135`) remain
  the host API; the wire layer adapts them (adds the `jsonrpc` member and
  structured errors) without changing method semantics. Error taxonomy:
  `-32700` parse error (reply with `id:null` + continue per frame);
  `-32600` invalid request (bad `id`/missing `method`/method-before-hello/
  second hello; NON-OBJECT JSON values (arrays, scalars); missing or
  wrong `jsonrpc` member (e.g. `"1.0"`); reply with `id:null`; R6's
  id-less drop rule applies only to id-less notification-shaped
  objects, not to these); `-32601` unknown method (incl. a kind-violation on a shell-gated
 method — the method is not in this connection's allowed set; R5's
 credential-class matrix governs post-hello calls, not only the
 handshake),
  `-32602` invalid params (a MALFORMED `stream_id` on `chat_cancel` —
  non-u64 — and a second `chat_approval_resolve_start` for a binding
  already resolving), and `-32603` internal error (incl. pool-queue-full
  "server busy" and the 17th concurrent stream "stream limit reached") —
  these three reply with the REQUEST id and keep the connection open
  (JSON-RPC 2.0 semantics); `-32000` ticket rejected (close `4001`);
  `-32001` unsupported protocol version (close `4002`); framing
  violations (binary/non-UTF-8/oversized frame) close with `4003` and a
  diagnostic; the 9th concurrent connection closes with `4003` and a
  "too many connections" diagnostic (R7 bounds). `chat_cancel` on an
  UNKNOWN or already-terminal `stream_id` is NOT an error — it is a
  no-op returning `{"requested": false}` (R6; the Tauri registry removes
  streams at terminal, `main.rs:122-125`, so both cases miss the active
  set). ADR-0083 MUST record the parse-error/framing-violation split
  (parse errors continue; framing violations terminate loudly —
  ADR-0045:140-142) as a deliberate divergence from the HTTP mode.
  Stdio MUST reserve stdout for protocol only; all logging goes to
  stderr/files (a stray `println!` corrupts framing — pinned by a
  stdout-purity test). Plain `optimus serve` (no `--stdio`) MUST NOT read
  stdin at all — a GUI-spawned child's stdin is typically /dev/null, and
  an immediate EOF must not be treated as a carrier disconnect (R9's
  EOF-exit rule applies ONLY when the stdio carrier is active).
- R5. Handshake: the FIRST client frame on either carrier MUST be a
  `hello` request: `{"jsonrpc":"2.0","id":1,"method":"hello","params":{
  "protocol_version":1,"client_kind":"renderer|tui|cli|shell","ticket":"…"}}`
  (ticket required on WebSocket, empty/omitted on stdio — pipe ownership
  is the stdio credential). Credential classes (R7): the record token
  authenticates `renderer|tui|cli`; the staging PROCESS SECRET
  authenticates `client_kind:"shell"` — a shell-kind claim presenting the
  record token MUST be rejected (close `4001`), and symmetrically a
  renderer/tui/cli-kind claim presenting the process secret MUST be
  rejected (close `4001`; the class matrix is complete, pinned by
  serve_protocol.rs). The class matrix applies on BOTH carriers: a
  shell-kind hello over stdio MUST present the env process secret
  exactly as over WebSocket — stdio's ticket omission covers only the
  renderer/tui/cli kinds, and pipe ownership is NOT a shell credential;
  a shell-kind hello without the secret is rejected on stdio as on WS
  (stdio: stderr diagnostic + exit 2 — security-validation
  class, R1; the REJECTION is pinned by serve_protocol.rs, the exit-2
  pin by capability_probe.rs (v), A5; serve's secret
  injection, R7, is never applied to a connection that did not present
  the secret). The server MUST reply with a `hello` result
  `{"protocol_version":1,"capabilities":{"streaming":true,"carriers":
  ["stdio","ws"]}}` followed by a `host.ready` notification whose params
  are `{"protocol_version":1}`. A client that does not recognize the
  server's `protocol_version` MUST fail closed (close, surface
  "incompatible backend", no automatic retry loop); a server that does
  not recognize the client's version MUST reject with `-32001` + close
  `4002`. No method other than `hello` is accepted before the handshake
  completes (violation: `-32600`; absent ticket: close `4001`). A
  `hello` whose `client_kind` is outside {renderer|tui|cli|shell} MUST
  be rejected with `-32600` (invalid request, reply with `id:null`, the
  connection stays open) — pinned by a serve_protocol.rs conformance
  case.
- R6. Server→client events MUST be JSON-RPC 2.0 notifications with method
  `"event"` and params `{"stream_id": u64, "event": <StreamEvent>}`, where
  `<StreamEvent>` is the existing vocabulary (`contracts.ts:410-418`:
  `delta|thinking|status|tool|timing|done|cancelled|error`) plus the
  wire-level notifications `host.ready` (params `{"protocol_version":1}`)
  and `host.error` (params `{"code": int, "message": string}`).
  `host.error` fires ONLY for connection-fatal internal errors
  immediately before close — never for recoverable per-request failures
  (those are `-326xx` replies) or stream failures (those are stream
  terminal `error` events); pinned by a serve_protocol.rs conformance
  case. Client requests whose method ∈ {`event`, `host.ready`,
  `host.error`} MUST be rejected with `-32601` (unknown method — these
  are server-origin-only; a client must not be able to inject events
  into streams or spoof readiness), and the accepted-method-table test
  (R12) MUST enumerate the rejection. Client-sent NOTIFICATION frames
  (method-only, id-less) are NOT dispatched and MUST NOT receive a
  reply (JSON-RPC 2.0 forbids replying to notifications) — they are
 dropped, and the id-less drop rule takes PRECEDENCE over the
 `-32600`/`-32601` rejections, which apply to id-ful requests only:
 an id-less frame before `hello`, an id-less unknown-method frame, or
 an id-less `hello` with an unknown `client_kind` is dropped, never
 answered; the accepted-method-table test enumerates all three
 id-less cases (pre-hello, unknown-method, unknown-kind `hello`). The
 drop rule governs REPLY-layer rejections only — and it applies to the
 `hello` frame too: dispatch drops ALL id-less frames before hello
 validation (`crates/optimus-host/src/dispatch.rs:375-382`), so an
 id-less `hello` with an absent or wrong ticket is DROPPED, never
 answered and never closed (a transport-level close is not a JSON-RPC
 response, but the credential check is not reached for a frame without
 an id). The 30 s hello deadline (R7) still bounds the connection, so
 the drop is not an exposure. The event vocabulary AND every payload shape (method
  params/results, event payloads, the trio's request shapes) MUST be
  declared in the committed machine-readable protocol schema
  (`docs/architecture/surface-protocol.schema.json`, R10); `contracts.ts`
  remains the renderer's typed mapping, never the authority. Every chat
  stream MUST emit exactly one terminal event (`done|cancelled|error`);
  events MUST be ordered per stream (`stream_id`) and observable (laws
  10, 11; spec-002 R6). Concurrent streams interleave only between
  streams, never within one. Request/result params: `chat_start` =
  `{"stream_id": u64, "request": ChatRequest}` (shape at
  `contracts.ts:420-429`); `chat_cancel` = `{"stream_id": u64}` → result
  `{"requested": bool}` mirroring the Tauri command
  (`apps/optimus-tauri/src/main.rs:191-201`), where an unknown or
  already-terminal stream is a no-op returning `{"requested": false}`
  (never `-32602` — pinned by a conformance case);
  `chat_approval_resolve_start` = `{"stream_id": u64, "params":
  ApprovalResolveRequest}` (shape at `contracts.ts:438-448`).
- R7. Security: the WebSocket carrier MUST bind loopback only (ADR-0020)
  and MUST reject handshakes whose Origin is not loopback-local or a
  packaged webview origin: allowlist = `{tauri://localhost,
  http://tauri.localhost}` (packaged Tauri v2 webview origins; default
  custom protocol, `tauri.conf.json:2-34` configures none) ∪
  `{http://127.0.0.1:<any>, http://localhost:<any>, http://[::1]:<any>}`
  (dev server, e2e harness — `e2e/support.js:147,157` — and any loopback
  origin, IPv4 and IPv6). Missing-Origin (raw non-browser clients) and
  `Origin: null` (custom-scheme webviews, sandboxed iframes) are ACCEPTED
  with a valid credential (the credential is the authorization; the
  Origin check is defense-in-depth against non-loopback pages, which
  cannot present a loopback Origin — decisions recorded in ADR-0084).
  The wry-era `optimus://localhost` origin
  (`apps/optimus-desktop/src/main.rs:43-45`) is retired and MUST NOT be
  re-admitted (ADR-0084). The packaged webview's CSP MUST extend
  `connect-src` with `ws://127.0.0.1:*` (`tauri.conf.json:15` carried
  `connect-src 'self' ipc: http://ipc.localhost` at spec-writing time;
  the Phase-A3 landing extended :15 to include `ws://127.0.0.1:*`).
  Authentication MUST be
  per-credential-class: (a) the per-launch ticket — CSPRNG, >= 32 chars,
  presented in the `hello` frame (never in a URL, query string, header
  the renderer cannot set, argv, or rendered page) — authenticates
  renderer/tui/cli kinds; (b) the staging process secret authenticates
  the shell kind. The process secret's lifecycle (currently NOTHING in
  the tree mints it — `os.rs:9,88` are the only references; this spec
  creates the lifecycle): MINT — the spawning shell mints it per launch
  (CSPRNG, >= 32 chars per `os.rs:109`); DELIVERY — passed to serve via
  environment at spawn (never argv), held in shell Rust memory, never in
  the record, never rendered; ENFORCEMENT — serve validates a shell-kind
  hello against the env secret (constant-time, `os.rs:105-118` pattern)
  and, for `project_root_stage_native` calls on shell-kind connections,
  injects the secret into the method params server-side so `os.rs:88-92`'s
  existing per-call constant-time check passes unchanged — the
  injection OVERRIDES any client-supplied `native_selection_token`
  param (a shell client cannot substitute its own token; the injected
  secret is authoritative); a manual serve
  (no env secret) MUST reject all shell-kind connections (the staging
  relay is unavailable outside the spawn path). Ticket mint/delivery:
  the SPAWNING shell mints the ticket and passes it to serve via
  environment (never argv — argv is ps-visible); a serve started
  manually (no env ticket) MUST mint its own per-launch ticket. In BOTH
  mint paths serve MUST write the ticket to the user-only
  `host-runtime.json` record (`record.rs:68-80`; 0600 per
  `crates/optimus-kernel/src/credential.rs:36`) — the record token IS the
  accepted WS dial ticket for renderer/tui/cli kinds, and the record is
  the single persistent-storage exception to this rule and the attach
  credential for every surface (desktop shell, TUI fallback — both named
  record readers). Ticket delivery, as landed: the desktop shell lets
  serve mint the dial ticket and reads it from the record (the sanctioned
  attach path, `ticket.rs:31-42` + `record.rs:68-80`); the shell-minted
  env-delivery leg (R7's env path) is implemented and tested in
  `ticket.rs` (`TICKET_ENV` read + mint fallback) but is not the shell's
  exercised path — no exposure, both legs converge on the same record
  token. The renderer MUST receive a dial ticket exactly once,
  in memory only, through the shell broker (a Tauri command that sets a
  broker-owned global the transport reads; the broker MUST re-issue on
  webview reload, which loses in-memory state). In dev mode (no Tauri
  broker) the same global is the injection point for test tickets
  (`addInitScript`), never the URL. `OPTIMUS_HTTP_TOKEN` MUST remain
  renderer-inaccessible (spec-001 R4); `optimus serve` MUST NOT print the
  ticket or the process secret to stderr (divergence from the HTTP-token
  stderr pairing, `apps/optimus-desktop/src/main.rs:169-171` — recorded
  in ADR-0084). Bounds (tunable constants in `serve.rs`): frame-size cap
  1 MiB (`HTTP_MAX_REQUEST_BODY_BYTES` precedent, `server.rs:23`); rate
 limit 600 requests/min per connection (`WindowRateLimiter` precedent,
 `server.rs:26-54`) — the rate limit applies to worker-dispatched
 method requests only; the control-plane exempt set is CLOSED-FORM:
 {`hello`, `chat_cancel`} are the only client-visible exempt methods
 (stream-registry operations and disconnect cleanup are
 server-internal actions with no client method; `chat_start` is NOT
 exempt — it is worker-dispatched and rate-limited), so a client
 exhausting its own budget cannot stall its own `chat_cancel` (R3's
 control-plane guarantee); exceeding the limit rejects the request
 with `-32603` "rate limit exceeded" (request id, connection stays
 open — pinned by a conformance case); WS ping keepalive every 30 s (faster dead-peer
  detection than the SSE path's 300 s `recv_timeout`,
  `server.rs:540`); max 8 concurrent connections (the 9th closes with
  `4003` "too many connections", R4) and 16 concurrent streams (the 17th
  is rejected with `-32603` "stream limit reached", R4), with a 30 s
  hello deadline — a connection that completes no `hello` within 30 s
  of upgrade is closed, so silent connections cannot hold the 8-slot
  bound indefinitely (same-user DoS, closed cheaply) — both pinned by
  conformance cases (A2, R12).
- R8. Lifecycle: `optimus serve` MUST answer HTTP `GET /api/health` on the
  record port with the existing probe shape, Bearer-gated exactly like
  today (`server.rs:212-221` authorizes every `/api/` path;
  `GET /api/health` at `server.rs:239-245` requires the Bearer; the probe
  sends it, `record.rs:110-115`) — the record token is the Bearer, so
  the health endpoint is protected by the same credential as the WS
  handshake — in addition to accepting WebSocket upgrades on the same
  port. Mechanism as landed (recorded in ADR-0083): a raw loopback
  `TcpListener` + hand-rolled HTTP parser performs the upgrade in
  `crates/optimus-host/src/ws.rs` (not tiny_http's `Request::upgrade()`,
  which the draft evaluated at spec-writing time — name-based citation,
  lock line numbers shift; the locked direct deps are
  tiny_http 0.12.0 `Cargo.lock:5602-5603` for the HTTP health endpoint
  and tungstenite 0.29.0 `Cargo.lock:5890-5891` for RFC 6455).
  `optimus serve --stdio` MUST also open the record + WS listener (the
  stdio carrier is ADDITIVE). A post-bind record-write failure MUST be
  treated as FATAL: serve exits 2 (record-write failure joins the
  exit-2 class, R1) — the record is the
  attach contract (the dial ticket lives only there, R7); a running
  serve without a record would be unreachable by design and would
  produce a false "check port 17865" diagnostic in every client (the
  today-HTTP-mode best-effort write, `server.rs:171-173`, is NOT
  inherited by serve). The host-runtime record MUST bump to
  version 2 with `transport:"ws"` (`record.rs:29-33`); the surviving
  `--host-only` writer MUST emit v2 with `transport:"http"` in the same
  wave, so records are uniform; `read_record` MUST become
  known-version-tolerant (accepts v1 and v2; `healthy_serving_port`,
  `record.rs:97-98`, probes any version). Refusal semantics: serve
  MUST refuse (exit 3) when the probe reports a healthy server of ANY
  record version/transport — one core per home; a healthy v1/http or
  v2/http holder yields the named diagnostic "a host is already serving
  this home in HTTP mode", and a healthy v2/ws holder yields "a host
  is already serving this home in ws mode" (R1, ADR-0083 — the
  serve-side refusal text names the holder's transport); the WS attach
  path requires a v2
  `transport:"ws"` record — v1 records fall through to a fresh spawn,
  and a healthy v1 holder prevents that spawn (no dual-core window).
  Desktop-side behavior for a healthy v1/http or v2/http holder: the app
  can neither attach nor spawn — it MUST surface the named diagnostic
  through the single recovery affordance as a terminal state (no relaunch
  loop). TUI-side: the WS-attach fallback encountering a healthy http
  holder MUST surface the same named diagnostic terminal state (Phase B).
  Port policy: 17865 (`DEFAULT_HOST_PORT` precedent,
  `apps/optimus-desktop/src/main.rs:34`); bind failure after a negative
  probe exits with code 2 (spawn-race loser; ADR-0045:137-139 documented
  the race — fail closed, never a second port). Spawn lifecycle (all
  exit codes defined): BEFORE spawning, the shell runs a capability probe
  — `cli_binary serve --help`; exit 0 = the `serve` subcommand exists =
  capable, ANY non-zero exit = stale CLI (clap's unknown-subcommand
  path exits 2 with empty stdout — live-probed against the installed
  binary; top-level `cli_binary --help` exits 0 on BOTH stale and new
  binaries and is NOT a discriminator). The probe MUST NOT parse
  stdout or stderr text — the exit code is the discriminator
  (help-render changes, i18n, or about-lines containing "serve" must
  never affect it) — and serve's clap definition MUST NOT disable its
  help flag (`disable_help_flag`/`disable_help_subcommand` are
  forbidden for the `serve` subcommand; the probe depends on the flag
  existing). Binary discovery: the installed binary is `cli_binary`
  from `install-meta.json`, then `PATH`; install-meta.json carries
  `cli_binary`/`tauri_binary`/`desktop_binary` as its only
  binary-path fields (plus `desktop_entry` and metadata) —
  `scripts/rebuild-install-relaunch.sh:643-662`; no new field,
  `scripts/tests/test_rebuild_install_safety.py:265` pins
  `host_binary`'s absence). A probe failure is the ONLY stale-CLI
  signal: surface "the installed Optimus CLI does not support
  `optimus serve` — reinstall" immediately (deterministic; exit-code
  based, no text matching, no timing heuristics). After spawn: the app MUST wait for
  readiness (record written only after bind, plus `host.ready`) with an
  overall bound of 15 s — epoch pinned: measured from process spawn to
  record-visible-or-diagnostic (a pre-bind hang — locked home, stalled
  FS — is bounded, not infinite). A pre-bind readiness timeout MUST NOT
  consume a crash-relaunch attempt (a slow-but-healthy start is not a
  crash; 3 × 15 s = 45 s < 60 s, so three slow starts must not exhaust
  the 3/60 s budget into the terminal affordance). On spawn
  exit 2 or 3: re-probe (250 ms probes) for 5 s, then attach if a v2/ws
  record appears (race recovery — the winner writes the record only
  after bind, `record.rs:68-80`); if the port is occupied and no
 record exists, surface the honest diagnostic "serve failed to start:
 check port 17865" (NOT "reinstall" — a bind failure is not a stale
 CLI); if after the 5 s re-probe NO record appears AND the port is
 free, surface the generic diagnostic "serve failed to start" (no
 port hint — exit 2 covers bind OR security-validation failure
 (`main.rs:173-178`, `server.rs:110-130`); a security failure leaves
 the port free, and it is deterministic, so the diagnostic is a
 terminal state through the single recovery affordance — no relaunch
 loop). The branch list is exhaustive over record-state × port-state
 × probe-health (probe = TCP connect + 200 + `ok:true`,
 `record.rs:104-115`): a HEALTHY holder of ANY record
 version/transport ends in its named holder diagnostic (v1/http
 holder → "a host is already serving this home in HTTP mode"; v2/ws
 holder → attach); an occupied port whose probe is UNHEALTHY — an
 unrelated occupier of 17865, or a dead holder's port reused —
 resolves to the "check port 17865"-class diagnostic, never the
 false serving-home diagnostic; "check port 17865" also applies when
 no record at all exists; a STALE record of ANY version (v1 or v2,
 ws or http) with a free port (holder died mid-window) resolves to
 the generic terminal diagnostic — the re-probe window is never a
 fresh-spawn point (the stale-fall-through rule governs the pre-spawn
 attach decision only; spawning again inside the window would need a
 relaunch budget and contradict the no-relaunch-loop design). On any other exit (0, 1, 101/panic): the bounded crash-relaunch
  path applies (3 attempts / 60 s), then the terminal affordance. The
  app MUST terminate the spawned backend on quit, and on backend crash
  MUST surface exactly one recovery affordance (a single named shell
  element) with the 3/60 s bound, after which the affordance is terminal
  (manual restart only). A stale record falls through to a fresh spawn
  (existing health-check rule, `record.rs:97-115`). Serve MUST
  append an accepted-connection line (origin or `null`/`missing`, and
  timestamp — never the ticket) to `<home>/logs/connections.log` on every
  accepted WS connection whose hello handshake COMPLETES
  (post-credential-validation — a rejected handshake never logs; the
  A3 launch-gate assertion therefore proves dial AND handshake, not
  just the upgrade; format pinned in
  the schema).
- R9. Cancellation and disconnect: every long-running wire operation (chat
  stream, approval-resolve stream) MUST support cancellation with
  idempotent semantics and exactly one terminal outcome (law 9; spec-002
  R6). EXEMPTION (R2): the bounded synchronous effect methods
  (`term_run`, `campaign_run`) are not stream-cancellable by design —
  they are budget-bounded and occupy only their dispatch worker, never
  the connection's read/event loop (R3). The no-orphan invariant is
  SCOPED to streams: on WebSocket disconnect mid-turn, serve MUST cancel
  the connection's in-flight streams (the HTTP path's
  `stream_delivery_control(false)` → `StreamControl::Cancel` behavior,
  `chat.rs:20-26`; `server.rs:477-481`); a WS send failure (write timeout
  10 s, Hermes `tui_gateway/ws.py` precedent) MUST map to the same
  delivered=false → Cancel path. Disconnect mid-effect: for tracked job
  ids (term_run/campaign_run job handles), serve MUST call
  `request_job_cancellation` (`crates/optimus-runtime/src/lib.rs:405,595-707`;
  `CancellationToken` at :61-66) — the machinery exists, so an orphaned
  effect is a choice, not a construction limit; the cancel path runs on
  the connection loop (control-plane class, R3; the DB write is bounded,
  the token flip is a `SeqCst` store — no deadlock); an effect that
  cannot be tracked continues to its budget bound (recorded in
  ADR-0083). On stdio EOF or a broken-pipe write, serve MUST cancel that
  connection's streams and exit 0 (a clean carrier close is normal
  teardown, not a failure; the Phase-B TUI classifies exit 0 after
  its own close as normal teardown, never a crash-relaunch trigger;
  Hermes `tui_gateway/entry.py` precedent: the readline loop breaks
  on genuine EOF and recovers/exits) — this
  rule applies ONLY when the stdio carrier is active (plain serve never
  reads stdin, R4). When the TUI spawned serve and the desktop is
  attached over WS, the TUI's exit kills the backend — WS clients see
  the dead socket; the wsTransport MUST synthesize a terminal `error`
  event for every in-flight stream on unexpected close (the
  exactly-one-terminal-event invariant holds client-side too), and lost
  streams are NOT auto-resumed (session state persists server-side; A7's
  recovery affordance is the only continuation path — designed, not a
  bug). The stream registry is per-connection
  (`apps/optimus-tauri/src/main.rs:97-101` pattern).
- R10. Versioning and artifact (law 12): the protocol MUST be versioned as
  one artifact — `PROTOCOL_VERSION` in `contract.rs`, exchanged in the
  `hello` handshake; framing (JSON-RPC 2.0 + RFC 6455), method
  vocabulary, event vocabulary, and payload shapes version together. The
  contract artifact is: (1) this spec (prose is DOCUMENTATION-ONLY for
  shapes — the schema is the machine authority; editors change the schema
  first, prose second); (2) the committed machine-readable protocol
  schema `docs/architecture/surface-protocol.schema.json` declaring every
  method's params/results, every event payload, and the trio's request
  shapes (JSON-Schema dialect with `required` arrays encoding
  requiredness); (3) the gate-generated registry dump, committed at
  `docs/architecture/surface-protocol.registry.json` with a sanctioned
  regeneration ritual — `check-surface-contract.py --update-dump` via a
  `just` target, run-then-commit exactly like `just modules-ratchet`
  (`scripts/gates/check-module-size.py:281-285,320`; `justfile:191-193`);
  the gate itself only compares (law 13 respected — generated files are
  never hand-edited); (4) the Rust event-vocabulary const in
  `contract.rs`. `contracts.ts` remains the renderer's typed mapping,
  never the authority. Conformance: `serve_protocol.rs` (pinned at
  `crates/optimus-host/tests/serve_protocol.rs` — an integration test
  of the host, the package owning the wire layer) MUST drive every
  schema-declared payload shape through both carriers AND validate the
  server's responses and events AGAINST the schema (bidirectional — not
  merely sending schema-shaped requests), so Rust-side shape drift
  fails the suite (acceptance test).
  TS-side conformance (`contracts.schema.test.ts`, named in A5): the
  test imports the schema JSON and (a) asserts the schema wire-method
  set equals the method union extracted from `contracts.ts` UNION the
  protocol-method set UNION the shell-gated set, and the schema event
  set equals the `StreamEvent` union UNION {`host.ready`, `host.error`}
  — the schema declares every wire method and event including the
  protocol methods and the shell-gated staging method
  (`project_root_stage_native`, in the host registry at
  `router.rs:38`, wire-reachable from shell-kind connections only,
  R2/R12), all of which `DesktopMethod`/`StreamEvent` legitimately
  exclude, so plain equality would fail by construction; the
  protocol-method set ({`hello`, `event`, `host.ready`, `host.error`})
  and the shell-gated set ({`project_root_stage_native`}) are named
  constants in the test, cross-referenced to R12's own buckets
  (regex-extraction, the `parse_react_desktop_methods` pattern,
  `check-surface-contract.py:110-118`); (b) asserts type-level
  bidirectional assignability between each schema-declared payload and a
  const mirror declared in the test (a `satisfies`-style helper; if the
  type-level half needs a harness, add `tsd` as a devDependency —
  `apps/optimus-ui/package.json` currently has none, typescript 5.8.3 is
  present); (c) the schema's `required` arrays govern optionality — an
  optional TS field not declared optional by the schema fails. Phase A3
  removed the `TimingEvent` index signature (the former `contracts.ts:398`
  `[key: string]: unknown` is gone — `TimingEvent` is a closed object type
  at `contracts.ts:396-409`), which otherwise would have made every
  assignability assertion vacuous. Module-size plan (law 21): `serve.rs`, `ws.rs`,
  `ticket.rs`, the handshake module, and `spawn_decision.rs` each stay
  under 800 production
  lines; the ratchet baseline is updated only for the deltas, never for
  hand-rolled growth.
- R11. The desktop renderer MUST be a pure protocol client for agent
  methods and chat streaming (WebSocket carrier); `host_invoke` MUST
  shrink to the shell-native allowlist (window chrome, folder picker —
  spec-001 R5) plus the staging relay: `project_root_stage_native` is
  called by the SHELL over its own authenticated wire connection
  (`client_kind:"shell"` presenting the process secret, R5/R7; serve
  injects the secret into the call params so `os.rs:88-92` passes
  unchanged; the renderer never holds the staging credential, spec-002
  R7's process secret; the brokered dial ticket also serves reconnects).
  Enforcement as landed: the shrink is enforced by the surface-contract
  gate's union rules + the renderer's move to the WebSocket carrier —
  the Tauri command itself remains a generic dispatcher
  (`apps/optimus-tauri/src/main.rs:72-87`), which is safe because the
  renderer no longer routes agent methods through it (the gate would
  fail any renderer call not on the wire set); no server-side allowlist
  on the command is required for the milestone.
  `browser_*` stay renderer-callable over the wire (R2) — the preview
  browser keeps working. Phase-B clauses: the TUI MUST become a protocol
  client over the stdio carrier (Phase B1), with a WebSocket-attach
  fallback when its spawned serve exits 2 or 3 (spawn-race loser or
  already-served) or when no record appears after a bounded wait (5 s,
  250 ms probes — the race winner writes the record only after bind,
  `record.rs:68-80`); the fallback reads the dial ticket from the
  record (R7 — the TUI is a named record reader), is the TUI's only WS
  use, and surfaces the named http-holder diagnostic as a terminal state
  (R8); the CLI MUST default to client mode (attach-or-spawn; Phase B2),
  keeping an embedded kernel mode for CI and headless use (ADR-0045
  consequence). Phase A alone does NOT deliver "one protocol boundary
  covers CLI, TUI, and desktop" — that is the Phase-B end-state, tracked
  by follow-on issues created at landing.
- R12. Gate split: the static surface-contract gate MUST fail on any wire
  method missing from or extra to the pinned wire set — formula: registry
  (from `parse_rust_registry`, `check-surface-contract.py:89-97`) −
  non-wire channels − SUPERSEDED (the blocking chat family) + streaming
  trio + the explicit protocol-method set (`hello`, `event`,
  `host.ready`, `host.error` — named so the formula never flags them as
  extras), with `project_root_stage_native` pinned as its OWN shell-gated
  bucket (not subtracted with the non-wire channels, not in the
  renderer wire set: the formula's non-wire subtraction EXCLUDES it
  explicitly, and the accepted-method-table test includes it as
  shell-kind-only — "wire-reachable from shell-kind only" (R2) and
  "not in the renderer wire set" are the same bucket read from the
  vocabulary and the formula perspectives; this reconciliation is
  load-bearing for the gate's literal reading) — any event or payload shape not in
  the schema, any `PROTOCOL_VERSION` mismatch (compare source: the
  `contract.rs` const == the schema-declared version, cross-checked by
  the accepted-method-table test), or any drift of the committed
  registry dump. The gate MUST own all buckets (wire set, shell
  allowlist, shell-gated staging, SUPERSEDED, trio, protocol methods,
  HTTP-legacy: string-typed references to `chat_approval_resolve` in
  `httpTransport.ts`/`fixtureTransport.ts` — the dev/test transports —
  are exempt from the renderer-union rule, the exemption recorded per
  A14) — the string-typed invoke methods in the two transports are
  EXACTLY {`chat_approval_resolve`}; a new string-typed invoke of any
  other method in either transport fails the gate (the legacy shim
  cannot grow)
  and the renderer-union rules (CRITICAL − SUPERSEDED ⊆ union; union ⊆
  wire set ∪ shell allowlist; no silent methods; staging methods
  callable only from shell-kind connections — R5's credential classes);
  `serve_protocol.rs` (`crates/optimus-host/tests/serve_protocol.rs`)
  MUST carry an accepted-method-table test
  enumerating the actual dispatch (the static formula cannot see
  `serve.rs`'s table) and the runtime conformance suite (framing,
  parse-error continuation, hello-order, auth/version rejection incl.
  the full credential-class matrix, terminal-outcome exactly-once,
  cancellation, ordering, disconnect cleanup, stdio EOF, stdout purity,
  `host.error` firing, `chat_cancel` no-op semantics, connection/stream
  bounds (9th/17th), the 30 s hello deadline case (R7 — silent
  connections close 4001 and free their slot), starvation + saturation —
  R3) that the gate tier runs.
- R13. User sovereignty over approval posture (owner directive 2026-08-07;
  ADR-0085): the runtime MUST NOT force a security posture on the user, in
  either direction — approval depth, permission strictness, and autonomy
  MUST be user-selectable (per profile and/or per session), and an explicit
  user choice MUST always override any product default; no posture MAY be
  hard-mandated by the runtime for a user who selected another.
  [phase-marked: acceptance deferred — the posture-selection surface, kernel
  grant routing, and tests land in a follow-on phase; the constitution-level
  requirement is live since 2026-08-07 (OPTIMUS_AGENTS.md "User sovereignty",
  ADR-0085)]

## Acceptance criteria

Phase A (milestone: desktop is a pure protocol client):

- [ ] A1. Given `optimus serve` started against a fresh home, when a
  WebSocket client presents a valid ticket in `hello` and sends a non-chat
  registry method (e.g. `startup_context`), then it receives the `hello`
  result + `host.ready` first, a JSON-RPC result for the method, and the
  result matches the in-process `handle_ipc` result for the same call.
- [ ] A2. Given the same server, when a WebSocket client sends no `hello`,
  a method before `hello`, a second `hello`, no ticket, a wrong ticket, a
  shell-kind claim with a record-token credential, a renderer-kind claim
  with the process secret, a shell-kind hello over stdio without the
  process secret, an unsupported protocol version, an unknown
  method, an unknown `client_kind`, a client request for a
  server-origin-only method (`event`/`host.ready`/`host.error`), an
  id-less client frame (dropped without dispatch or reply, R6), a
  renderer/tui/cli-kind connection calling
  `project_root_stage_native` (→ `-32601`, R4's kind-violation rule),
  malformed JSON, an oversized frame, a binary/non-UTF-8 frame,
  a 9th concurrent connection, or a 17th concurrent stream, then each
  case is rejected per R4/R5/R7 (`-32700`/`-32600` replies with `id:null`
  + continue; `-32601`/`-32602`/`-32603` reply with the request id and
  the connection stays open; close `4001`/`4002`/`4003` for
  ticket/version/framing/connection-limit violations) and the server
  remains healthy for subsequent clients. The ticket cases are
  WebSocket-scoped for the renderer/tui/cli kinds (stdio legally omits
  tickets, R5); shell-kind credential validation applies on BOTH
  carriers — the list above includes the stdio shell-kind rejection
  (stderr diagnostic + exit 2, R5). Starvation
  (mechanism pinned, R3): given a `term_run` of a bounded `sleep`
  command verifiably in flight on connection 1 (in-flight probe: a
  `jobs_list` poll shows the running job), when connection 2 sends
  `hello`, then `chat_start` (offline-paced via
  `OPTIMUS_OFFLINE_LATENCY_MS`, `chat.rs:239-244`), then `chat_cancel`,
  then connection 2's `hello` and `chat_cancel` complete within the
  latency oracle bound while the job is in flight, and its stream emits
  exactly one terminal event; teardown waits for the budgeted completion
  (the exemption means no cancellation of `term_run`). Saturation: given
  4 long turns in flight (pool saturated), a 5th connection's
  `hello`/`chat_cancel` still complete (control-plane bypass).
- [x] A3. Given the desktop e2e suite re-pointed at the WebSocket
  transport (test ticket injected via `addInitScript` into the broker
  global, never a URL; workbench served from a loopback origin — the
  allowlist admits it, R7), when Playwright drives the React workbench
  against a spawned `optimus serve`, then all specs pass including a chat
  round-trip that emits exactly one terminal event. (proven 2026-08-07:
  desktop e2e 46/46 against the re-pointed suite — `apps/optimus-desktop/e2e`
  spawns `optimus serve` per worker, serves `apps/optimus-ui/dist` from a
  loopback origin, injects the ticket via `page.addInitScript`; the A3
  headline test `chat round-trip emits exactly one terminal event`
  asserts `terminalCount === 1` over the WS wire; `chat_cancel` one-shot
  and held-stream control-plane bypass are pinned as separate specs.)
  Packaged-shell
  evidence (the repo has no WebKitGTK driver — evidence ceiling,
  `specs/001-desktop-shell/spec.md:68-81`): the Origin allowlist is
  proven by ws.rs unit tests feeding `tauri://localhost`,
  `http://tauri.localhost`, loopback origins, missing-Origin, and
  `Origin: null` (the last two with valid credentials, R7); the CSP
  extension is proven by a static pin on `tauri.conf.json:15`; the
  packaged-shell behavioral proof exercises the SPAWN path (the packaged
  app spawns serve — the milestone's "it launches for you") and observes
  serve's accepted-connection log (`<home>/logs/connections.log`, R8)
  tailed by the extended `check-tauri-launch.py` (named in A5's gate
  inventory) after the packaged launch, asserting an accepted connection
  from ANY allowlisted packaged origin, or `Origin: null` with a valid
  credential (Linux WebKitGTK may present either); a full in-webview
  round trip remains launch-gate + manual verification (the evidence
  ceiling is real and stays).
- [ ] A4. Given an in-flight WS chat stream, when the client cancels it or
  disconnects mid-turn, then the stream emits exactly one terminal event
  (cancelled) and serve cancels the turn (no orphaned execution); given
  an in-flight `term_run`/`campaign_run` with a tracked job id, when the
  connection disconnects, serve calls `request_job_cancellation` on the
  connection loop (R9); on stdio EOF (active stdio carrier), serve
  cancels that connection's streams and exits; given a serve killed
  without disconnect, the wsTransport synthesizes a terminal `error`
  event for in-flight streams (R9).
- [ ] A5. Given an approval-resolve continuation over the wire, when the
  client calls `chat_approval_resolve_start` and then `chat_cancel`, then
  continuation events stream with exactly one terminal event, and the
  cancelled-wins outcome matches the Tauri path
  (`apps/optimus-tauri/src/main.rs:167-176`); a second
  `chat_approval_resolve_start` for the same binding is rejected with
  `-32602`; `chat_cancel` on an unknown/already-terminal stream returns
  `{"requested": false}` (no-op, R6).
- [ ] A6. Given a healthily served home, when a second `optimus serve`
  starts (exit 3, named diagnostic for a healthy holder of ANY
  version/transport — http-mode text for http holders, ws-mode text
  for v2/ws holders, R1/R8) or a
  client attaches via the record (HTTP probe, Bearer token), then the
  attaching client reaches the existing backend; a version-1 record falls
  through to a fresh spawn only when no healthy holder exists; given a
  healthy v1/http or v2/http holder, the desktop surfaces the named
  diagnostic via the single recovery affordance as a terminal state (and
  in Phase B, the TUI fallback does the same).
- [ ] A7. Given a backend killed mid-session, when the desktop detects the
  dead socket, then it surfaces the single recovery affordance and
  relaunches at most 3 times in 60 s (never a hot loop); a stale
  host-runtime record falls through to a fresh spawn; a pre-bind hang is
  bounded (15 s from spawn, R8) to the same affordance WITHOUT
  consuming a crash-relaunch attempt; a capability-probe failure
  (`serve --help` exit-code probe, R8) surfaces the reinstall
  diagnostic; a port-occupied-no-record exit 2 surfaces "check port
  17865"; an exit-2-with-free-port (security-validation failure)
  surfaces the generic "serve failed to start" diagnostic as a
  terminal state (R8). The shell-level SURFACING of the named
  diagnostics is proven by launch-gate + manual verification per the
  spec-001 evidence ceiling (the packaged shell's UI is not
  scriptable); the DECISION logic behind the surfacing is fully
  unit-tested (`spawn_decision.rs`, A4/A5).
- [ ] A8. Given the surface-contract gate, when it runs, then it exits 0
  with the wire surface exactly equal to the pinned wire set (R2), the
  shell-gated staging bucket reconciled (R12), the event vocabulary and
  payload shapes equal to the schema, the accepted-method-table test
  green, the TS-side schema conformance test green (R10's
  union-∪-protocol-set-∪-shell-gated reconciliation), and the committed
  registry-dump snapshot current; a regression test fails when a phantom
  method, a missing method, an undocumented event, or a schema/payload
  drift (Rust or TS side) is introduced.
- [ ] A9. Given the docs cascade, when `just docs-check` and
  `python3 scripts/tools/engineering_memory.py validate` run, then the
  contract documentation matches the gate output and Engineering Memory
  is current (law 20).
- [ ] A10. Given the full gate spine, when `just check` and
  `bash scripts/verify.sh all` run, then all gates pass including the new
  serve conformance tests, and the module-size ratchet holds (law 21).
- [ ] A11. Given a user who selects a security posture (approval depth /
  permission strictness / autonomy) explicitly, when the runtime runs, then
  the selected posture is honoured in every profile and session: a user who
  selects less approval friction is never asked more, and a user who selects
  more is never silently granted less; product defaults never override the
  explicit choice. [phase-marked: acceptance deferred — lands with the
  posture-selection implementation (R13); the constitution-level requirement
  is live since 2026-08-07 (ADR-0085)]

Phase B (one protocol boundary complete — follow-on issues at landing):

- [ ] A11. Given one `optimus serve` process with two clients (renderer
  over WebSocket; TUI over stdio when the TUI spawned serve, or over the
  WS-attach fallback when the desktop won the spawn — R11), when both run
  chat turns, then both observe the same session state (oracle: both
  issue the same session snapshot method and compare), each stream emits
  exactly one terminal event, and a cancellation from one surface is
  terminal for that stream and visible in the other surface's session
  snapshot.
- [ ] A12. Given the CLI in client mode, when a serve is already running,
  then the CLI attaches to it (no second core); when `--embedded` is
  passed, the CLI opens a kernel directly and works headless (CI mode).
- [ ] A13. Given the TUI as a stdio client (offline provider env
  `OPTIMUS_OFFLINE_LATENCY_MS` passed through to the serve child), when
  the tmux gates (`tui_e2e.py`, `tui_feature_matrix.py`,
  `tui_layout_playwright.cjs`) run, then all pass (spec-010 R1 preserved).
- [ ] A14. Given the HTTP-retention decision, if the HTTP mode is retired,
  then no `OPTIMUS_HTTP_TOKEN`/`httpTransport.ts` residue remains per the
  six-plane ritual and gates + docs cascade are green; if retained, the
  HTTP mode MUST either move its chat surface to the streaming trio or be
  explicitly exempted from R2's supersession (recorded in the gate), and
  stays green under the matrix gate.

## Implementation phases

Phase A (the milestone's core — desktop is a protocol client):

- A1. `optimus serve` command in `apps/optimus-cli`; canonicalize the
  existing `--host-only` loopback server (`apps/optimus-desktop/src/main.rs`)
  into the product verb; keep HTTP `GET /api/health` on the record port
  (Bearer-gated exactly as today, `server.rs:212-221,239-245`); record v2
  with `transport:"ws"` written by serve and `transport:"http"` by the
  surviving `--host-only` writer; `read_record` becomes
  known-version-tolerant; refusal on any healthy record (exit 3, named
  diagnostic for http transport at A1; the ws-mode diagnostic joins from
  A2 — R1/R8 mandate both transports); exit codes pinned 2/3 (bind-failure 2
  is a CHANGE recorded in ADR-0083 — today's HTTP mode exits 1);
  port policy 17865 with fail-closed bind-failure exit 2; `--stdio` opens
  the record + WS listener additively and is the ONLY mode that reads
  stdin; ticket + process secret always written to serve's env by the
  spawning shell (secret minted per launch, CSPRNG >= 32); manual-start
  mint fallback for the ticket only
  (`apps/optimus-desktop/src/main.rs:144-155` pattern; no
  process secret → shell-kind rejected); spawn-binary discovery via
  `cli_binary` (install-meta.json), then PATH, with the
  `serve --help` exit-code capability probe (R8);
  `<home>/logs/connections.log` accepted-connection
  lines (R8).
- A2. Wire layer in `crates/optimus-host` (first network-server deps:
  tiny_http + tungstenite, recorded in ADR-0083): `serve.rs` (stdio +
  dispatch wiring + bounded worker pool, production default 4, queue
  bound 64, control-plane-on-loop dispatch classes per R3), `ws.rs` (RFC
  6455 via tungstenite, `hello`/`host.ready` handshake, credential
  classes, `-32600`/`-32000`/`-32001`, Origin allowlist per R7, 30 s
  ping, 1 MiB frame cap, per-connection rate limit, 8/16 bounds with
  rejection semantics, 10 s write timeout, accepted-connection log
  lines), `ticket.rs` (CSPRNG mint, env delivery, manual fallback, record
  write); `PROTOCOL_VERSION` + the event-vocabulary Rust const in
  `contract.rs`; the committed protocol schema
  (`docs/architecture/surface-protocol.schema.json`); the streaming trio
  promoted to wire methods over the existing
  `chat_turn`/cancellable-resolve pipelines with `on_event` wired
  (`chat.rs:34-51`); the blocking chat family excluded from the wire
  surface; `term_run`/`campaign_run` dispatched on workers with
  disconnect → `request_job_cancellation` for tracked job ids (R9);
  shell-kind hello validation + server-side secret injection into the
  staging call (R7).
- A3. Renderer `wsTransport` (`apps/optimus-ui/src/ipc/wsTransport.ts`) as
  a new `OptimusTransport` kind (incl. synthetic terminal `error` on
  unexpected close, R9); transport auto-detect updated (spec-001 R8):
  WS only when a broker ticket global is present; otherwise HTTP (dev)
  / fixture — the HTTP-pointed playwright tier (`verify.sh:452,634`)
  stays green in the window between the atomic commit and the e2e
  re-point; the HTTP/fixture fallback is DEV-ONLY — in the packaged
  app a confirmed broker absence selects NO transport and surfaces the
  terminal affordance (never a silent fixture in the packaged
  renderer); the packaged-vs-dev discriminator is
  `window.__TAURI_INTERNALS__` presence (spec-001 R8's existing
  predicate): broker absence is confirmed only when the Tauri bridge
  is present and the broker answered no ticket; the terminal
  affordance's coverage is launch-gate + manual per the evidence
  ceiling (the A7 downgrade); the renderer bootstrap MUST await the broker ticket (or
  its confirmed absence) BEFORE the first transport construction —
  the transport is created once and cached
  (`apps/optimus-ui/src/ipc/index.ts:7`), so a wrong
  ordering silently picks HTTP/fixture in the packaged app;
  shell broker command sets the broker-owned ticket global (re-issued on
  reload; dev-mode injection point for test tickets); `tauri.conf.json`
  CSP `connect-src` extended with `ws://127.0.0.1:*`; staging relay: the
  shell calls `project_root_stage_native` over its own
  `client_kind:"shell"` wire connection presenting the process secret;
  `DesktopMethod` union surgery: `chat_approval_resolve` removed
  (superseded), the trio added, and the `TimingEvent` index signature
  removed (`contracts.ts:398` — required by R10's TS conformance). In
  the SAME atomic commit (A5's bundle) the two legacy transports that
  still call the removed member — `httpTransport.ts:125-126` and
  `fixtureTransport.ts:308-322`, typed via
  `invoke<T>(method: DesktopMethod, …)` at `httpTransport.ts:39` with
  `tsc -b` as the `build react ui` gate (`verify.sh:361`) — move to a
  NAMED typed legacy shim: a string-typed invoke path for
  `chat_approval_resolve` only, so the atomic commit keeps the full
  spine green; the shim is exempted in the new gate per A14 (R12's
  HTTP-legacy bucket).
  spec-014 coupling: `wsTransport` MUST reproduce `tauriTransport`'s
  behavior contract incl. its terminal-event handling seams
  (`tauriTransport.ts:30-36`), and the unlanded spec-014 B-fixes
  (resume_error, consent KernelConfig sites `chat.rs:87-93` and
  `chat.rs:437-459`) MUST be re-applied to the serve path when they land —
  named touchpoints, not a regression waiver.
- A4. Desktop lifecycle: the attach-or-spawn-or-diagnose DECISION
  function lands in a unit-testable module in the HOST crate,
  `crates/optimus-host/src/spawn_decision.rs` — optimus-host is a lib
  target, so Phase-B TUI/CLI attach-or-spawn decisions reuse the same
  module instead of duplicating it (the desktop app is a bin-only
  crate); the decision function takes probe results as ARGUMENTS and
  probing is injected, which is what makes the full
  record-state × port-state × probe-health × exit-code branch matrix
  (the R8 branch table as executable tests) and the 3/60 s + 15 s
  budget arithmetic unit-testable; the shell's surfacing of the
  outcome stays thin. Lifecycle: attach-first — read the record + health check
  (a healthy backend is ipso facto capable; probing before attach could
  surface a spurious reinstall diagnostic while a healthy record exists)
  → spawn only when attach fails (the R8 capability probe runs ONLY
  when a spawn is needed) → ready wait (15 s from spawn, R8) → quit
  termination → bounded crash relaunch (3/60 s; a pre-bind readiness
  timeout does NOT consume an attempt, R8) with the single recovery
  affordance; healthy-v1/http holder → named
  diagnostic terminal state; spawn exit 2/3 → bounded re-probe (5 s,
  250 ms probes) → attach or "check port 17865" diagnostic; capability-
  probe failure → reinstall diagnostic (R8).
- A5. Gates + tests: `check-surface-contract.py` owns the full formula
  (registry − non-wire channels − SUPERSEDED + trio + protocol-method
  set, with `project_root_stage_native` as the shell-gated bucket per
  R12; event/payload schema pin; committed registry-dump snapshot at
  `docs/architecture/surface-protocol.registry.json` with `--update-dump`
  ritual + `just` target; renderer-union rules: CRITICAL − SUPERSEDED ⊆
  union, union ⊆ wire set ∪ shell allowlist, staging methods shell-kind
  only). The old gate was DELETED (folded in) with the COMPLETE six-plane
  sweep that landed in the (2) atomic commit: the 4 verify.sh sites were
  removed (the new gate + self-test now sit at `verify.sh:230,251` in
  tier_gates and `:481,502` in tier_all), `test_desktop_ipc_matrix.py`
  was deleted (its loader imported the old gate by absolute path), re-point
  `check-architecture-marks.py:103-110`
  (`required_paths` → `check-surface-contract.py`), re-point
  `validated_by`/`covers` frontmatter in spec-002 (incl. its A1
  criterion and "Tests:" footer, `specs/002-host-ipc/spec.md:72,87`) AND
  ADR-0045:22 AND ADR-0038 (`0038-ui-ipc-architecture.md:12-13,21-22`)
  AND ADR-0051 (`0051-…:22`) AND spec-015's own frontmatter (at the
 implementation commit), update the `docs/architecture.md:1757`
 gate-table row, wire the new gate + its self-test
 (`test_surface_contract.py`) at the 4 verify.sh sites, EM refresh.
 Scope notes: `test_verify_gate_parity.py` needs NO edit — it is fully
 generic (verify.sh tier set-comparison, no gate names,
 `test_verify_gate_parity.py:24-48`); parity holds automatically
 because the new gate + its self-test join both verify.sh tiers in
 the same commit (noted so the landing agent invents no work). ADR
 body prose stays as historical record per the Electron-cutover
 precedent (frontmatter re-pointing only — bodies already name
 deleted `apps/optimus-electron/…` files post-cutover, and
 `docs_system.py:680-699` validates frontmatter bindings, not prose);
 the lone remaining old-gate mention, a comment in
 `check-project-scope-assertions.py:148`, is not a path pin and stays.
  `serve_protocol.rs` (pinned at
  `crates/optimus-host/tests/serve_protocol.rs` — accepted-method-table
  test incl. the server-origin-only rejection, R6 — + framing, handshake,
  hello-order, auth incl. the full credential-class matrix,
  terminal-outcome, cancellation, ordering, disconnect cleanup, stdio
  EOF, stdout purity, `host.error` firing, `chat_cancel` no-op
  semantics, connection/stream bounds, starvation + saturation,
  schema-payload conformance; the gate tier invokes the split Rust
  suites by pinned command: `cargo test -p optimus-host --test
  serve_protocol` and `cargo test -p optimus-cli --test
  capability_probe`, wired into verify.sh's Rust tier alongside the
  workspace test tier — the pinned commands exist for gate
  self-containment, not exclusivity: the workspace
  `cargo nextest run --workspace` tier already includes both files and
  the double-run is harmless), the capability-probe validity test at
  `apps/optimus-cli/tests/capability_probe.rs` — an integration test
  of the package that DEFINES the `optimus` bin
  (`apps/optimus-cli/Cargo.toml:9-11`): Cargo sets `CARGO_BIN_EXE_OPTIMUS`
  only for tests of the bin's own package, so a test inside
  optimus-host cannot read it; the test asserts the built `optimus`
  binary (installed as `cli_binary`) `serve --help` exits 0, pinning
  the probe's premise by test, not prose — and the SAME file is the
  named executor for serve's exit-code/diagnostic pins (R1/R8):
  spawning the built binary's `serve` against (i) an occupied port
  → exit 2, (iia) an http-holder home (scripted health server +
  v1/http or v2/http record, the `record.rs:209-238` test pattern,
  mirrored at `capability_probe.rs:94-108`) → exit 3 with the http-mode refusal diagnostic, (iib) a
  ws-holder home (the natural (iv)→(iib) sequence) → exit 3 with
  the ws-mode refusal diagnostic, (iii) a record-write failure
  (pre-created directory at the record path so the atomic rename
  fails after bind) → exit 2, (iv) a fresh home → binds and writes
  record v2/ws within the readiness bound, (v) a stdio shell-kind
  hello without the process secret → stderr diagnostic + exit 2
  (spawn `serve --stdio` with the secret env absent; write a
  shell-kind `hello` frame to stdin); the e2e harness's existing child-exitCode
  tracking (`e2e/support.js:41,57,97-102`) MAY assert the same from
  the spawned app path —, the TS-side schema conformance test
  (`contracts.schema.test.ts`, R10), the `spawn_decision` unit tests (the A4 decision matrix + budget
 arithmetic as executable tests), desktop e2e over WS (dev-origin;
  packaged-shell evidence per A3 with `check-tauri-launch.py` extended
  as the connections.log observer), approval-resolve-over-WS test (A5).
  LANDING ORDER (pinned, green at every commit): (1) A2 landed ALONE — the wire
  layer, schema, and conformance tests touched no gate-visible file
  (the trio was in neither the registry nor the union yet;
  the then-current `check-desktop-ipc-matrix.py` read only
  router.rs + contracts.ts), so
  the old gate stayed green; the cost of the split — the wire surface was
  unpinned on main between (1) and (2) — was accepted and stated; (2) the
  union surgery (A3) + the gate replacement incl. the six-plane sweep
  (A5) landed in ONE atomic commit ("move the renderer surface to the wire"):
  the pair had to land together — A3-first would have broken the old gate
  THREE times (unknown trio `unknown_react`, `missing_critical`,
  and `uncovered` on `chat_approval_resolve` — the old
  `check-desktop-ipc-matrix.py`'s checks, since deleted),
  A5-first would have broken the new gate's
  union rule; together they were one logical change per
  `specs/conventions.md:63`. The bundle is deliberately narrow: the
  e2e re-point to WS, the `check-tauri-launch.py` extension, the
  `contracts.schema.test.ts` TS-conformance test, and the spec-014
  touchpoint re-application break neither gate and land as separate
  green commits immediately after (each preserves the pinned surface).
  Prerequisite chain for the follow-ups (green at every commit): A1
  (the serve verb + record v2 + version-tolerant `read_record`) is
  gate-invisible and lands BEFORE the (2) atomic bundle — the
  bundle's `capability_probe.rs` test asserts the built `optimus`
  binary `serve --help` exits 0, which requires the serve subcommand;
  the A3 remainder (wsTransport,
  transport auto-detect, broker ticket global, CSP, staging relay)
  lands after (2); A4 (desktop lifecycle) lands after A1 + A2 (it
  spawns serve); the e2e re-point lands after A1 + the A3 remainder
  (it drives Playwright against a spawned `optimus serve`, which needs
  the serve verb); the
  `check-tauri-launch.py` extension lands after A4 (it observes the
  packaged spawn path); the TS-conformance test and the spec-014
  touchpoint re-application are green at any point after (2).
- A6. ADRs 0083 (one wire protocol for all surfaces; supersedes ADR-0045's
  two-transport table only — attach-or-spawn, the naming plane, and all
  other consequences stay; records the `serve`-verb disambiguation from
  `cron serve`/`gateway serve`, the `host.*` wire-event naming divergence
  from Hermes' `gateway.*`, the parse-error-continue vs
  framing-violation-close split, the exit codes 2/3 (bind-2 as a change),
  the record v2 + `transport` field + known-version-tolerant
  `read_record`, the committed schema artifact, the worker-pool dispatch
  model with control-plane bypass, the disconnect → job-cancellation
  rule, and the host's first network-server dependencies) and 0084
  (ticket + process-secret security model: loopback-only, env-minted
  per-launch ticket with manual-start fallback, record token == dial
  ticket for renderer/tui/cli kinds, per-launch minted process secret
  with env delivery and server-side injection for the shell kind,
  Origin allowlist incl. IPv6 loopback with the credential as
  authorization, missing-Origin and null-Origin decisions, bearer-gated
  health, renderer-brokered single delivery with reload re-issue, no
  stderr/argv/URL logging, retired wry origin not re-admitted). spec-002
  (renderer-surface semantics incl. R5's host_invoke shrink, R4 amended
  with the SUPERSEDED category, streaming channel, main-only list,
  `validated_by` re-point, A1 + footer re-point) AND spec-001 R8
  (transport auto-detect text) MUST be amended in the same wave; both
  specs stay current. ADR TIMING (issue #131 deliverable: "landed …
  with ADRs 0083-0084, docs + EM cascade, gates green"): ADR-0083 and
  ADR-0084 are written at the SPEC-LANDING commit with frontmatter
  scoped to files existing at landing (ADR-0062 dead-binding precedent
  — `covers`/`validated_by` bindings extend at the Phase-A
  implementation commit); the same-wave clause above then means
  keeping them current as the code lands. Docs cascade in repo order:
  `just docs-refresh <ids>` — the refresh sweeps the PRE-EXISTING
  verification-lock stale ids (re-derive at landing — 34 changed at
  review time, incl. spec-010/014 and ADR-0082 — the sibling
  spec-014 wave) plus
  `spec-015-surface-protocol`, in one lock file, so the landing commit
  is green → `just docs-generate` → project_knowledge regeneration
  → `engineering_memory.py generate` → verify (AGENTS.md steps 8-9).
  Frontmatter bindings (`covers`/`validated_by`) extend to the new files
  (serve.rs, ws.rs, ticket.rs, record.rs, wsTransport.ts,
  check-surface-contract.py,
  crates/optimus-host/tests/serve_protocol.rs,
  apps/optimus-cli/tests/capability_probe.rs, surface-protocol.schema.json,
  surface-protocol.registry.json, contracts.schema.test.ts,
  test_surface_contract.py, crates/optimus-host/src/spawn_decision.rs)
  in the Phase-A implementation commit, not at spec landing —
  REPLACING, not adding to, the deleted-gate bindings
  (`check-desktop-ipc-matrix.py` and `test_desktop_ipc_matrix.py`,
  which were in ADR-0083's landing `validated_by`) in the SAME commit
  as the six-plane deletion, so `docs_system.py` stays green at every
  commit. Binding timing: each binding is added in the commit where
  its file LANDS — files landing after the (2) atomic bundle
  (wsTransport.ts, contracts.schema.test.ts, record.rs, the A3/A4
  remainder) get
  their bindings in their own landing commits, never before (ADR-0062
  dead-binding precedent: `validate_bindings` fails on bindings that
  resolve no files, `scripts/tools/docs_system.py:680-699`). One issue
  per workstream, closed by the landing commit; the Phase-B follow-on
  issues (B1-B3) are created at spec landing. Landing hygiene: re-run
  `git status` before committing (a parallel agent session may be
  editing the tree — e.g. `crates/optimus-runtime/src/toolchain.rs` was
  live-edited during this review); stage explicit paths only, never
  `git add -A`.

Phase B (one protocol boundary complete):

- B1. TUI over stdio: `optimus-tui` spawns `optimus serve --stdio` and
  speaks the protocol instead of in-process `handle_ipc`; on exit 2 or 3,
  or no record after the bounded wait (5 s, 250 ms probes), the TUI
  attaches over WS using the record token (the R11 fallback); a healthy
  http holder → named diagnostic terminal state; the offline provider env
  (`OPTIMUS_OFFLINE_LATENCY_MS`) passes through to the serve child so the
  tmux gates keep working (spec-010).
- B2. CLI client mode by default (attach-or-spawn); `--embedded` keeps the
  direct kernel open for CI/headless.
- B3. Cross-surface acceptance A11 as a gate (tmux TUI + WS renderer
  against one serve, either spawn order); A12/A13 pinned by their own
  tests.
- B4. HTTP test-mode retention decision (open question below); if retired,
  the six-plane ritual from the Electron retirement applies (the ritual
  lives in the Hermes profile skill at
  `~/.hermes/skills/software-development/optimus-agent-development/references/desktop-shell-cutover.md`);
  if retained, its chat surface moves to the trio or is explicitly
  exempted (A14); docs + EM cascade (same order as A6).

## Out of scope

- Electron in any form (spec-001 exclusive-Tauri; `check-product-complete-install.py`
  forbids it). The protocol is shell-agnostic; the Tauri shell is its
  first packaged client. The milestone's words "Electron" and
  "tui_gateway" describe the Hermes reference model, not Optimus mandates
  (Purpose).
- Remote or phone access: no LAN listener, no public bind (ADR-0020
  loopback restriction unchanged).
- The messaging gateway (`optimus-ops`) — untouched; its "gateway" naming
  plane is preserved: the new artifact uses the `serve`/host vocabulary,
  including wire-event names (`host.ready`, `host.error` — ADR-0083
  records the divergence from Hermes' `gateway.*` event names).
- The Wry rollback shell and its dated retirement (spec-012, 2026-10-31).
- The spec-014 B1–C5 workstream (issues #128–130; sidelined by the
  owner): this milestone
  must not regress its landed A1 work, and the serve path inherits the
  unlanded B1-C5 behavior — the named touchpoints in Phase A3 are
  reproduced now and B-fixes re-applied when they land.
- Rewriting the kernel or runtime — `optimus serve` is a process wrapper
  over the existing host, not a new agent core.
- Multi-profile or multi-home serving from one process.
- Cancellable/streaming variants of `term_run`/`campaign_run` (streaming
  job progress) — the bounded-synchronous exemption (R2/R9) with
  disconnect → job-cancellation is the Phase-A contract; streaming job
  execution is future work.
- An aggregate campaign-run budget cap — the N×30 s aggregate is stated
  honestly in R2 and left to future work.
- The HTTP+SSE test mode (`httpTransport.ts`) is untouched by Phase A
  — unchanged except the NAMED typed legacy shim for its blocking
  resolve (`chat_approval_resolve` via a string-typed invoke,
  `httpTransport.ts:125-126`/`fixtureTransport.ts:308-322`), exempted per A14
  and recorded in the gate (R12's HTTP-legacy bucket); its retention is
  the only open question below.

## Open questions

- HTTP test-mode retention: keep `httpTransport.ts` + `OPTIMUS_HTTP_TOKEN`
  as the dev/test-only transport alongside the WebSocket carrier, or retire
  it once A3 lands? Decision criterion: spec-001 R8's transport
  auto-detect and A3's e2e re-pointing — if the WS path covers both, the
  HTTP mode becomes redundant and is retired in Phase B4 with the same
  six-plane ritual the Electron retirement used. If retained, its chat
  surface MUST move to the streaming trio or be explicitly exempted from
  R2's supersession (A14).

## Planned ADRs

- ADR-0083: one wire protocol for all surfaces (supersedes ADR-0045's
  two-transport table; preserves its naming decision and attach-or-spawn;
  disambiguates `optimus serve` from `cron serve`/`gateway serve`; records
  the `host.*` wire-event naming, the parse-error/framing-violation
  split, the exit codes 2/3 (bind-2 as a change), the exit-code
  capability probe (`serve --help` exit 0 ⟺ capable; serve's help
  flag never disabled), the record v2 +
  transport field + version-tolerant `read_record`, the committed schema
  artifact, the worker-pool dispatch model with control-plane bypass,
  disconnect → job-cancellation, and the host's first network-server
  dependencies).
- ADR-0084: WebSocket ticket + process-secret security model
  (loopback-only, env-minted per-launch ticket with manual-start
  fallback, record token == dial ticket for renderer/tui/cli kinds,
  per-launch minted process secret with env delivery and server-side
  injection for the shell kind, renderer-brokered single delivery with
  reload re-issue, Origin allowlist incl. IPv6 loopback with the
  credential as authorization, missing-Origin and null-Origin decisions,
  bearer-gated health, credential-class enforcement on both carriers
  (pipe ownership is not a shell credential), no stderr logging,
  retired wry origin not re-admitted).

## Links

- Model: Hermes `tui_gateway` — `ws.py` (shared dispatch with the stdio
  carrier, newline-delimited JSON-RPC both directions, `gateway.ready`
  after accept, delta coalescing, 10 s write timeout),
  `server.py` (dispatch, parse-error continuation, stdout purity),
  `entry.py` (stdio entry; the readline loop breaks on genuine EOF and
  recovers/exits), `hermes serve` headless launch mode
  (`hermes_cli/main.py`). Hermes file line numbers are cited without
  lines where checkouts differ.
- ADRs: 0045 (superseded in part by 0083), 0020, 0038, 0051, 0060.
- Specs: 001 (desktop shell, amended in Phase A6), 002 (host IPC, amended
  in Phase A6), 010 (surfaces).
- Code: `crates/optimus-host/src/router.rs`, `contract.rs`, `chat.rs`,
  `runtime_ops.rs`, `os.rs`, `record.rs`; `apps/optimus-ui/src/ipc/contracts.ts`;
  `apps/optimus-desktop/src/host_runtime.rs` (re-export shim);
  `scripts/gates/check-surface-contract.py`;
  `scripts/gates/check-architecture-marks.py`.
- Tests: `crates/optimus-host/tests/serve_protocol.rs` (landed),
  `apps/optimus-cli/tests/capability_probe.rs` (landed),
  `apps/optimus-ui/src/ipc/contracts.schema.test.ts` (landed), desktop
  e2e (`apps/optimus-desktop/e2e/**`).
- ADRs: 0083 (landed), 0084 (landed).
- Ontology: optimus-host, optimus-ui, optimus-cli, optimus-tui.
