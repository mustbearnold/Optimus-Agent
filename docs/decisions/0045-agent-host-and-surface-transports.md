---
doc_id: decisions-0045-agent-host-and-surface-transports
doc_type: decision
plane: decision
status: current
authority: record
summary: - Date: 2026-07-27 - Program: program P30+ (TUI + core foundation)
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: decision
covers:
  - crates/optimus-host/src/lib.rs
  - crates/optimus-host/src/router.rs
  - apps/optimus-tui/src/main.rs
depends_on:
  - docs/decisions/0028-electron-react-shell-rust-host.md
  - docs/decisions/0029-react-workbench-and-electron-preview-view.md
  - docs/decisions/0038-ui-ipc-architecture.md
  - docs/decisions/0020-work-graph-integrity-and-loopback-security.md
  - docs/decisions/0044-bounded-project-trust-and-capability-broker.md
validated_by:
  - scripts/gates/check-desktop-ipc-matrix.py
  - scripts/gates/check-crate-layers.py
---

# ADR-0045: Agent host and surface transports

- **Status:** Accepted
- **Accepted:** 2026-08-01 — delivered: the surface-command registry lives in `optimus-ops` (`builtin_surface_commands`) and every surface derives from it
- **Date:** 2026-07-27
- **Program:** program P30+ (TUI + core foundation)

## Context

Optimus grew desktop-first. `apps/optimus-desktop/src/ipc/` owns an exact method
registry — 162 methods across 10 domains — and the React workbench is a client of
it over loopback HTTP. That contract works and is gate-guarded by
`check-desktop-ipc-matrix.py`.

Three things are wrong for the intended product shape:

1. **The contract lives in a surface.** A registry every surface must speak is
   owned by one app. A TUI or CLI cannot reach it without depending on the
   desktop binary.
2. **The CLI embeds its own kernel.** `apps/optimus-cli` calls
   `Kernel::open_session` directly, so two processes open the same SQLite home
   with no shared live state and no cross-surface cancellation.
3. **There is no local agent server.** Hermes runs two gateways: `tui_gateway/`
   is the local agent server and `gateway/` is remote messaging. Optimus built
   the second (`optimus-ops`) and never built the first.

The intended topology puts a hub between the surfaces and the core: the TUI owns
the session, and the desktop attaches to it. Bare `optimus` opens that TUI.

## Decision

**Extract the method registry and domain dispatch out of `apps/optimus-desktop`
into `crates/optimus-host`.** Surfaces become clients of the host; the host is
the only thing that opens the kernel or the SQLite home.

**The host is one process with two parts:** a headless agent host that owns the
session, plus a terminal face on top. The face is the host's first client, not
its owner. Closing the terminal does not end the session or disconnect the
desktop.

**Name it `host`, not `gateway`.** `gateway` already means remote messaging in
this tree (`optimus-ops`, program P28). Reusing it would collapse two distinct
planes. `host` matches the existing vocabulary: `--host-only`,
`OPTIMUS_HOST_PORT`, `host_binary` in `install-meta.json`.

**Two transports, chosen by what the client can physically do:**

| Transport | Client | Why |
|---|---|---|
| Newline-delimited JSON-RPC over stdio | TUI | Spawned child. No port, no token, no collision; dies with its parent. |
| Loopback HTTP + minted token | Electron | A JavaScript main process cannot be a stdio child of a Rust binary. |

**Attach or spawn, in that order.** Probe the host port first and attach if a
healthy host answers; spawn only when none does. `apps/optimus-electron/main.cjs`
line 213 always spawns and never probes, which is what produces two cores when a
TUI and desktop run together.

**Phone access is not a transport.** It rides the messaging gateway
(`optimus-ops`) via Telegram, Signal, and WhatsApp adapters. No LAN listener, no
device pairing, no public bind. ADR-0020's loopback restriction stands unchanged.

## Alternatives considered

### Leave the registry in `apps/optimus-desktop` and have the TUI depend on it

A terminal client would pull in Wry, GTK, and WebKitGTK to reach a method table.
It also keeps the layering inverted: the shared contract stays owned by one
surface.

### Give the TUI its own embedded kernel

Fastest to build and exactly the mistake `apps/optimus-cli` already made. It
would put a third writer on one SQLite home and make shared live state
impossible.

### One transport for everyone

Stdio alone cannot serve Electron. Loopback HTTP alone forces the TUI to bind a
port, mint and store a token, and handle collisions — for a child process that
already has a pipe to its parent.

### Call the new crate `optimus-gateway`

Collides with the P28 messaging gateway. Two planes under one word is exactly the
naming-plane collapse `AGENTS.md` forbids.

## Reasons

- The contract is already modular — 10 domain modules behind one router — so this
  is a move, not a rewrite.
- The desktop has proven the contract in production; the TUI inherits a tested
  protocol instead of inventing one.
- One host means one live state, one cancellation path, and one SQLite writer.
- Hermes demonstrates the shape: `ui-tui` spawns `python -m tui_gateway.entry`
  over stdio JSON-RPC while the desktop speaks the same protocol.

## Consequences

- `apps/optimus-cli` stops embedding a kernel and becomes a host client, keeping
  an embedded mode for CI and headless use.
- `check-desktop-ipc-matrix.py` must follow the registry to its new path and keep
  the Electron allowlist ⊆ registry and React union == allowlist rules intact.
- Bare `optimus` opens the TUI; `command: Commands` becomes `Option<Commands>` so
  all 77 subcommands keep working.
- The permission modes from ADR-0044 become reachable from any surface, including
  `/yolo`, because the profile rides the host session rather than one composer.

## Risks

- **A move this size can silently drop a method.** Mitigated by the matrix gate,
  which fails closed on any registry/allowlist divergence.
- **Attach-or-spawn races.** Two surfaces starting at once could both probe, find
  nothing, and both spawn. The host must fail closed on a bound port rather than
  pick a second one.
- **Stdio framing bugs are silent.** A malformed frame can desynchronise the
  stream instead of erroring. Frame length and parse failures must terminate the
  connection loudly.
- **Token pairing for non-Electron HTTP clients.** Electron reads the token from
  the host's stderr. Any future HTTP client that is not the spawning parent needs
  a different mechanism; none is authorised by this ADR.

## Evaluation evidence

- `check-desktop-ipc-matrix.py` green before and after the move, with the same
  method count.
- `apps/optimus-electron/e2e/compiled-shell.spec.cjs` and
  `compiled-workbench.spec.cjs` green, proving the desktop still works over
  loopback HTTP through the extracted host.
- A stdio round-trip test: spawn the host, issue a method, assert the framed
  response.
- An attach-or-spawn test: start one host, confirm a second client attaches
  instead of spawning a second process.

## Conditions for reconsideration

- A surface appears that can be neither a stdio child nor a loopback HTTP client
  on the same machine, and that cannot be served by the messaging gateway.
- Measurement shows stdio framing is a throughput bottleneck for streaming turns.
- The host needs to serve more than one concurrent session, which would make the
  "TUI owns the session" framing wrong and require a session-multiplexing ADR.

## Relevant code

- `apps/optimus-desktop/src/ipc/router.rs` — the registry being moved
- `apps/optimus-desktop/src/ipc/` — 10 domain modules
- `apps/optimus-electron/main.cjs` — spawn and token pairing to port
- `apps/optimus-cli/src/main.rs` — the embedded-kernel call sites to convert
- `crates/optimus-policy/src/lib.rs` — profiles carried on the host session

## Relevant tests

- `scripts/gates/check-desktop-ipc-matrix.py`
- `scripts/gates/check-crate-layers.py`
- `apps/optimus-electron/e2e/compiled-shell.spec.cjs`
- `apps/optimus-electron/e2e/compiled-workbench.spec.cjs`
- `crates/optimus-runtime/tests/project_trust_profile.rs`
