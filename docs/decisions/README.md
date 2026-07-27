---
knowledge_type: decision-index
status: current
covers:
  - docs/decisions/*.md
depends_on:
  - docs/architecture/system-overview.md
validated_by:
  - scripts/test_engineering_memory.py
last_verified_commit: null
---

# Architecture decision index

This index preserves existing decisions and exposes their current documentary
state. Source and tests still determine whether an accepted design is fully
implemented.

**Identity:** `ADR-NNNN` is the **decision** plane only. It is never equal to
S+++ program phase `P##` or delivery `PR #N`. See
[artifact-naming.md](../contributing/artifact-naming.md).

| ID | Decision | Documentary status | Implementation interpretation |
|---|---|---|---|
| 0000 | Locked defaults | Historical locked-defaults note; not full ADR shape | Mixed product constraints; verify individually. |
| 0001 | Kernel language and Work Graph spine | Accepted | Confirmed current core. |
| 0002 | Memory invariants | Accepted; full store originally phased | Confirmed current memory core with documented gaps. |
| 0003 | Policy, budgets, bounded commands | Accepted | Confirmed current core. |
| 0004 | MetaMemory MVP | Accepted | Confirmed current memory core. |
| 0005 | Skills 2.0 | Accepted | Confirmed current skill registry. |
| 0006 | Capability packs | Accepted | Confirmed current pack core; many catalog tools remain unavailable. |
| 0007 | Provider-agnostic turn loop | Accepted | Confirmed current behavior. |
| 0008 | OpenAI-compatible provider | Accepted | Confirmed current adapter. |
| 0009 | Durable sessions | Accepted | Confirmed current behavior. |
| 0010 | Context compression | Accepted | Confirmed current behavior with limited evaluation. |
| 0011 | Codex OAuth | Accepted | Confirmed current adapter and plain-JSON auth-store debt. |
| 0012 | Kernel effectors | Partly superseded by canonical contract | Use ADR-0016 plus current source. |
| 0013 | Command capture | Accepted | Confirmed current behavior. |
| 0014 | Native WebView IPC mode | Accepted | Confirmed current native/HTTP split. |
| 0015 | Preview Browser via CDP | Design accepted; implementation phased | Planned behavior; current browser is bounded HTTP and preview UI is absent. |
| 0016-A | Canonical tool/pack contract | Accepted (aliased; file `0016-canonical-tool-contract.md`) | Confirmed current canonical contract; independent security review remains separate delivery evidence. |
| 0016-B | Filesystem sandbox allowlist | Accepted (aliased; file `0016-fs-sandbox-allowlist.md`) | `FsRoots` reads Confirmed; runtime write confinement governed by ADR-0018 / envelope ADR-0035. |
| 0017 | Repository-local Engineering Memory | Accepted | Implemented by docs, skill, generator, tests, and generated indexes. |
| 0018 | Fail-closed runtime path and campaign decoding | Accepted; limitations superseded by 0019 | Historical normal-component and strict-decoding decision; see 0019 for current filesystem and campaign authority. |
| 0019 | Capability files and unified campaign authority | Accepted; limitations superseded by 0020 | Retained workspace capability, shared secret policy, unified campaign authority, deterministic handoff, and job-derived campaign status. |
| 0020 | Work Graph integrity and loopback security | Accepted | Atomic transitions, terminal uniqueness, schema-v4 campaign leases, durable attempts/cancellation, exact-action approvals, and authenticated bounded loopback APIs. |
| 0021 | Owned execution and causal delivery | Accepted | Suspended Job Object command ownership, cooperative model cancellation, leased cron/gateway attempts, reconciled transactional outbox, and session-to-effect provenance. |
| 0022-A | Versioned agent and workflow contracts | Accepted | Typed immutable agent/workflow identities, permission closure, durable invocation outcomes, and explicit adapter capability limits. |
| 0023 | Fixture replay, causal traces, routing telemetry, and versioned evaluation | Accepted | Immutable bounded replay/evaluation evidence with exact identities and deterministic comparison. |
| 0024 | Fail-closed Hermes parity version gate | Accepted | Independent Optimus SemVer plus exact-release feature, quality, speed, cost, memory, and release-collision gates. |
| 0025 | Artifact workbench and owned presentation state | Accepted | Compact Vantage workbench, one mounted-pane state owner, stable frame-committed streaming, bounded no-inertia motion, and a reversible visual seam. |
| 0026 | Separate development and runtime agent instructions | Accepted | `AGENTS.md` governs repository development; `OPTIMUS_AGENTS.md` governs installed product sessions. |
| 0027 | Settings-driven work isolation modes | Accepted; Phase 0 store/UI | Durable `settings.json` + Settings UI; project-bound/profile enforcement planned. |
| 0028 | Electron + React shell over Rust host | Accepted; migration in progress | Electron/React frontend; durable authority stays Rust; strangler via host HTTP. |
| 0029 | React workbench and Electron preview view | Accepted; repository cutover implemented | React is the default Electron renderer, main mediates the token and SSE, and a sandboxed `WebContentsView` owns user-preview pixels; installed cutover remains planned. |
| 0030 | Codex-measured shell and multi-folder projects | Accepted; project-authority boundary superseded by 0031 | Measured neutral geometry, versioned `rootPaths[]` projects, categorized Settings, bounded native annotations, and overlay-aware preview suspension are implemented. |
| 0031 | Safe project work loop and durable tool lifecycle | Accepted | Rust-authorized canonical project roots, root-bound SmartDeny effects, typed persisted tool events, and reload/reconnect projection are implemented. |
| 0032 | Compact Engineering Memory facts and budgeted agent lenses | Accepted | Schema v2 compact indexes, hash-only staleness, pattern impact, and context/report lenses are implemented. |
| 0033 | Multi-agent DAG execution (P10) | Accepted | Two specialists, three registered workflows, durable `WorkflowRunStore`, parent cancel tree; multi-agent mark **S+++** after P12 command-FS close (registered-only; no open-ended spawn). |
| 0034 | Control-plane crate peels (P11) | Accepted | `optimus-agent` / `optimus-workflow` / `optimus-artifacts` peels; kernel re-export waist; layer lint; control-plane mark **S+++**. |
| 0035 | Command capability envelope + Unrestricted break-glass (P12) | Accepted | Linux confined bwrap (workspace-only RW); `CommandFsEnvelope` orthogonal to SmartDeny; Windows residual / fail-closed; shared egress helper; Security **S+++**. |
| 0036 | Domain modularity — single catalog and memory planes (P13) | Accepted | ToolDesc-only catalog; plane-separated auth; domain gate script; Domain **S+++**. |
| 0037 | Local causal export (not OTLP) — P14 | Accepted | `optimus.causal.v1` JSON export + redaction; obs gate; Observability **S+++**. |
| 0038 | UI IPC architecture completeness (P15) | Accepted | Matrix 100% host classification; expanded critical invokes; preview sandbox tests; UI **S+++**. |
| 0039 | Files-mutate effect taxonomy (program P22) | Accepted | Mkdir/Delete/Rename/Patch (+ Project*); SmartDeny high-risk; single Work Graph plane. |
| 0040 | SharedBrowserContract (program P23) | Accepted | Dual trust domains (UserPreview vs AgentEffector); host coordination bus; supersedes ADR-0015 shared-session claim. |
| 0044 | Bounded project trust + capability broker (program P30) | Accepted | `optimus-policy` broker; Standard auto-authorize with exact receipts; autonomy ≠ containment. |
| 0045 | Agent host + surface transports (TUI hub) | Proposed | Registry moves to `optimus-host`; stdio for TUI, loopback HTTP for Electron; attach before spawn. |
| 0046 | Approving an exact action resumes the turn | Proposed | Settlement stops being terminal; tool result carries the receipt body; the approved call is still never regenerated. |
| 0047 | A turn's step budget is 32 model round trips | Proposed | Default `max_steps` 8 → 32; approval round trips consume the budget, so 8 starved ordinary turns. |

## Known documentary debt

- **Resolved (P16):** two historical files share number `0016`; titles and index
  use **ADR-0016-A** / **ADR-0016-B** aliases without renumbering files (history
  preserved). Residual is dual pathnames only, not ambiguous identity.
- **Confirmed current behaviour:** two files use ADR number `0022`. They remain
  unchanged to preserve history; use the A/B labels only in this index.
- **Confirmed current behaviour:** ADRs 0000–0016 predate the full template and
  omit one or more modern fields such as alternatives, risks, evaluation
  evidence, or reconsideration conditions.
- **Planned behaviour:** new ADRs use the full template. Existing ADRs may gain
  non-destructive addenda, but must not be rewritten to conceal prior reasoning.
- **Unknown or unresolved behaviour:** no automated source proves that every
  historical accepted ADR remains implemented; contract/source/test maps must
  be consulted for each change.
