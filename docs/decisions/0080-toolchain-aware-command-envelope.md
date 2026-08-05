---
doc_id: decisions-0080-toolchain-aware-command-envelope
doc_type: decision
plane: decision
status: current
authority: record
summary: Approved commands under Developer Full Access must actually run: the confined bwrap envelope gains a classed toolchain bind tier (rw non-secret caches, ro functional toolchains, credential/identity paths never bound under shared-net Confined), deterministic exec resolution (first present-and-visible PATH entry + bind-derived Environment=PATH, re-verified per turn at spawn), and a pre-card feasibility probe that denies doomed effects with actionable recovery instead of carding them.
reviewed_on: 2026-08-05
review_by: 2026-11-05
knowledge_type: decision
covers:
  - specs/014-self-build-reliability/spec.md
  - crates/optimus-runtime/src/command_envelope.rs
  - crates/optimus-runtime/src/process_ownership.rs
  - crates/optimus-kernel/src/developer_runtime.rs
validated_by:
  - crates/optimus-runtime/tests/command_envelope.rs
---

# ADR-0080: Toolchain-aware command envelope for Developer Full Access

- **Status:** Accepted
- **Date:** 2026-08-05

## Context

Every `Terminal` effect runs through `systemd-run --user` + bwrap under
`CommandFsEnvelope::Confined` (the product default and the envelope DFA gets
for any non-entire-machine scope). Confined binds only the workspace
read-write; nothing under `$HOME` is visible. On a rustup-based machine the
entire toolchain — `cargo`/`rustc` shims at `~/.cargo/bin`, toolchains at
`~/.rustup`, registry at `~/.cargo/registry` — lives in `$HOME`, so every
approved build dies instantly (`bwrap: execvp … No such file or directory`,
proven live). This is the primary "I approved and it still fails" path for
self-build work.

The fix must not widen the boundary for ordinary profiles: ro-binds are
readable inside the sandbox, and under the shared-network Confined envelope
readable paths are network-exfiltratable — so credential and identity paths
(`~/.cargo/credentials.toml`, `~/.cargo/config.toml`, `~/.gitconfig`,
`~/.config/git`, `~/.config/gh`, `~/.ssh`) must not be bound at all, even
read-only. Only non-secret caches get rw binds (writes go through to the
host; registry-cache poisoning by a malicious `build.rs` is an accepted,
documented risk — workspace rw already accepts semi-trusted code).

Exec resolution needs two layers: the host-side `systemd-run` PATH and the
sandbox child PATH. Bare names resolve deterministically (first PATH entry
present and visible in the bind set, normalized to an absolute path) with a
bind-derived `Environment=PATH`; binds and authority are re-derived per turn
from the live grant snapshot, so the pre-card probe is a feasibility
predictor re-verified at spawn (availability-only failures, no authority
impact).

## Decision

1. Extend the envelope API from all-rw extra roots to classed
   `(PathBuf, BindMode)` binds; populate the toolchain tier only for DFA +
   terminal execution. rw: `~/.cargo/registry`, `~/.cargo/git`, `~/.bun`,
   `~/.cache/cargo`, `~/.cache/bun`. ro: `~/.cargo/bin`, `~/.rustup`,
   `~/.cache/ms-playwright`. Never bound: credential/identity paths above.
   Every entry skip-if-absent; rw sources host-created.
2. Normalize `program` via the first present-and-visible PATH entry;
   set `systemd-run --property=Environment=PATH=<bind-derived>`; re-verify
   at spawn.
3. Probe before the card: program visibility, shim dependencies, and
   bind-mode-aware write targets (HostInstall-class and any effect writing
   into a ro-bound toolchain dir, including shell-wrapped); deny with
   recovery text instead of carding doomed effects.
4. Ordinary profiles keep the strict Confined envelope unchanged; a
   regression test asserts zero `$HOME` binds even when toolchains exist.

## Consequences

- Approved builds/installs/commits become runnable under DFA; `cargo
  publish`/`gh` fail closed with clear errors (their classes ask anyway).
- Sandboxed `just verify` covers the UI-tier/TUI gates only; the desktop-e2e
  leg cannot run in-sandbox (nested `systemd-run --user` needs the user bus,
  which the sandbox does not have) — documented exclusion.
- Registry-cache poisoning via rw binds is an accepted risk, recorded here.

## Alternatives considered

- Bind the whole `$HOME` read-write (simplest): rejected — silently turns the
  sandbox into the host; every host file becomes writable by approved commands.
- Bind `$HOME` read-only: rejected — ro-binds are readable inside the sandbox,
  and under the shared-network Confined envelope readable credential paths are
  exfiltratable (proven: a ro-bound `~/.gitconfig` reads in-sandbox).
- Bind `~/.cargo` wholesale rw: rejected — exposes `~/.cargo/bin` (host-writable
  binaries) and `credentials.toml` (publish tokens) through the bind.
- Keep the strict envelope and rely on Developer Full Access + UnrestrictedHost
  scope (entire machine): rejected — the user would have to widen to
  entire-local-machine scope for ordinary repo builds; the toolchain tier is the
  minimal widening that makes approved work run.
- Resolve bare programs by extending the host PATH only: rejected — the bwrap
  child's PATH comes from the systemd user manager; both layers need the
  bind-derived PATH.

## Evaluation evidence

- Live probes (2026-08-05, CachyOS): strict Confined `cargo --version` →
  `bwrap: execvp … No such file or directory`; with the v3/v4 bind set +
  `Environment=PATH`, bare `cargo` runs through the exact product chain
  (`systemd-run --user --wait --pipe` → bwrap) rc=0, and a fixture `cargo
  build` with a dependency succeeds; chromium-1228 headless runs under a ro
  `~/.cache/ms-playwright` bind rc=0; ro-bound gitconfig is readable in-sandbox
  (write refused); missing bind sources abort the whole invocation
  (`bwrap: Can't find source path`), motivating skip-if-absent.

## Conditions for reconsideration

- If the shared-network assumption changes (Confined gains mandatory net
  unshare), ro-binding identity paths could be revisited — still not required
  for builds.
- If a future sandbox provides per-path policy beyond rw/ro (e.g. MAC), the
  classed bind list becomes the policy input.

## Reasons

The toolchain tier is the smallest change that makes approved commands runnable
under DFA while preserving the boundary for ordinary profiles and keeping
credential paths invisible. Classed binds + skip-if-absent + deterministic
exec resolution were each forced by a live-probed failure mode.

## Risks

- rw registry/git cache binds write through to the host; a malicious `build.rs`
  can poison the host cargo registry (mtime-based freshness will not re-verify).
  Accepted: workspace rw already accepts semi-trusted code; documented in the
  ADR and spec.
- The exec-resolution PATH walk must match the spawn environment or approved
  commands resolve differently than the probe; mitigated by re-verification at
  spawn per turn.

## Relevant code

- `crates/optimus-runtime/src/command_envelope.rs`
- `crates/optimus-runtime/src/process_ownership.rs`
- `crates/optimus-kernel/src/developer_runtime.rs`

## Relevant tests

- `crates/optimus-runtime/tests/command_envelope.rs` (classed binds, argv order,
  skip-if-absent, credential invisibility, strict-envelope zero-`$HOME`)
- `crates/optimus-runtime/tests/command_envelope_live.rs` (bare-cargo chain)
