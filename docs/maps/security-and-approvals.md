---
knowledge_type: security-map
status: current
covers:
  - crates/optimus-graph/src/**
  - crates/optimus-runtime/src/**
  - crates/optimus-kernel/src/fs_sandbox.rs
  - crates/optimus-kernel/src/browser.rs
  - crates/optimus-kernel/src/codex_oauth.rs
  - crates/optimus-kernel/src/credential.rs
  - crates/optimus-kernel/src/agent.rs
  - crates/optimus-packs/src/**
  - apps/optimus-desktop/src/bridge.rs
  - apps/optimus-desktop/src/server.rs
  - apps/optimus-desktop/src/ipc/**
  - apps/optimus-cli/src/gateway_http.rs
depends_on:
  - docs/decisions/0003-phase1-policy-budgets.md
  - docs/decisions/0012-desktop-approval-boundary.md
  - docs/decisions/0016-canonical-tool-contract.md
validated_by:
  - crates/optimus-runtime/tests/**
  - crates/optimus-kernel/tests/**
  - crates/optimus-packs/tests/**
  - apps/optimus-desktop/e2e/**
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
2. **Confirmed:** SmartDeny classifies `RunCommand` as high-risk; `WriteFile` and
   `AssertFileEquals` are not high-risk.
3. **Confirmed:** execution marks the command node `awaiting_approval` and stops.
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

**Known boundary:** approved arbitrary child processes are not governed by the
built-in file-effect directory capability and can use their own filesystem
syscalls.

**Known boundary:** Windows command containment uses the NT native
`NtResumeProcess` entry point because `std::process::Child` does not expose the
primary thread handle. Focused Windows tests cover pre-assignment failure and Job
membership. Unix containment depends on process-group ownership and signals; it
does not provide a separate filesystem, network, namespace, or cgroup sandbox.

## Browser/network boundary

**Confirmed current behaviour:** the implemented browser effector is bounded
HTTP text/link navigation, not CDP. It permits only HTTP(S), rejects local/private
and metadata destinations before DNS resolution and after every redirect,
limits redirects/body/history, and does not execute page JavaScript.

**Confirmed current behaviour:** `web_search` uses a public HTML endpoint with a
bounded request timeout. It is network read, not browser automation.

**Unknown or unresolved behaviour:** no shared egress policy, domain allowlist,
proxy policy, TLS pinning, privacy classification, or per-project network policy
covers all provider/search/browser/OAuth calls.

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
- **Unknown/unresolved:** no built-in specialist definitions or OS sandbox per
  agent exists, and no dedicated policy package spans publishing/desktop control.
- **Unknown/unresolved:** publishing, Git, browser mutation, desktop control, and
  network-write tools are unavailable placeholders rather than implemented
  approval contracts.
- **Planned:** high-risk tools must declare exact side effects and approval scope
  in the canonical descriptor; authorization remains deterministic and outside
  the LLM.
