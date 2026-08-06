---
doc_id: architecture-tools-modularity
doc_type: explanation
plane: current
status: current
authority: canonical
summary: Tool system contract (ToolDesc) and domain modularity (P13 / ADR-0036) — current behaviour.
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: architecture
owns:
  - crates/optimus-packs/src/lib.rs
---

# Tool system and domain modularity

## Tool system

**Confirmed current behaviour:** `optimus-packs::ToolDesc` is the canonical
implemented tool contract. It owns stable ID, description, provider input and
output schemas, policy and invocation identity, replay class, retry,
idempotency, timeout ownership, cancellation, observability declarations,
availability, pack ownership, and schema-token cost. Available tool calls are validated against the exact set
advertised for that model step, including non-empty unique call IDs, before any
sibling effect runs.

**Confirmed current behaviour:** provider responses have a hard ceiling of 64 tool
calls before effects and a default execution budget of eight calls per model step.
Valid overflow calls receive typed suppressed outcomes and force the next request
to advertise no tools. Exact normalized repeated `web_search`, `memory_recall`, and
`skill_resolve` calls are suppressed after their first execution in a turn; mutable
and context-sensitive tools are not semantic-deduplicated.

**Confirmed current behaviour:** available tools are `read_file`, `write_file`,
`terminal`, `web_search`, `memory_recall`, `skill_resolve`, `activate_pack`,
`browser_navigate`, `browser_click`, and `browser_snapshot`. Other catalog items
are explicit unavailable placeholders and are not advertised to models.

**Confirmed current behaviour:** `write_file` and `terminal` route through
durable jobs. `terminal` pauses under SmartDeny until a separate grant bound to its exact
job/node/SHA-256 effect identity. Browser tools use an HTTP text/link effector,
not CDP. `read_file`
uses the filesystem sandbox and denies secret basenames.

## Domain modularity (P13 / ADR-0036)

**Confirmed current behaviour:** domain ownership is single-catalog and
plane-separated (grade **S+++** in architecture-marks):

| Plane | Owner | Must not |
|---|---|---|
| Tool identity | `optimus-packs::ToolDesc` / `ToolId` / `ToolInvocation` | Second catalog in kernel or surfaces |
| Session transcript | `SessionStore` | Authorize host effects |
| Semantic memory | `optimus-memory` | `ActionAuthorize` / live capability grants |
| Procedural skills | `optimus-skills` | Expand closed permissions; grant wrong effect class |
| Work Graph jobs | store / graph / runtime | Own chat UI schema |
| Engineering Memory | repo docs / EM scripts | Runtime authorization |

Kernel dispatch resolves only `packs.resolve_loaded_tool` then matches on
`ToolInvocation`. Skill grants are class-scoped (`FsWorkspace` → writes,
`Terminal` → commands). Gates: `scripts/gates/check-domain-modularity.py` and
`cargo test -p optimus-kernel --test domain_modularity`.

**Confirmed current behaviour:** project sessions load canonical roots from the
Rust-owned project authority store. Reads use the authorized root set. Writes
and commands persist the primary workspace hash, are high-risk under SmartDeny,
and reopen the exact matching authorized root when an approval is granted.

**Confirmed current behaviour:** tool streams use stable run/call/event IDs and
explicit lifecycle phases. Each transition is stored before delivery in an
ordered execution event table. Desktop session reload removes provider protocol
messages and attaches those events to the owning assistant turn; React reduces
them by call identity and deduplicates reconnect delivery by event identity.

**Unknown or unresolved behaviour:** owner-specific runtime paths do not yet
implement universal cooperative cancellation or retries merely because the
descriptor declares their support boundary.
