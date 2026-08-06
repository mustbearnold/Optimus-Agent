---
doc_id: architecture-security-observability
doc_type: explanation
plane: current
status: current
authority: canonical
summary: Security and approvals, events/observability/replay, GPU/CPU fallback, and architectural debt.
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: architecture
owns:
  - crates/optimus-kernel/src/lib.rs
  - crates/optimus-runtime/src/lib.rs
---

# Security, observability, and architectural debt

## Security and approvals

**Confirmed current behaviour:** SmartDeny is the default runtime policy and
`RunCommand` is the only current Work Graph effect classified high-risk.
Approval decisions are durable and bound to exact job, node, and SHA-256 effect
identity, with actor, creation time, expiry, denial, and revocation metadata.
They do not transfer to changed effects or later nodes. Skills cannot expand
their declared permissions; a skill can grant a terminal action approval only
if it declares `Terminal`.

**Confirmed current behaviour:** `FsRoots` reads are rooted, canonicalized,
symlink/prefix checked, and secret-name denied. Runtime `WriteFile` and
`AssertFileEquals` accept only normal path components and resolve through a
retained `cap-std` workspace directory capability. Root replacement and linked
Windows junction/Unix symlink ancestors cannot redirect built-in effects. Kernel
and runtime share one case-insensitive secret-basename policy. Browser HTTP
navigation rejects non-HTTP(S),
loopback, private, link-local, local, and metadata targets before and after
redirects.

**Confirmed current behaviour:** campaign persistence rejects malformed scalar or
step JSON fields and validates a migrated expected step count plus contiguous
indices before any runtime effect. Missing or partially reassigned steps cannot
silently shorten the executable plan. Campaign schema v4 has transactional
migrations, future-version rejection, read-only legacy import, diagnostics, and
deterministic projection repair plus fenced owner leases. Campaign status is
derived from Work Graph jobs in the same SQLite database.

**Confirmed current behaviour:** native desktop uses Wry IPC on a custom origin.
HTTP mode and the webhook gateway bind to `127.0.0.1`.

**Confirmed current behaviour:** the Tauri production renderer loads built
relative assets embedded in the shell binary. The Tauri bridge (`host_invoke`)
forwards typed methods to the Rust host, which owns the bearer token and every
durable effect; the shell adds bounded chat streams, window chrome, and native
folder selection. The renderer never receives `OPTIMUS_HTTP_TOKEN`.

**Confirmed current behaviour (P15):** IPC matrix gate requires host registry ⊇
React `DesktopMethod` (the renderer surface over the Tauri bridge); every host
method is renderer-callable or classified non-invoke/main-only. Critical
invokes include approvals, project scopes, sessions, fs, settings, `term_run`,
and `jobs_list`. `project_root_stage_native` stays main-only. See ADR-0038.

**Confirmed current behaviour:** the workbench Browser surface drives the
kernel `browser_*` effector (HTTP SSRF-safe, CDP when available). The
Electron-era `WebContentsView` preview and its native annotation mode are
retired with Electron; the agent browser tools own all renderer browser
activity and no shared cookies, history, or automation target is claimed
beyond the effector's own bounded state.

**Confirmed current behaviour:** the React project catalog can group several
folder paths under one local project identity and nominate one primary root.
Legacy single-path records migrate to `rootPaths[]`. New roots become runtime
authority only after a native picker stages a single-use token and Rust accepts
the canonical scope; renderer presentation state alone grants nothing.

**Confirmed current behaviour:** desktop HTTP mode is explicitly development-only
and requires a 32-character bearer token. Effectful POSTs additionally require
an exact loopback origin and CSRF header; wildcard CORS is disabled. The gateway
requires its own 32-character bearer token and validates any supplied browser
origin. Both surfaces cap request bodies, apply fixed-window request limits,
bound aggregate operations, omit home paths from health responses, and return
stable redacted errors while retaining local stderr diagnostics.

**Confirmed current behaviour:** SmartDeny treats `WriteFile`,
`ProjectWriteFile`, `RunCommand`, `ProjectRunCommand`, and `ProjectServe` as
high-risk. `AssertFileEquals` does not require approval.

**Confirmed current behaviour (ADR-0059):** the Standard broker lane permits
direct project work while recognised remote/network commands and command-string
shell forms ask; uncheckpointed project deletes also ask. Classification does
not prove an arbitrary binary lacks network or ambient-credential authority,
so those remain explicit blockers to a universal Standard fallback.

**Confirmed current behaviour (ADR-0060 foundation):** owned-localhost
capabilities require a coherent project/session/run/process-tree/socket
constraint envelope, which cannot ride an unrelated capability. The pure broker
does not establish liveness. The agent CDP backend is public-only unless
constructed with one exact numeric HTTP loopback origin, and it checks
navigation, intercepted requests, and post-click URLs. The HTTP backend follows
redirects manually and validates each target before connection. The runtime now
contains a default-inactive lease registry: a copied binding cannot become
authority without exact live membership, the same opaque execution context, current
generation/expiry, retained-listener liveness, and a non-serializable use
guard. Revocation removes membership before bounded use drain and process
cleanup. No production constructor can create the opaque listener proof yet;
no production path can mint the execution context either. The structured
issuer/owned-server lifecycle, timer-driven expiry, restart orphan cleanup, and
worker/service-worker target coverage remain absent. This is still a
fail-closed authority substrate rather than a shipped localhost product path.

**Confirmed current behaviour (P12):** approved commands use `CommandFsEnvelope`
(default confined): Linux bwrap binds the workspace read-write only (no full
root rw bind); `UnrestrictedHost` is explicit break-glass. See ADR-0035.

**Known residual (product-visible):** Windows command FS is Job Object process-
tree ownership under confined mode; `ConfinedNoNetwork` fail-closes on non-
Linux. Provider/OAuth TLS is adapter-local beyond shared browser/search egress.

## Events, observability, and replay

**Confirmed current behaviour:** Work Graph events have an ordered SQLite
sequence and optional job/node IDs. Model turns expose in-process text, tool,
and status stream events. Sessions, cron, campaigns, gateway, skills, and memory
also retain subsystem-specific state.

**Confirmed current behaviour (P14):** machine-readable local causal export
`optimus.causal.v1` (`optimus trace export` / `write_causal_export`) is
store-backed, versioned, and redacts the Optimus home path. It does not re-run
live providers and is not OTLP. Merge gate
`scripts/gates/check-observability-gate.py` covers integrity, causal/export tests,
and export API surface. See ADR-0037.

**Confirmed current behaviour:** versioned execution manifests and immutable,
bounded, content-addressed fixture bundles support zero-effect offline replay.
Planning binds exact source manifest, trace, policy, tool catalog, stage order,
fixture hashes, and terminal evidence. Input or fixture drift fails before later
stages and one immutable replay report records the terminal comparison.

**Confirmed current behaviour:** canonical trace/span identities support ordered
append-only events, one terminal span outcome, traced route decisions, and
immutable execution-manifest trace links. Versioned evaluation datasets retain
ten declared cases and produce deterministic candidate-bound metrics, thresholds,
reports, immutable baselines, and regression comparisons. Comparison permits a
changed source-tree identity only while dataset, contract, tool catalog, route
policy, provider/model, threshold policy, report hashes, and metric schema remain
compatible. Report construction, baseline acceptance/loading, and comparison
revalidate supported identities, exact metric dimensions and arithmetic, unique
threshold policy, failure/pass projection, and content hash before returning or
persisting evidence. Report construction also rejects observations without trace
evidence when the matched dataset case declares that trace is required.

**Confirmed current behaviour:** production kernel turns create the execution
manifest and one parentless trace link atomically in the execution database.
Successful results expose that exact context; interrupted turns reuse it after
validating manifest identity and running status. Missing, malformed, mismatched,
or already-terminal resume evidence fails before model or tool execution. The
execution link does not claim that a corresponding `TraceStore` span exists.

**Confirmed current behaviour:** a public offline integrity executor exercises
the six required memory, SmartDeny, routing, cancellation/fencing, and gateway
cases against isolated local run state. It requires run-directory ownership
before execution, matches policy-specific denial outcomes, and returns a complete
deterministic failed report when setup is unavailable. It does not execute an
approved command or access the network. Usable runs persist one evaluation-owned
root span per case with hashed evidence and terminal status, then return the exact
read-back trace context and deterministic replay class. Independent retries use
fresh trace identities and stable normalized semantics.

**Confirmed current behaviour:** a separate exact four-case offline trajectory
runner reloads each successful turn's execution evidence and returns exact
assistant text, canonical invoked tools, terminal status, replay classification,
and root trace. Missing or mismatched persisted evidence fails the case; failed
cases carry no typed success evidence.

**Confirmed current behaviour:** the exact Priority-2 report runner owns a fresh
run directory, executes the four trajectory and six integrity cases, projects
their typed evidence in canonical dataset order, and returns one deterministic
candidate-bound report. Per-case latency and cost are mandatory explicit inputs;
they are not inferred from wall time or silently defaulted. Equal inputs yield
equal report bytes while run and trace identities remain fresh.

**Confirmed current behaviour:** `optimus eval report` reads candidate binding,
per-case measurements, and optional thresholds from separate bounded JSON files.
Typed policies are preflighted before evaluation run state. Success and threshold
failure both print the complete report; threshold failure exits non-zero. The
legacy four-case `eval run` command remains available.

**Confirmed current behaviour:**
`python scripts/tools/engineering_memory.py binding` emits the only context accepted by
the exact offline runner: the current canonical source-tree identity, canonical
evaluation/tool/routing source hashes, and fixed `offline/offline-scripted`
provider/model identity. The runner rejects context drift before creating run state.

**Confirmed current behaviour:** `optimus eval compare` reads two bounded exact
reports, invokes the canonical candidate-aware comparator, and prints one comparison
without creating the configured home. A valid regression is comparison evidence,
not an implicit release gate; invalid or incompatible reports fail without output.

**Confirmed bounded behaviour:** desktop stream delivery loss requests the same
cooperative token used by active providers and tool-loop boundaries. A turn can
still commit a durable effect and then fail before the session transcript is
saved.

**Confirmed current behaviour:** operators can reconstruct a turn from durable
stores via `load_causal_turn` / `optimus trace show` using a root trace id,
manifest id, or turn id. Security/policy fences map to a closed
`SecurityDenialCode` vocabulary when classifiable. Offline integrity + causal +
export surface tests are the observability gate
(`scripts/gates/check-observability-gate.py`, P14).

**Unknown or unresolved behaviour:** there is no OpenTelemetry/OTLP export (local
`optimus.causal.v1` export exists — ADR-0037), live security-denial event stream,
token accounting, artifact publication lineage, GPU/fallback telemetry, or a
transaction spanning trace, route, execution, runtime, agent, workflow, and
session stores. Fixture replay does not rerun live providers or external
effects. Logs remain non-authoritative.

## GPU and CPU fallback

**Confirmed current behaviour:** no CUDA, GPU crate, embedding backend, vector
index, reranker, or local-model runtime is implemented. Core functionality is
CPU-only and does not require CUDA.

**Planned behaviour:** GPU adapters may accelerate embedding similarity,
batching, reranking, and local utility inference when benchmarks justify them.
Each adapter must remain replaceable and have correctness-tested CPU fallback.
RTX 5070 12 GB is a development constraint, not permission to make GPU
availability mandatory.

## Current architectural debt and open decisions

1. **Partial product:** two built-in specialists and a registered-definition DAG
   runner exist; no model-chosen specialist router or open MCP agents.
2. **Partial product:** DAG executor runs registered built-in definitions with a
   closed specialist dispatch table; not a universal executor for arbitrary
   third-party definitions beyond registry validation.
3. **Partially implemented:** cancellation remains owner-specific.
4. **Confirmed contract, unresolved product:** metadata declarations do not create universal runtime cancellation/retry.
5. **Partially implemented:** policy and telemetry routing exist; evaluation-driven routing does not.
6. **Confirmed bounded behaviour:** fixture replay and local causal traces exist; live-effect replay and distributed tracing do not.
7. **Unknown/unresolved:** provenance and artifact publishing contracts.
8. **Confirmed (P12) / residual:** file effects use `cap-std`; approved commands
   use `CommandFsEnvelope` (Linux Confined = workspace-only RW; Windows Confined
   = Job Object process-tree residual; `ConfinedNoNetwork` fail-closed non-Linux;
   `UnrestrictedHost` explicit break-glass). See ADR-0035.
9. **Confirmed bounded behaviour:** Work Graph terminal uniqueness and campaign,
   cron, and gateway owner/generation/token/deadline fencing are implemented;
   external exactly-once delivery remains unresolved.
10. **Confirmed current behaviour (S+++ Phase 1B):** if durable effect links
    exist without matching tool transcript messages, session open injects
    deterministic repaired tool messages from the links and persists them.
11. **Resolved (P16):** duplicate ADR number `0016` aliased as **ADR-0016-A**
    (tool contract) and **ADR-0016-B** (FS sandbox); historical file names kept.
12. **Residual (owned by P16 banners / ongoing):** blueprint and historical
    phase notes may mix plan vs current; readers use status banners and the
    Confirmed/Planned/Unknown legend. Do not rewrite history to hide priors.
13. **Program:** architecture quality marks live in
    [architecture-marks.md](../runbooks/architecture-marks.md). Foundation Phases 0–5:
    s-plus-trust-spine.md (atticked) (done). S+++ climb
    **P10–P19 done** — all architecture marks **S+++** (board:
    s-plus-plus-plus-review-2026-07-25.md (atticked);
    history: s-plus-plus-plus-program.md (atticked)).
    **Closed product program:** product-complete-program.md (atticked)
    (program P20–P29 **PRODUCT-COMPLETE** with residuals); historical task record
    full-app-microtasks.md (atticked). Current roadmap:
    current/roadmap.md (see specs/BACKLOG.md); named phase programs are
    historical implementation records unless that roadmap promotes them.
    Operator gate matrix: release-and-parity-gates.md (merged).
    Durability backup/doctor: durability-and-backup.md (merged).
