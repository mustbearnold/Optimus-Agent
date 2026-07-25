---
knowledge_type: security-map
status: current
owns:
  - crates/optimus-graph/src/lib.rs
  - crates/optimus-runtime/src/lib.rs
  - crates/optimus-kernel/src/fs_sandbox.rs
  - crates/optimus-kernel/src/browser.rs
  - crates/optimus-kernel/src/codex_oauth.rs
  - crates/optimus-kernel/src/credential.rs
  - crates/optimus-kernel/src/agent.rs
  - crates/optimus-kernel/src/project_authority.rs
  - crates/optimus-packs/src/lib.rs
  - apps/optimus-desktop/src/bridge.rs
  - apps/optimus-desktop/src/server.rs
  - apps/optimus-desktop/src/ipc/router.rs
  - apps/optimus-desktop/src/ipc/chat.rs
  - apps/optimus-desktop/src/ipc/runtime_ops.rs
  - apps/optimus-cli/src/gateway_http.rs
watches:
  - crates/optimus-graph/src/**
  - crates/optimus-runtime/src/**
  - crates/optimus-packs/src/**
  - apps/optimus-desktop/src/ipc/**
covers:
  - crates/optimus-graph/src/lib.rs
  - crates/optimus-runtime/src/lib.rs
  - crates/optimus-kernel/src/fs_sandbox.rs
  - crates/optimus-kernel/src/browser.rs
  - crates/optimus-kernel/src/codex_oauth.rs
  - crates/optimus-kernel/src/credential.rs
  - crates/optimus-kernel/src/agent.rs
  - crates/optimus-kernel/src/project_authority.rs
  - crates/optimus-packs/src/lib.rs
  - apps/optimus-desktop/src/bridge.rs
  - apps/optimus-desktop/src/server.rs
  - apps/optimus-desktop/src/ipc/router.rs
  - apps/optimus-desktop/src/ipc/chat.rs
  - apps/optimus-desktop/src/ipc/runtime_ops.rs
  - apps/optimus-cli/src/gateway_http.rs
depends_on:
  - docs/decisions/0003-phase1-policy-budgets.md
  - docs/decisions/0016-canonical-tool-contract.md
  - docs/decisions/0020-work-graph-integrity-and-loopback-security.md
  - docs/decisions/0031-safe-project-work-loop.md
validated_by:
  - crates/optimus-runtime/tests/approvals_surface.rs
  - crates/optimus-runtime/tests/path_confinement.rs
  - crates/optimus-kernel/tests/kernel_turn.rs
  - crates/optimus-packs/tests/packs_budget.rs
  - apps/optimus-desktop/e2e/04-capabilities-and-tools.spec.js
last_verified_commit: b59b90766fd3b001725dd1542a05326a1d4b4894
---

# Security and approval boundaries

## Trust-boundary map

```text
Untrusted model/provider output
  -> strict provider envelope parsing
  -> whole-batch canonical ToolId/schema/advertisement validation
  -> typed ToolInvocation dispatch
  -> sandboxed deterministic effector or durable runtime job
  -> SmartDeny approval for RunCommand

Untrusted WebView/HTTP request
  -> frozen IPC method registry
  -> domain handler
  -> kernel/runtime/store

External URL
  -> HTTP(S)-only + DNS/IP/redirect SSRF checks
  -> bounded body/text/link extraction
```

## Canonical tool boundary

**Confirmed current behaviour:** raw model tool names are untrusted lookup keys.
The kernel resolves them through the exact `ToolId` set advertised for that
completion step, requires non-empty unique call IDs and canonical names,
requires loaded/available descriptors, and validates every sibling's arguments
before any sibling effect runs.

**Confirmed current behaviour:** pack activation cannot authorize a sibling call
in the same model response. OpenAI calls must use the supported `function`
variant; malformed OpenAI/Codex containers and completed SSE forms fail closed.

## Work Graph and approval flow

1. **Confirmed:** a high-risk effect is persisted as a job/node before execution.
2. **Confirmed:** SmartDeny classifies host-mutating effects as high-risk:
   `RunCommand`, `ProjectRunCommand`, `WriteFile`, `ProjectWriteFile`, and
   program P22 file-mutate family (`Mkdir`/`DeletePath`/`RenamePath`/`PatchFile`
   and Project* twins). `AssertFileEquals` remains non-high-risk (read/compare only).
3. **Confirmed:** execution marks the high-risk node `awaiting_approval` and stops.
4. **Confirmed:** desktop `term_run` cannot self-grant; a separate
   `approvals_grant(job_id)` call resolves the awaiting node, persists a grant
   bound to job/node/SHA-256 effect identity, and resumes.
5. **Confirmed:** a runtime skill can supply a grant only when its closed
   permission set includes `Terminal`.
6. **Confirmed:** approval decisions and status transitions are present in the
   job event/state stores.
7. **Confirmed:** approved commands enter a private platform-owned process tree
   before user code runs. Unix uses a new process group before `exec`; Windows
   creates the process suspended, assigns it to a private kill-on-close Job
   Object, then resumes it. Cancellation, timeout, root exit, and guard drop
   terminate descendants and verify the owned tree is empty before settlement.

**Confirmed current behaviour:** action decisions retain actor, creation time,
expiry, denial/revocation reason, revoking actor, and ordered ledger events.
Expired, revoked, changed-effect, and later-node grants do not authorize work.

## Filesystem boundaries

**Confirmed current behaviour:** kernel/desktop reads through `FsRoots`; paths
are canonicalized under configured roots, absolute/traversal/symlink escapes are
denied, secret basenames are denied, binaries are rejected as text, and output
is bounded.

**Confirmed current behaviour:** runtime writes accept only normal components and
reject empty, current-directory, absolute, parent, root, and platform-prefix
paths. `WriteFile` creation/writes and `AssertFileEquals` opens resolve through a
retained `cap-std` workspace directory capability. Root replacement and linked
existing targets or missing descendants below symlinks/Windows junctions cannot
redirect built-in effects. Runtime and `FsRoots` share one case-insensitive
secret-basename predicate.

**Confirmed current behaviour:** project scopes are a separate Rust-owned,
versioned allowlist. A new canonical root requires a short-lived single-use
token staged by a native folder selection; the renderer cannot mint one.
Project file and command effects persist the canonical workspace hash and
fail before execution if approval is replayed against another root.

**Confirmed current behaviour:** skill grants are effect-class scoped. A skill
may grant only when its closed permission set includes `Terminal` for command
effects or `FsWorkspace` for write effects.

**Confirmed current behaviour:** command execution uses the runtime workspace as
`current_dir` and strips loader-injection environment variables (`LD_PRELOAD`,
`LD_LIBRARY_PATH`, `DYLD_*`, and similar) before spawn.

**Confirmed current behaviour (P12):** approved commands run under
`CommandFsEnvelope` (orthogonal to `PolicyMode` / SmartDeny):

- **Default `Confined` (Linux):** bwrap via systemd-run with workspace as the
  **only** host path bound read-write; system trees ro-bind when present; **no**
  full-root `--bind / /`. Outside-workspace writes fail (tested).
- **`ConfinedNoNetwork`:** confined FS + Linux `--unshare-net`. Product setting
  `isolated_profiles` maps here. **Windows fail-closes** this mode until
  AppContainer (or equivalent) exists.
- **`UnrestrictedHost`:** explicit operator break-glass; Linux may full-root
  bind. Distinct from `PolicyMode::Unrestricted` (approval auto-grant only).

**Confirmed current behaviour:** Windows command containment uses Job Objects
(process tree). Under `Confined`, filesystem residual is product-visible and
accepted; under `ConfinedNoNetwork`, spawn refuses rather than claiming a false
sandbox.

**Confirmed current behaviour:** browser HTTP and `web_search` share
`network_policy::assert_public_http_url` for SSRF/private destination refusal.
Literal private IPs and blocked hostnames fail closed. **Known residual:** if
DNS resolution fails open (resolver error), the pre-connect IP check may not
run; provider TLS/OAuth adapters may keep adapter-local checks.

## Browser/network boundary

**Confirmed current behaviour (ADR-0040 SharedBrowserContract):** two trust
domains stay distinct by default — **UserPreview** (Electron sandboxed
`WebContentsView` / fixture) and **AgentEffector** (kernel `BrowserEffector`).
Coordination is a host-owned event bus (`browser_coord.json` / `BrowserCoordBus`)
recording per-domain navigation URLs and **distinct** domain session ids. Product
claims must say **coordinated preview + agent browser**, not “one shared CDP
session.” Shared cookies, storage partitions, and attaching agent CDP to the
preview WebContentsView partition are **forbidden** without a break-glass ADR.

**Confirmed current behaviour:** the agent browser HTTP effector is bounded
HTTP text/link navigation (optional CDP backend when available). It permits only
HTTP(S), rejects local/private and metadata destinations before DNS resolution
and after every redirect, limits redirects/body/history, and does not execute
page JavaScript on the HTTP path.

**Confirmed current behaviour:** `web_search` returns a versioned extract
envelope (`schema_version`, `provenance_url`, `source`, `retrieved_at_unix_ms`)
from public endpoints with a bounded request timeout. Results are evidence, not
instruction. It is network read, not browser automation.

**Confirmed current behaviour:** shared egress hooks cover browser HTTP +
`web_search` destinations (public HTTP(S) only; private/metadata blocked).

**Confirmed current behaviour:** preview annotations enter a gallery; composer
injection requires explicit **Add to prompt** and remains untrusted context
(C-17 / ADR-0029 §9 / ADR-0040).

**Unknown or unresolved behaviour:** domain allowlist, proxy policy, TLS
pinning, privacy classification, and per-project network policy still do not
cover all provider/OAuth adapter calls.

## Desktop and gateway boundary

**Confirmed current behaviour:** native desktop uses a custom origin and Wry IPC.
A frozen method/domain table rejects unknown IPC methods. Worker pools and
queues are bounded.

**Confirmed current behaviour:** desktop HTTP test mode and gateway HTTP bind to
`127.0.0.1` only and require separate 32-character bearer tokens. Desktop HTTP
also requires `--development-http`; unsafe calls require exact loopback `Origin`
and `X-Optimus-CSRF`, and wildcard CORS is disabled. Gateway validates any
supplied browser origin and CSRF header while allowing bearer-authenticated
non-browser adapters without `Origin`.

**Confirmed current behaviour:** both surfaces cap request bodies and requests
per minute. Gateway list/drain aggregation is bounded. Health omits home paths,
and public errors are stable/redacted while internal detail is logged locally.

## Credentials

**Confirmed current behaviour:** Codex credentials are serialized through
`SystemCredentialProtector`. Windows stores a DPAPI-protected versioned envelope
and migrates legacy plaintext once; corruption fails without rewrite. Other
platforms retain a versioned plaintext fallback but apply user-only file
permissions where the platform supports them. Import reads Hermes or Codex CLI
credential files. Status responses omit token values.

**Unknown or unresolved behaviour:** non-Windows encryption-at-rest, backup/key
recovery, credential expiry/revocation automation, comprehensive redaction
audit, and local IPC process authorization remain absent.

## Security ownership gaps

- **Confirmed current behaviour:** agent descriptors and requests carry exact
  filesystem-root, network-host, effect, and canonical-tool sets. Registry and
  invocation validation require host and descriptor subset closure. These are
  declarations and admission gates; runtime SmartDeny remains effect authority.
- **Confirmed:** built-in specialists (`workspace_writer`, `workspace_reader`)
  are registered under `optimus-agent` / `optimus-workflow` with closed
  permission ceilings; host mutation still routes SmartDeny.
- **Unknown/unresolved:** OS sandbox per
  agent exists, and no dedicated policy package spans publishing/desktop control.
- **Unknown/unresolved:** publishing, Git, browser mutation, desktop control, and
  network-write tools are unavailable placeholders rather than implemented
  approval contracts.
- **Planned:** high-risk tools must declare exact side effects and approval scope
  in the canonical descriptor; authorization remains deterministic and outside
  the LLM.
