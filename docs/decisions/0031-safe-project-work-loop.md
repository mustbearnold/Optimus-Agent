---
knowledge_type: decision
status: current
covers:
  - crates/optimus-graph/src/lib.rs
  - crates/optimus-runtime/src/lib.rs
  - crates/optimus-kernel/src/project_authority.rs
  - crates/optimus-kernel/src/execution.rs
  - crates/optimus-kernel/src/lib.rs
  - apps/optimus-desktop/src/ipc/**
  - apps/optimus-electron/main.cjs
  - apps/optimus-ui/src/**
depends_on:
  - docs/decisions/0018-fail-closed-runtime-safety.md
  - docs/decisions/0021-owned-execution-and-causal-delivery.md
  - docs/decisions/0030-codex-measured-shell-and-multi-folder-projects.md
validated_by:
  - crates/optimus-runtime/tests/approvals_surface.rs
  - crates/optimus-kernel/tests/kernel_turn.rs
  - crates/optimus-kernel/src/execution.rs
  - apps/optimus-desktop/src/ipc/runtime_ops.rs
  - apps/optimus-desktop/src/ipc/sessions.rs
  - apps/optimus-desktop/src/ipc/chat.rs
  - apps/optimus-ui/src/components/workbench/Transcript.test.tsx
  - apps/optimus-ui/src/state/conversationStore.test.ts
last_verified_commit: null
---

# ADR-0031: Safe project work loop and durable tool lifecycle

- **Status:** Accepted
- **Date:** 2026-07-24

## Context

The React workbench could display tool-like activity, but the presentation was
not driven by one complete runtime contract. Project folders lived in renderer
storage, folder selection did not grant Rust authority, `write_file` could use
the shared runtime workspace, approval-required state was inferred from prose,
and execution manifests retained terminal hashes rather than replayable tool
lifecycle events. A renderer reload could therefore lose tool cards or expose
serialized provider tool calls as assistant prose.

The next milestone needs a safe vertical loop: authorize a real project root,
bind a mutation to that root, pause on the exact SmartDeny action, execute only
after that grant, and preserve truthful tool state across reload and reconnect.

## Decision

1. **Rust owns project authority.** `project-authority.json` stores versioned
   project root scopes under the Optimus home. New roots require a short-lived,
   single-use opaque grant staged only from a native folder selection. The
   Electron host exchange additionally requires a separate random main-process
   secret that is never exposed through the renderer bridge. The renderer
   cannot fabricate a path grant. Existing scopes may be retained or narrowed
   without silently broadening authority.
2. **Canonical paths are mandatory.** Authorized roots must exist, be
   canonical directories, avoid secret-bearing components, and not contain the
   runtime home or a filesystem root. One authorized root is explicitly primary.
3. **Project effects bind the root identity.** `ProjectWriteFile` and
   `ProjectRunCommand` persist the SHA-256 identity of the canonical workspace.
   Runtime reopens the exact authorized root for approval execution and rejects
   a changed or foreign root before the effect runs.
4. **SmartDeny sees the exact mutation.** Project writes and commands are
   high-risk effects. The approval lifecycle summary describes the exact
   relative path/byte count or command. A grant remains bound to the persisted
   job, node, and effect hash; it cannot authorize another root or action.
5. **Tool state is typed and runtime-owned.** Every tool call has stable run,
   call, and event identities and emits explicit `started`,
   `approval_required`, `succeeded`, `failed`, `cancelled`, `suppressed`, or
   `ambiguous` phases. Terminal events may carry the validated canonical
   `ToolOutcome`; UI state never infers a phase from status prose.
6. **Persist before projection.** Each lifecycle transition is inserted into
   `execution_tool_events` before stream delivery. Stable event IDs make
   duplicate delivery idempotent. Events retain ordered per-session/turn
   identity and the full typed payload required to rebuild tool cards.
7. **Reload is a projection, not protocol exposure.** `get_session` removes
   system messages, tool-result protocol messages, and serialized assistant
   tool-call arrays. It attaches ordered durable lifecycle events to the owning
   assistant turn, synthesizing an empty assistant projection when a turn stops
   for approval. React reduces events by call identity and seeds its event-ID
   dedupe set before reconnect delivery.
8. **Unrestricted access remains explicit.** Only the explicit `full` access
   selection uses the unrestricted effect policy. Other chat access modes use
   SmartDeny, and an absent project scope does not fall back to a shared folder.
9. **Transcript decisions settle the exact paused turn.** A bound approval card
   repeats the durable run, call, job, node, node index, and effect digest when
   the user approves or denies. Rust validates that full identity, reopens only
   the authorized project scope, executes at most the approved effect, and
   appends a terminal tool event plus assistant receipt before settling the
   turn and manifest. Denial executes nothing. The renderer then rebuilds from
   `get_session` rather than inventing an optimistic terminal state.

## Alternatives considered

### Trust renderer project paths

Rejected. Local storage is presentation state and a compromised renderer could
invent or broaden paths without a user-mediated native selection.

### Approve a generic write or command capability

Rejected. A broad approval cannot prove which root, path, bytes, or command the
user authorized and would weaken the existing exact-effect ledger.

### Reconstruct tool cards from transcript strings

Rejected. Provider protocol JSON and human prose are not a stable lifecycle
contract, cannot represent an approval pause honestly, and make deduplication
ambiguous after reconnect.

### Persist terminal tool outcomes only

Rejected. Approval-required and interrupted calls have no terminal outcome, so
terminal-only receipts would erase the most safety-sensitive UI state.

## Reasons

This design uses the existing Work Graph, SmartDeny, and execution manifest as
the authoritative waist. Native selection proves user intent to add a root;
canonical hashes bind later effects; exact approvals retain least authority;
and a durable typed event log lets every frontend projection be discarded and
rebuilt without changing execution truth.

## Consequences

- Multi-folder projects now have a Rust-enforced runtime scope in addition to
  their renderer catalog.
- Project writes pause before mutation under the default access mode and resume
  against the exact authorized root after approval.
- Tool cards and approval state survive a renderer reload or duplicate
  reconnect delivery.
- A pending card can now approve or deny its exact action in the transcript;
  the decision and terminal receipt remain visible after reload.
- `execution.db` stores full lifecycle payloads in addition to the pre-existing
  provenance hashes and timing evidence.
- Older sessions without lifecycle rows remain readable but cannot gain tool
  cards that were never durably recorded.

## Risks and unresolved boundaries

- **Known boundary (refined by ADR-0035):** approved commands are not
  `cap-std` file effects; they use `CommandFsEnvelope` (Linux confined
  workspace-only RW; Windows Job Object residual; UnrestrictedHost break-glass).
- **Known boundary:** project authority is local-machine state and has no
  cross-device synchronization or enterprise policy layer.
- **Known boundary:** approval resolution deterministically settles the paused
  turn with a concise receipt. It does not make another provider call because
  the original provider, model, and access configuration are not yet durably
  recoverable as a resumable generation lease.
- **Unknown or unresolved behaviour:** native picker provenance proves a local
  selection event, not operating-system identity or remote filesystem trust.

## Evaluation evidence

- Project authority tests cover single-use selection grants, persistence,
  concurrent mutation locking, expiry/fabrication rejection, narrowing, and
  primary-root validation; desktop tests cover the internal staging secret.
- Runtime tests cover exact project writes and cross-workspace replay rejection
  without creating a file.
- Kernel tests cover explicit approval lifecycle before any write effect and
  durable ordered/idempotent lifecycle receipts after reopen, plus exact-bound
  approval, denial, mismatch rejection, and turn/manifest settlement.
- Desktop tests prove an approval reopens the exact authorized project root and
  `get_session` rebuilds tool cards without returning protocol JSON. IPC tests
  prove malformed or mismatched approval identities cannot reach execution.
- React tests prove stable event/call identity, overlapping calls, approval
  retention, reload reconstruction, reconnect deduplication, and accessible
  in-transcript approval/denial controls with pending and error feedback.

## Relevant code

- `crates/optimus-kernel/src/project_authority.rs`
- `crates/optimus-kernel/src/execution.rs`
- `crates/optimus-kernel/src/lib.rs`
- `crates/optimus-runtime/src/lib.rs`
- `apps/optimus-desktop/src/ipc/sessions.rs`
- `apps/optimus-desktop/src/ipc/chat.rs`
- `apps/optimus-ui/src/components/workbench/ActivityTimeline.tsx`
- `apps/optimus-ui/src/state/conversationStore.ts`

## Relevant tests

- `crates/optimus-runtime/tests/approvals_surface.rs`
- `crates/optimus-kernel/tests/kernel_turn.rs`
- `crates/optimus-kernel/src/execution.rs`
- `apps/optimus-desktop/src/ipc/runtime_ops.rs`
- `apps/optimus-desktop/src/ipc/sessions.rs`
- `apps/optimus-desktop/src/ipc/chat.rs`
- `apps/optimus-ui/src/components/workbench/Transcript.test.tsx`
- `apps/optimus-ui/src/state/conversationStore.test.ts`

## Conditions for reconsideration

Reconsider the native grant format if project authority moves to an OS-backed
capability broker. Reconsider the event schema through an additive versioned
migration if tools gain parallel child runs or nested approval scopes. Do not
replace exact root/effect binding with a broader convenience permission.

## Addendum (2026-07-25)

Command FS residual refined by **ADR-0035** (P12):
approved commands use `CommandFsEnvelope` (Linux confined workspace-only RW;
Windows Job Object residual; UnrestrictedHost break-glass). Historical wording
above remains for decision-time context.
