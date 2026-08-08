---
doc_id: spec-019-tool-catalog-breadth
doc_type: reference
plane: work
status: current
authority: canonical
summary: Filling the empty capability packs under ADR-0068 discipline — Devex (git read tools first, write tools under SmartDeny), Media expansion (image generation, TTS, STT on config-gated providers), Social (durable sends over the spec-017 gateway contract), and Office (docx/xlsx/pdf read + extract) — every tool through the kernel tool-add ceremony in one commit, with schema-budget and module ratchets respected and zero placeholder catalog rows.
reviewed_on: 2026-08-08
review_by: 2026-11-08
knowledge_type: specification
covers:
  - crates/optimus-packs/src/catalog.rs
  - crates/optimus-packs/src/invocation.rs
  - crates/optimus-kernel/src/tool_dispatch.rs
  - scripts/gates/check-tool-coverage.py
  - specs/006-memory-skills-packs/spec.md
depends_on:
  - docs/decisions/0068-a-catalog-row-must-dispatch-or-not-exist.md
  - specs/017-gateway-breadth/spec.md
---

# Spec-019: Tool catalog breadth — filling the empty packs

Status: current
Owner: optimus-agent-development (prompt-only owner)

## Revision table

| Round | Verdict | Findings | Fixes |
|---|---|---|---|
| 1 | REJECTED | B1: budget ratchet claim wrong (test is Core + two HEAVIEST, never mean); B2: Office tools unreachable (activation enum lacks office; coverage pins rejection); 3 nits (spec-020 now current, git_commit replay class, A4 threshold unpinned) | B1: worst-case wording + packs_budget.rs cite; B2: activation-enum widening + coverage-pin replacement mandated in R2; 3 nits applied (round 2) |
| 2 | REJECTED | B1: "policy: normal" names a nonexistent ToolPolicy variant (lib.rs:431-444); social_reply lacked a policy; 4 nits (home pin survival, ToolDesc location, ADR-0071 link, WER fixture pinning) | B1: real variants named (WorkspaceRead/WorkspaceWrite/Media/NetworkWrite) at every tool; 4 nits applied (round 3) |
| 3 | APPROVED | 1 non-blocking nit (git_commit retry semantics — new SHA per attempt) | Applied 2026-08-08 (retry rule pinned in R1) |

## Purpose

Four of Optimus's capability packs are empty shells and the fifth is a
single tool. `crates/optimus-packs/src/catalog.rs` defines Devex
("no tools until designed"), Social ("returns with a live gateway
transport"), Home ("no tools until integrated"), and Office ("no tools
until integrated") with zero tools, and Media with exactly one
(`VisionAnalyze`) — image generation and TTS are explicitly parked
until "their lane" is designed (ADR-0068). Hermes, the parity target,
ships image generation, TTS/STT, document, and developer tools as
first-class capabilities.

This spec designs the missing lanes as a mechanism: which packs get
which tools, with what policy, replay class, and dependency — and
requires every tool to arrive through the kernel tool-add ceremony
(invocation enum → catalog → dispatch → coverage pin → EM test →
spec amendment, all in ONE commit), under ADR-0068's law that a
catalog row must dispatch or not exist. No placeholder rows, no
vacuous tests, no unapproved effect.

## Current state (Confirmed behaviour)

- `crates/optimus-packs/src/catalog.rs` `builtin_catalog()` defines:
  Core (fs, terminal, web, memory, skills, packs, jobs, clarify),
  Browser, Desktop, Media (one tool: `VisionAnalyze`), Devex (0
  tools), Social (0 tools), Home (0 tools), Office (0 tools)
  (Confirmed: catalog source).
- Media's summary records the parking decision verbatim: "Vision
  analysis (imagegen/TTS return with their lane; ADR-0068)" and each
  empty pack's summary records its gate ("no tools until designed /
  integrated"; Social "returns with a live gateway transport")
  (Confirmed: catalog source).
- Every tool must dispatch: ADR-0068 ("a catalog row must dispatch or
  not exist") forbids non-dispatchable rows; the closed-registry gates
  fail a catalog row without a handler (Confirmed: ADR-0068).
- Adding a tool touches many sites in one commit: `ToolInvocation`
  enum + `ALL_DISPATCHABLE` in `crates/optimus-packs/src/invocation.rs`,
  catalog entry, dispatch arm in `crates/optimus-kernel/src/tool_dispatch.rs`,
  `DISPATCHABLE_EXERCISED` + a real dispatch test in
  `crates/optimus-kernel/tests/tool_coverage.rs`, the
  `PINNED_DISPATCHABLE` count in `scripts/gates/check-tool-coverage.py`,
  the pinned catalog reconciliation test in
  `scripts/tests/test_engineering_memory.py`, the module-size ratchet,
  and the owning spec's amendment — this is the validated ceremony
  (issue #134, `release_pack`, 2026-08-07) (Confirmed: ceremony
  reference + repo gates).
- Pack budget: a catalog-derived test asserts Core + the two HEAVIEST
  on-demand packs fit `PackBudgetConfig::default().max_schema_tokens`
  (worst case, never averages — spec-006 R6;
  `default_budget_fits_core_plus_heaviest_co_required_pair` in
  `crates/optimus-packs/tests/packs_budget.rs`); module baselines are
  shrink-only (Confirmed: `scripts/gates/check-module-size.py` +
  ceremony reference).
- Tools carry policy (approval level) and replay class
  (deterministic / model_nondeterministic / destructive / …) in their
  `ToolDesc` (Confirmed: `crates/optimus-packs/src/lib.rs`,
  `canonical_tool_output_schema` in catalog.rs).

## Requirements

### R1. Pack-content plan

The following tools are DESIGNED by this spec. Each MUST ship through
the full ceremony (R2) with the named policy and replay class:

- **Devex pack** — local git tooling, read-only first:
  - `git_status` (policy: `ToolPolicy::WorkspaceRead`; deterministic)
  - `git_log` (policy: `ToolPolicy::WorkspaceRead`; deterministic)
  - `git_diff` (policy: `ToolPolicy::WorkspaceRead`; deterministic;
    bounded output, no pager)
  - `git_commit` (policy: `ToolPolicy::WorkspaceWrite`; SmartDeny
    approval required — creates commits; replay class convergent (a
    commit is additive, not irreversible data loss); the tool MUST
    NOT push; retry within an approved turn re-runs the commit and
    reports the NEW SHA — one attempt per approval, a fresh commit
    object each time)
  - These operate on a workspace path passed in the invocation, never
    on the Optimus repo's own tree unless that path is explicitly the
    workspace (MUST).
- **Media pack expansion**:
  - `image_generate` (config-gated external provider — same provider
    catalog mechanism as model providers; produces an artifact in the
    artifact store with provenance; policy: `ToolPolicy::Media`;
    replay class external_nondeterministic)
  - `text_to_speech` (config-gated TTS provider; produces an audio
    artifact the desktop player can play; policy:
    `ToolPolicy::Media`; external_nondeterministic)
  - `speech_to_text` (config-gated STT; accepts an audio artifact path;
    policy: `ToolPolicy::Media`; external_nondeterministic; output
    includes transcript + confidence when the provider returns it)
- **Social pack** (DEPENDS on spec-017's gateway contract):
  - `social_send` (enqueue a durable outbound message to a routing
    address per ADR-0070/ADR-0071 via the spec-017 adapter contract;
    policy: `ToolPolicy::NetworkWrite`; replay: deterministic for the
    enqueue, delivery outcome reported per R8 of spec-017)
  - `social_reply` (reply to the originating thread of an inbound
    message; policy: `ToolPolicy::NetworkWrite`; same durability
    contract)
  - The pack's tools MUST NOT exist in the catalog until spec-017's
    adapter contract exists — until then the pack stays empty with its
    gate comment (MUST; ADR-0068 — a row that cannot dispatch must not
    exist).
- **Office pack** (read + extract first; write tools MAY follow):
  - `docx_read` (extract text + table structure from a .docx artifact;
    policy: `ToolPolicy::WorkspaceRead`; deterministic;
    stdlib/pure-Rust extraction library)
  - `xlsx_read` (extract sheets/cells from .xlsx; policy:
    `ToolPolicy::WorkspaceRead`; deterministic)
  - `pdf_extract` (extract text from PDF artifacts; deterministic
    where the PDF has a text layer; scanned PDFs return a named
    `pdf_no_text_layer` diagnostic rather than guessing; policy:
    `ToolPolicy::WorkspaceRead`)
- Home pack stays empty under this spec; its tools arrive with the
  Home Assistant integration (spec-020) (MUST).

### R2. The ceremony is mandatory per tool

- Every tool above MUST land through the validated ceremony in ONE
  commit: `ToolInvocation` variant + `ALL_DISPATCHABLE` +
  `id()`/`policy()`/`replay()`, catalog entry, dispatch arm (with
  system-prompt rebuild where the toolset changes mid-turn),
  `DISPATCHABLE_EXERCISED` bump + a REAL dispatch test through a
  scripted turn (vacuous tests fail adjudication), the
  `check-tool-coverage.py` pin bump, the EM catalog-reconciliation
  test update, module-size ratchet compliance, and the spec amendment
  (MUST).
- A tool whose dispatch test passes with its handler deleted is a
  defect, not a test (mutation-check the guard) (MUST).
- A pack that gains tools under this spec MUST also widen the
  activation enum (`ActivatePack`/`ReleasePack` name enums in
  `crates/optimus-packs/src/catalog.rs`, currently
  `["browser","desktop","media","devex","social"]` — `home` and
  `office` are deliberately outside) and MUST replace the `office`
  rejection pin in
  `crates/optimus-kernel/tests/tool_coverage.rs` with coverage for
  what the widening unlocks, in the same commit — the `home` pin
  SURVIVES, because the Home pack stays empty under this spec and
  its tools arrive with spec-020 (MUST; the pin's own message
  demands the coverage ledger move together with the widening).
- Each tool's input schema MUST be closed (`additionalProperties:
  false`) and its output MUST conform to the canonical tool output
  schema with a `replay` class and `provenance` (MUST).
- New code goes in NEW modules; baselined modules may only shrink
  (MUST; module-size ratchet).

### R3. Fail-closed provider gating for Media tools

- `image_generate`, `text_to_speech`, and `speech_to_text` MUST read
  provider configuration from the Optimus config; with no provider
  configured, the tool MUST return the named diagnostic
  `media_provider_unconfigured` and MUST NOT attempt any network call
  (MUST; fail-closed).
- Provider choice MUST use the existing provider catalog/failover
  mechanism rather than a parallel config path (MUST).
- GPU law (AGENTS.md law 14): any tool that can use local GPU
  acceleration MUST have a CPU fallback; the v1 media tools use
  external APIs and therefore have no GPU dependency, but a local
  provider added later MUST satisfy the law (MUST).

### R4. Devex workspace discipline

- `git_*` tools MUST take an explicit workspace path and MUST refuse
  (named diagnostic `devex_not_a_git_repo` / `devex_path_unsafe`) to
  operate on paths outside the workspace or on paths the invoking
  session does not own (MUST).
- `git_commit` MUST require SmartDeny approval and MUST report the
  produced commit SHA as its terminal outcome; it MUST NOT push
  (MUST).

### R5. Social durability

- `social_send`/`social_reply` MUST enqueue through the spec-017
  outbox (ADR-0070 durable obligation), MUST return a receipt handle,
  and MUST surface delivery failure as an error with the spec-017
  named diagnostic — never silent success (MUST).
- The pack MUST respect per-transport allowlists at enqueue time
  (sending to a chat not in the allowlist is refused with
  `transport_refused_unauthorized`) (MUST; spec-017 R6).

### R6. Office honesty

- Extraction tools MUST return the text actually extractable; a
  document with no text layer MUST produce the named diagnostic, not
  fabricated content (MUST).
- Office write tools (docx/xlsx/pdf creation) are MAY in this spec:
  if they ship, every generated file MUST round-trip through the
  corresponding read tool in the tool's dispatch test (MUST).

### R7. Observability and provenance

- Every new tool's outputs MUST carry provenance (artifact SHA-256 +
  source path/URL + tool id) per the canonical output schema (MUST;
  AGENTS.md law 16).
- Media artifacts MUST land in the artifact store with
  content-addressed keys, and MUST be playable/viewable through the
  existing artifact gallery (MUST).

## Acceptance criteria

- [ ] A1. Given the ceremony, when `git_status`, `git_log`, and
  `git_diff` land, then each has a real dispatch test through a
  scripted turn, the coverage pin and EM catalog test move in the same
  commit, and `just verify` passes with zero skips (R1 Devex, R2).
- [ ] A2. Given `git_commit` in a scratch workspace, when invoked
  without approval, then SmartDeny blocks it; with approval, then the
  commit SHA is returned and no push occurs (R4).
- [ ] A3. Given a configured image provider, when `image_generate` is
  invoked, then an artifact with provenance lands in the artifact
  store; given NO provider configured, then
  `media_provider_unconfigured` is returned with zero network calls
  (R3, R7).
- [ ] A4. Given the in-repo golden fixture audio (clean speech, pinned
  in the test tree), when `speech_to_text` runs with a configured
  provider, then the transcript's word error rate against the golden
  transcript is ≤ 5%, with the threshold re-verified per provider at
  fixture-pinning time (R1 Media, R3).
- [ ] A5. Given spec-017's adapter contract landed, when
  `social_send` enqueues to an allowlisted chat, then the outbox
  receipt returns and delivery failure is an error; when the chat is
  not allowlisted, then `transport_refused_unauthorized` is returned
  (R5).
- [ ] A6. Given fixture docx/xlsx/pdf files with text layers, when
  the office read tools extract, then the pinned fixture assertions
  pass; given a scanned PDF with no text layer, then
  `pdf_no_text_layer` is returned (R6).
- [ ] A7. Given the full spec implementation, when the catalog is
  audited, then every row dispatches to a real handler (ADR-0068), the
  schema budget test still passes, and no baselined module grew (R2).

## Out of scope

- Home pack tools (Home Assistant integration — spec-020).
- GitHub/SaaS/database integrations (spec-020).
- Local GPU image generation (external API providers first; local
  providers later must satisfy law 14).
- Video generation, image editing (MAY follow-ups).
- Pushing via `git_commit` or any remote mutation (devex write tools
  stop at the local commit).

## Open questions

- Image provider default: which provider catalog entry — resolved at
  implementation time via the existing provider-config mechanism,
  defaulting to an OpenAI-compatible images endpoint when configured.
- Whether office WRITE tools ship in v1 or v2 — the read/extract set
  is normative; writes are MAY per R6.
- Whether `speech_to_text` should also accept live mic input in v1 —
  default: artifact-based only.

## Links

- `crates/optimus-packs/src/catalog.rs` — the pack shells this spec
  fills.
- `crates/optimus-packs/src/invocation.rs` +
  `crates/optimus-kernel/src/tool_dispatch.rs` +
  `crates/optimus-kernel/tests/tool_coverage.rs` +
  `scripts/gates/check-tool-coverage.py` +
  `scripts/tests/test_engineering_memory.py` — the ceremony sites.
- `docs/decisions/0068-a-catalog-row-must-dispatch-or-not-exist.md`
  — the no-placeholder law.
- `docs/decisions/0070-an-outbound-send-is-a-durable-obligation.md`
  — social durability.
- `docs/decisions/0071-a-routing-address-is-not-a-session-identity.md`
  — routing-address foundation (Social pack).
- `specs/006-memory-skills-packs/spec.md` — the owning packs spec
  (R5/R6 amendment pattern).
- `specs/017-gateway-breadth/spec.md` — Social pack's dependency
  (adapter contract + allowlists).
- `specs/020-integrations-breadth/spec.md` — Home pack tools +
  external integrations (current; reciprocal dependency).
