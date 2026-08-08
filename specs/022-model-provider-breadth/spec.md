---
doc_id: spec-022-model-provider-breadth
doc_type: reference
plane: work
status: current
authority: canonical
summary: Provider breadth for Optimus — adding an Anthropic Messages API client (streaming + extended thinking), OpenRouter and Nous Portal entries on the existing OpenAI-compatible client, per-provider reasoning-level mapping with explicit degrade (never silent), truthful ordered failover with per-class diagnostics, and doctor-reported provider config state with the shared secret discipline.
reviewed_on: 2026-08-08
review_by: 2026-11-08
knowledge_type: specification
covers:
  - crates/optimus-kernel/src/openai_compat.rs
  - crates/optimus-kernel/src/model_contract.rs
  - crates/optimus-kernel/src/model_call.rs
  - crates/optimus-host/src/extensibility.rs
  - apps/optimus-cli/src/main.rs
depends_on:
  - specs/018-deployment-ops/spec.md
  - specs/020-integrations-breadth/spec.md
---

# Spec-022: Model/provider breadth — catalog, reasoning levels, truthful failover

Status: current
Owner: optimus-agent-development (prompt-only owner)

## Revision table

| Round | Verdict | Findings | Fixes |
|---|---|---|---|
| 1 | REJECTED | B1: current state omitted DeepSeek — a live 4th catalog entry (routing.rs ProviderId::Deepseek, DeepseekModel in openai_compat.rs) that is exactly the R2 precedent; B2: "no reasoning-level mapping" stale — normalize_thinking_level + apply_fast_mode + R8 caps exist kernel-hardcoded and silent; R3 framed greenfield; 1 nit (ModelCapability is a flag enum, not a mapping home) | B1: DeepSeek added to Purpose + current state; B2: R3 rewritten as a RELOCATION (mapping into ProviderDescriptor data, behavior-preserving, honest-degrade diagnostics added); nit applied (round 2) |
| 2 | APPROVED | 2 non-blocking nits (Anthropic 529 vs 429 wire code; doctor.rs link missing) | Applied 2026-08-08 (529 pinned in fixtures; doctor.rs added to Links) |

## Purpose

Optimus's provider surface is narrow: an OpenAI-compatible client
(`crates/optimus-kernel/src/openai_compat.rs`), a Codex OAuth
Responses provider, a DeepSeek provider (a live entry on the
OpenAI-compatible client — `ProviderId::Deepseek` in routing.rs,
`DeepseekModel` in openai_compat.rs), and an offline provider; the
catalog has four entries and the CLI's `--provider` accepts
auto/offline/openai/codex. There is no Anthropic Messages API client,
no OpenRouter, no Nous Portal, and the per-provider reasoning-level
normalization that exists (`normalize_thinking_level` in model_call.rs)
is kernel-hardcoded and SILENT — no honest degrade. Hermes, the parity
target, routes across many providers with configurable reasoning
effort.

This spec widens the catalog without weakening the mechanism: an
Anthropic client with streaming + extended thinking, OpenRouter and
Nous Portal as entries on the existing OpenAI-compatible client, a
per-provider reasoning-level contract where unsupported levels degrade
with a named diagnostic (never silently), ordered failover that stays
truthful (a permanent auth failure is never masked as a generic
failure), and doctor-reported provider state under the shared secret
discipline (spec-018 R6 / spec-020 R6).

## Current state (Confirmed behaviour)

- Providers today: OpenAI-compatible (`openai_compat.rs`), Codex
  OAuth (`codex_oauth.rs` + `codex_responses.rs` +
  `codex_device_login.rs`), DeepSeek (a live catalog entry on the
  OpenAI-compatible client: `ProviderId::Deepseek` in routing.rs with
  a `Reasoning` capability, `DeepseekModel` in openai_compat.rs, key
  in the home key store), and offline — four catalog entries; the CLI
  `--provider` choices are auto/offline/openai/codex; the P27 catalog
  status test asserts ≥3 entries (Confirmed: source).
- The provider catalog + ordered failover + route preview exist as P27
  IPC (`providers_catalog`, `providers_route_preview`,
  `resolve_route`, `ModelCapability`) (Confirmed: `extensibility.rs`).
- Thinking blocks are already a first-class kernel concept — "Thinking
  blocks separate from assistant text" is a shipped parity slice
  (scorecard) and the chat CLI exposes `--thinking
  off|minimal|low|medium|high|xhigh|max|ultra` (Confirmed: scorecard,
  `main.rs`).
- No Anthropic, OpenRouter, or Nous provider file exists in
  `crates/optimus-kernel/src/` (Confirmed: source tree).
- Reasoning-level normalization ALREADY EXISTS in the kernel:
  `normalize_thinking_level` in `model_call.rs` maps UI levels to
  provider strings (an `off` sentinel for DeepSeek, `ultra`→`max`
  for Codex OAuth), with `apply_fast_mode` and the R8
  `cap_effort_for_later_steps` (effort capped at `low` after the
  first step) — all silent; there is no named-diagnostic degrade
  path (Confirmed: source).
- Secrets discipline shared with spec-018/020: provider keys live in
  the config home with mode 0600; doctor reports issues with named
  diagnostics (Confirmed: doctor.rs + spec-018 R6).

## Requirements

### R1. Anthropic Messages API client

- Optimus MUST ship an `anthropic` provider implementing the Messages
  API: non-streaming and SSE streaming requests, tool-calling
  round-trips (the kernel's existing ToolCall contract), and
  extended-thinking blocks mapped to the kernel's existing thinking
  representation (MUST).
- The client MUST map Anthropic stop reasons, usage, and error codes
  to the kernel's canonical `CompletionResponse`/error contract with
  named diagnostics for 401/403/429/529 (`anthropic_unauthorized`
  / `anthropic_forbidden` / `anthropic_rate_limited` /
  `anthropic_overloaded` — HTTP 529 is Anthropic's overloaded class,
  distinct from 429) and the R1 conformance fixtures MUST pin the
  wire-code → diagnostic mapping (MUST).
- The provider MUST be exercised in CI against a mock Messages-API
  server (the openai_http.rs mock pattern) covering: plain turn,
  tool-call round-trip, streaming chunk sequence + stop, and error
  mapping (MUST).
- Config: API key in the config home mode 0600; model id + base URL
  configurable; base URL defaulting to the official endpoint (MUST).

### R2. OpenRouter and Nous Portal entries

- `openrouter` and `nous` MUST be added as catalog entries on the
  existing OpenAI-compatible client, with base URLs from config
  (MUST).
- Model routing for these entries MUST pass through the existing
  `resolve_route` mechanism so failover ordering applies to them like
  any other provider (MUST).
- Their keys MUST obey the shared secret discipline (config home,
  0600, never in output/diagnostics) (MUST).
- Each entry MUST have a CI mock conformance test (same pattern as
  R1) proving a request round-trips with the configured base URL and
  auth header shape (MUST).

### R3. Per-provider reasoning-level contract

- The CLI's thinking levels (off…ultra) MUST map per provider through
  a declared per-provider surface. The existing kernel-hardcoded
  mapping (`normalize_thinking_level` in model_call.rs) MUST be
  relocated into provider data: each `ProviderDescriptor` catalog row
  declares the levels it supports and their provider-native
  spellings (Anthropic extended-thinking levels, OpenAI-compatible
  `reasoning_effort` where the backend supports it, Codex and DeepSeek
  their existing normalized surfaces) — `ModelCapability` stays a
  capability-flag enum; it is not the mapping home (MUST).
- The relocation MUST preserve the existing behavior exactly:
  DeepSeek's `off` sentinel, Codex's `ultra`→`max`, `apply_fast_mode`,
  and the R8 `cap_effort_for_later_steps` interaction (a
  provider-specific high-effort label must not defeat the R8 cap)
  (MUST).
- Requesting a level a provider does not support MUST either map to
  the nearest supported level and say so in the response, or fail
  with the named diagnostic `provider_reasoning_unsupported` — a
  silent ignore is a defect (MUST).

### R4. Truthful ordered failover

- Failover MUST remain ordered per the existing route mechanism, and
  each hop MUST be recorded in the observability plane (route
  decision + reason + outcome) (MUST; law 11).
- A permanent auth failure (401/403) on the primary MUST be surfaced
  with its provider-specific diagnostic and MUST be distinguishable
  from network/overload failures in the route record (MUST).
- When the LAST configured provider fails permanently, the terminal
  diagnostic MUST name the provider and the class — never a generic
  "model unavailable" that hides which provider refused and why
  (MUST; truthfulness, ADR-0081 family).
- Route preview (`providers_route_preview`) MUST stay the single
  source of truth for what failover WOULD do, and MUST be extended to
  show the reasoning-level mapping for the resolved provider (MUST).

### R5. Provider state in doctor

- `optimus doctor` MUST report per-provider configuration state
  (configured / unconfigured / key-permission-issue) without
  exposing keys, and a key file with mode 0644 or wider MUST be the
  named issue `provider_key_permissions_too_open` (exit 1) (MUST;
  consistent with spec-018 R6).

## Acceptance criteria

- [ ] A1. Given the mock Anthropic Messages server, when a plain
  turn, a tool-call round-trip, and a streaming request are driven
  through the `anthropic` provider, then responses map to the
  canonical contract, the SSE stream terminates, and 429 maps to
  `anthropic_rate_limited` (R1).
- [ ] A2. Given configured OpenRouter and Nous entries, when
  `providers_route_preview` resolves them and a mock round-trip runs,
  then the base URL and auth header shape from config are used and
  failover ordering includes them (R2).
- [ ] A3. Given a provider that does not support a requested thinking
  level, when the CLI requests it, then the nearest supported level is
  used and reported, or `provider_reasoning_unsupported` is returned —
  never a silent ignore (R3).
- [ ] A4. Given a primary provider whose key is invalid, when a chat
  routes, then failover hops are recorded with the auth class, and a
  single-provider config surfaces the auth diagnostic naming the
  provider — never a generic failure (R4).
- [ ] A5. Given a provider key file at 0644, when `optimus doctor`
  runs, then it exits 1 with `provider_key_permissions_too_open`;
  with 0600 the issue clears (R5).
- [ ] A6. Given the full implementation, when the provider mock suites
  run in `just verify`, then R1/R2 conformance and the route-preview
  tests pass with zero skips (R1–R4).

## Out of scope

- Model training / fine-tuning surfaces.
- Local model serving (llama.cpp-style) as a provider — MAY later;
  the OpenAI-compatible client already makes a local server a
  one-line base-URL config.
- Provider billing/usage dashboards beyond the existing model_usage
  accounting.
- Changing the development-plane provider routing for agents that
  develop THIS repo (constitution principle 7 — instruction-plane
  firewall).

## Open questions

- Whether Anthropic's beta headers (extended thinking) require a
  versioned header pin — the conformance fixtures MUST pin the API
  version the client targets.
- Default failover order when auto is selected with multiple
  providers — resolved by the existing auto-selection rule; this spec
  only requires the decision to be observable (route preview).

## Links

- `crates/optimus-kernel/src/openai_compat.rs` — the client the
  OpenRouter/Nous entries extend.
- `crates/optimus-kernel/src/codex_oauth.rs` +
  `crates/optimus-kernel/src/codex_responses.rs` — the existing
  second-provider pattern R1 mirrors.
- `crates/optimus-host/src/extensibility.rs` — catalog/failover/route
  preview (R2, R4).
- `apps/optimus-cli/src/main.rs` — `--provider`/`--thinking` surfaces
  R3 extends.
- `specs/018-deployment-ops/spec.md` + `specs/020-integrations-breadth/spec.md`
  — the shared secret + doctor discipline (R5).
- `apps/optimus-cli/src/doctor.rs` — the doctor issue contract R5
  builds on.
- `docs/architecture/sota-scorecard.md` — implemented parity slices
  (provider client, Codex OAuth, failover) this spec widens.
