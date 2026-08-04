---
doc_id: map-model-routing
doc_type: reference
plane: current
status: current
authority: canonical
summary: Confirmed current behaviour: Optimus implements a canonical typed route resolver for provider/model ownership, required capabilities, privacy, bounded cost, explicit fallback, readiness-based Auto selection, and durable decision...
reviewed_on: 2026-08-03
review_by: 2026-10-31
knowledge_type: model-routing-map
covers:
  - crates/optimus-kernel/src/lib.rs
  - crates/optimus-kernel/src/openai_compat.rs
  - crates/optimus-kernel/src/codex_oauth.rs
  - crates/optimus-kernel/src/routing.rs
  - apps/optimus-cli/src/main.rs
  - apps/optimus-cli/src/gateway_http.rs
  - crates/optimus-host/src/chat.rs
depends_on:
  - docs/decisions/0011-codex-oauth.md
  - docs/decisions/0013-provider-tool-protocol.md
validated_by:
  - crates/optimus-kernel/tests/codex_oauth.rs
  - crates/optimus-kernel/tests/openai_http.rs
  - crates/optimus-kernel/tests/kernel_turn.rs
  - crates/optimus-eval/tests/integrity_integration.rs
  - apps/optimus-cli/tests/chat_auto.rs
  - apps/optimus-ui/src/state/composerStore.test.ts
  - apps/optimus-desktop/ui/composer-auto.test.cjs
last_verified_commit: 09fddbc1b60a6b37f9f80680988ea5036a9b8eec
---

# Model-routing map

## Current conclusion

**Confirmed current behaviour:** Optimus implements a canonical typed route
resolver for provider/model ownership, required capabilities, privacy, bounded
cost, explicit fallback, readiness-based Auto selection, and durable decision
records. Runtime provider health, measurement-driven cost/latency, and
evaluation-driven selection are not implemented.

**Confirmed current behaviour:** the `vision_analyze` tool makes its own
bounded OpenAI-compatible sub-call (env-configured endpoint, public-egress
enforced, fixture-overridable) outside the turn's route decision, and stamps
the provider it actually used in its tool envelope. It is a tool effect, not
a second router.

## Provider adapters

| Provider | State | Configuration | Tool protocol | Fallback/retry |
|---|---|---|---|---|
| `offline` / `ScriptedModel` | Confirmed current behaviour | In-process scripted responses | Native `CompletionResponse` | No provider fallback; deterministic test/offline use. |
| OpenAI-compatible | Confirmed current behaviour | `OPTIMUS_API_BASE`, `OPTIMUS_MODEL`, `OPTIMUS_API_KEY`; 120-second default HTTP timeout | Strict chat-completions function calls with canonical descriptors | No provider/model fallback. Cooperative cancellation is checked before/after its synchronous request. |
| DeepSeek V4 | Confirmed current behaviour | `DEEPSEEK_API_BASE` (default `https://api.deepseek.com`), `DEEPSEEK_MODEL` (default `deepseek-v4-flash`), `DEEPSEEK_API_KEY` | Chat Completions tools; assistant `reasoning_content` is replayed on tool-follow-up requests | No provider/model fallback. The adapter is non-streaming and uses the shared synchronous HTTP boundary. |
| Codex OAuth | Confirmed current behaviour | Optimus `auth.json`, imported Hermes/Codex credentials, compiled endpoint/catalog | Strict Responses JSON/SSE function-call parsing | One adapter retry after HTTP failure; cancellable SSE reads poll at 250 ms after stream open. No provider fallback. |

## Codex model catalog

**Confirmed current behaviour:** the compiled Codex catalog exposes
`gpt-5.6-terra`, `gpt-5.6-luna`, and `gpt-5.6-sol`, with aliases `terra`,
`luna`, and `sol`. Unknown model IDs are sanitized to `gpt-5.6-terra`.
DeepSeek has a separate two-model catalog: `deepseek-v4-flash` and
`deepseek-v4-pro`. Reasoning effort is normalized per provider; `Auto` omits a
provider-specific override, while explicit UI budgets map to the provider's
supported values. Fast mode is a request flag, not a separate provider.

**Confirmed current behaviour:** OpenAI-compatible model names are environment
strings and are not resolved through the Codex catalog.

**Confirmed current behaviour:** kernel normalization produces reasoning effort
and fast-mode fields for every provider request. Codex maps explicit effort
into `reasoning.effort` and omits it for `Auto`; DeepSeek maps the common UI
levels to `low`, `high`, or `max` and omits `thinking` for `Auto`. DeepSeek's
tool loop carries response `reasoning_content` into the next assistant tool
message because its API requires that replay.

## Selection by surface

| Surface | Confirmed current behaviour |
|---|---|
| CLI chat/cron | Builds a `RouteRequest`; chat defaults to Auto while explicit cron routes remain exact. Legacy persisted `openai_compat` schedules normalize to the canonical OpenAI-compatible identity before execution. The canonical resolver rejects unknown provider/model identities and policy violations. |
| Desktop chat | Uses the same resolver and defaults to Auto; misspelled providers no longer enter a catch-all Codex branch. |
| Gateway HTTP | Uses the same resolver before constructing an adapter. |
| Cron tick | Uses the same resolver while retaining cron-owned scheduling/claim semantics. |

**Confirmed current behaviour:** all four surfaces share canonical provider
identity, model ownership, privacy, capability, and budget evaluation. Adapter
construction and transport remain surface-owned.

**Confirmed current behaviour:** `auto` is a requested selection, never a
`ProviderId` or `ModelId`. At turn start it selects the first connected
policy-eligible provider in the fixed order Codex OAuth, configured DeepSeek,
configured OpenAI-compatible, then offline. Codex credentials that are already expiring
without refresh capability are not considered connected. No model override
means the selected provider's canonical default. Decisions persist the
requested `auto` value and the concrete selected provider/model. New explicit
choices remain exact after selection; legacy unchosen Offline preference
residue is migrated to Auto. A post-selection provider failure does not trigger
a silent cross-provider retry.

## Cancellation boundary

**Confirmed current behaviour:** `ModelProvider` has a cooperative cancellable
streaming method and the kernel exposes a cancellable turn entry point. Providers
can observe a shared token during an active call. Codex bounds SSE read waits and
checks the token between reads/events.

**Unknown or unresolved behaviour:** synchronous `ureq` connection establishment
and request writes are not force-abortable. The OpenAI-compatible adapter therefore
cannot guarantee bounded mid-request cancellation beyond its HTTP timeout.

## Router contract and remaining gaps

**Confirmed current behaviour:** `RouteRequest` includes surface, requested
provider/model, required capabilities, privacy, optional maximum cost, and an
explicit fallback flag plus optional bounded telemetry policy. `RouteDecision` has stable identity, selected canonical
provider/model, fallback source, reasons, and timestamp; accepted decisions are
persisted in `routing.db`. Unknown identities, wrong model ownership, local-only
privacy violations, missing capabilities, and cost violations fail closed.

**Confirmed current behaviour:** telemetry observations must match an existing
route decision's provider, model, and optional trace. Fresh bounded aggregates
use checked integer success, latency, and cost arithmetic. Static policy runs
first; telemetry can filter/rank only already-approved candidates and records a
snapshot hash in the accepted decision reason.

The following remain **unknown or unresolved behaviour**:

- token counts and actual billing integration;
- data residency beyond local-versus-remote classification;
- context-window and structured-output capability metadata;
- per-model tool-use reliability and evaluation-driven selection;
- local-model adapters and GPU/CPU fallback;
- automatic fallback based on runtime provider failure.

## Planned direction

**Planned behaviour:** extend current bounded operational telemetry with
evaluation evidence without moving provider-specific wire parsing out of current adapters.

Future router extensions must preserve the current fail-closed contract and:

1. retain explicit provider retry versus cross-provider fallback;
2. bind policy/evaluation versions to each decision;
3. record measured cost, latency, and health inputs;
4. retain exact model parameters in the execution manifest;
5. use CPU-capable operation when local GPU adapters are unavailable.
