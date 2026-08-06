---
doc_id: decisions-0083-one-wire-protocol-for-all-surfaces
doc_type: decision
plane: decision
status: current
authority: record
summary: Build the missing local agent server recorded in ADR-0045 — `optimus serve` as the headless agent backend (one core per home), one JSON-RPC 2.0 protocol over stdio and loopback WebSocket carriers sharing one dispatch over the host registry, making CLI, TUI, and desktop clients of one wire contract instead of embedded-runtime surfaces. Records the serve-verb disambiguation, the host.* wire-event naming divergence from Hermes' gateway.*, the exit codes 2/3 (bind-2 as a change), the record v2 + transport field + version-tolerant read_record, the committed schema artifact, the worker-pool dispatch model with control-plane bypass, disconnect-to-job-cancellation, the exit-code capability probe, and the host's first network-server dependencies.
reviewed_on: 2026-08-05
review_by: 2026-11-05
knowledge_type: decision
covers:
  - specs/015-surface-protocol/spec.md
  - crates/optimus-host/src/contract.rs
  - crates/optimus-host/src/router.rs
  - crates/optimus-host/src/chat.rs
  - apps/optimus-desktop/src/host_runtime.rs
  - apps/optimus-desktop/src/main.rs
  - apps/optimus-cli/src/main.rs
validated_by:
  - scripts/gates/check-surface-contract.py
  - scripts/tests/test_surface_contract.py
  - crates/optimus-host/tests/serve_protocol.rs
  - apps/optimus-cli/tests/capability_probe.rs
  - docs/architecture/surface-protocol.schema.json
  - docs/architecture/surface-protocol.registry.json
  - crates/optimus-host/src/spawn_decision.rs
  - apps/optimus-tauri/src/serve_lifecycle.rs
  - apps/optimus-tauri/src/stage_relay.rs
---

# ADR-0083: One wire protocol for all surfaces

- **Status:** Accepted
- **Date:** 2026-08-05

## Context

ADR-0045 recorded the gap verbatim: "There is no local agent server. Hermes
runs two gateways: `tui_gateway/` is the local agent server and `gateway/`
is remote messaging. Optimus built the second (`optimus-ops`) and never
built the first." Today every surface embeds the runtime: the TUI links
`optimus-host` in-process (`apps/optimus-tui/src/lib.rs:3-6`), the packaged
Tauri shell links the host in-process with the renderer reaching it over
`host_invoke` (`apps/optimus-tauri/src/main.rs:64`), and the CLI opens a
`Kernel` directly (`apps/optimus-cli/src/main.rs:700`). The owner milestone
(2026-08-05) directs the Hermes model: one protocol boundary covers CLI,
TUI, and desktop; the protocol artifact captures the whole surface
contract. The milestone's words "Electron" and "tui_gateway" describe the
Hermes reference model, not Optimus mandates — the packaged app is
exclusively Tauri (spec-001) and the new artifact uses the `serve`/host
vocabulary (ADR-0045's naming plane). The verb is deliberately distinct
from the existing subcommand-scoped `cron serve`
(`apps/optimus-cli/src/main.rs:257`) and `gateway serve` (`main.rs:327`).

## Decision

1. **`optimus serve` is the local agent server**: a headless backend
   process owning the SQLite home, sessions, approvals, filesystem scopes,
   and every durable effect; one core per home (a second `optimus serve`
   against a healthily served home refuses to start).
2. **One protocol, two carriers, one dispatch**: JSON-RPC 2.0 over stdio
   (spawned children) and loopback WebSocket (desktop renderer, attached
   clients), both dispatching through the same `handle_ipc`/chat-stream
   pipeline. Dispatch classes: control-plane operations run on the
   connection's own read/event loop; chat turns and effect methods share a
   bounded worker pool (production default 4, queue 64) so a blocking call
   occupies only its worker.
3. **The wire vocabulary is the host registry** (`METHOD_DOMAINS` behind
   `handle_ipc`) minus the superseded blocking chat family (`chat`,
   `chat_offline`, `chat_approval_resolve`), plus the streaming trio
   (`chat_start`, `chat_cancel`, `chat_approval_resolve_start`), plus the
   protocol methods (`hello`, `event`, `host.ready`, `host.error`).
   `project_root_stage_native` is a shell-gated wire method — reachable
   only from shell-kind connections presenting the process secret.
4. **The wire-event naming diverges from Hermes deliberately**: `host.ready`
   / `host.error` (not Hermes' `gateway.*`), preserving the optimus-ops
   naming plane.
5. **Exit codes pinned**: 2 = bind, security-validation, or record-write
   failure (bind-failure exit 2 is a CHANGE — today's HTTP mode exits 1
   on bind failure, `apps/optimus-desktop/src/main.rs:181-183`); 3 =
   refusal (home already served). Serve's refusal diagnostic names the
   holder's transport — "a host is already serving this home in HTTP
   mode" (http holder) / "a host is already serving this home in ws
   mode" (v2/ws holder) — serve-side text distinct from the desktop's
   existing C3 string. The spawner must parse both.
6. **Record v2**: the host-runtime record bumps to version 2 with a
   `transport` field (`"ws"` written by serve, `"http"` by the surviving
   `--host-only` writer); `read_record` becomes known-version-tolerant;
   refusal is against a healthy record of ANY version/transport.
7. **Framing divergence from the HTTP mode**: parse errors reply
   `-32700` with `id:null` and the connection continues; framing
   violations (binary/non-UTF-8/oversized) terminate loudly with close
   `4003` (ADR-0045:140-142 precedent).
8. **The schema is the shape authority**: every method's params/results,
   every event payload, and the trio's request shapes are declared in the
   committed machine-readable `docs/architecture/surface-protocol.schema.json`;
   the gate-generated registry dump is committed with a sanctioned
   regeneration ritual; prose in this spec is documentation-only for
   shapes.
9. **Disconnect-to-cancellation**: on WebSocket disconnect mid-turn, serve
   cancels the connection's in-flight streams; for tracked job ids
   (`term_run`/`campaign_run`) serve calls `request_job_cancellation` on
   the connection loop; an effect that cannot be tracked continues to its
   budget bound.
10. **Exit-code capability probe**: the spawning shell discriminates a
    stale CLI by running `cli_binary serve --help` — exit 0 ⟺ the `serve`
    subcommand exists; any non-zero exit = stale → reinstall diagnostic.
    No stdout/stderr text is ever parsed, and serve's clap definition must
    not disable its help flag.
11. **First network-server dependencies**: `optimus-host` gains tiny_http +
    tungstenite (0.29.0 already in `Cargo.lock` via headless_chrome), with
    the module-size plan attached (spec-015 R10).

## Consequence

- At the spec-landing commit this ADR's `validated_by` binds the
  CURRENT matrix gate; at the Phase-A implementation commit those
  bindings are REPLACED by `check-surface-contract.py` in the same
  commit as the six-plane deletion (spec-015 A6).

- Supersedes ADR-0045's two-transport table only; attach-or-spawn, the
  naming plane, and all other ADR-0045 consequences stay.
- spec-002 R4 is amended with the SUPERSEDED category; spec-001 R8
  (transport auto-detect) is amended; the static surface-contract gate
  (`check-surface-contract.py`) folds in and supersedes
  `check-desktop-ipc-matrix.py` (six-plane sweep at spec-015 Phase A5).
- spec-015 is the owning spec; this ADR records the choices, the spec
  carries the requirements.
