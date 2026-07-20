---
knowledge_type: model-routing-map
status: current
covers:
  - crates/optimus-kernel/src/lib.rs
  - crates/optimus-kernel/src/openai_compat.rs
  - crates/optimus-kernel/src/codex_oauth.rs
  - apps/optimus-cli/src/main.rs
  - apps/optimus-cli/src/gateway_http.rs
  - apps/optimus-desktop/src/ipc/chat.rs
depends_on:
  - docs/decisions/0011-codex-oauth.md
  - docs/decisions/0013-provider-tool-protocol.md
validated_by:
  - crates/optimus-kernel/tests/codex_oauth.rs
  - crates/optimus-kernel/tests/kernel_turn.rs
  - apps/optimus-cli/tests/**
  - apps/optimus-desktop/e2e/**
last_verified_commit: null
---

# Model-routing map

## Current conclusion

**Confirmed current behaviour:** Optimus currently implements provider adapters
and surface-level selection. It does not implement a capability-, cost-,
privacy-, or evaluation-driven model router.

## Provider adapters

| Provider | State | Configuration | Tool protocol | Fallback/retry |
|---|---|---|---|---|
| `offline` / `ScriptedModel` | Confirmed current behaviour | In-process scripted responses | Native `CompletionResponse` | No provider fallback; deterministic test/offline use. |
| OpenAI-compatible | Confirmed current behaviour | `OPTIMUS_API_BASE`, `OPTIMUS_MODEL`, `OPTIMUS_API_KEY`; 120-second default HTTP timeout | Strict chat-completions function calls with canonical descriptors | No provider/model fallback. Cooperative cancellation is checked before/after its synchronous request. |
| Codex OAuth | Confirmed current behaviour | Optimus `auth.json`, imported Hermes/Codex credentials, compiled endpoint/catalog | Strict Responses JSON/SSE function-call parsing | One adapter retry after HTTP failure; cancellable SSE reads poll at 250 ms after stream open. No provider fallback. |

## Codex model catalog

**Confirmed current behaviour:** the compiled catalog exposes `gpt-5.6-terra`,
`gpt-5.6-luna`, and `gpt-5.6-sol`, with aliases `terra`, `luna`, and `sol`.
Unknown model IDs are sanitized to `gpt-5.6-terra`. Reasoning effort is
normalized per catalog entry; fast mode is a request flag, not a separate
provider.

**Confirmed current behaviour:** OpenAI-compatible model names are environment
strings and are not resolved through the Codex catalog.

**Confirmed current behaviour:** kernel normalization produces reasoning effort
and fast-mode fields for every provider request. Codex maps those controls into
its request; the OpenAI-compatible request mapper currently omits both.

## Selection by surface

| Surface | Confirmed current behaviour |
|---|---|
| CLI chat/cron | Explicit match for `offline`, `openai`/`openai-compat`, and `codex`; unknown values fail. |
| Desktop chat | `offline` and OpenAI aliases are explicit; every other provider string enters the Codex branch. |
| Gateway HTTP | Supports only `offline` and `codex`; unknown values fail. |
| Cron tick | Supports its own explicit subset in CLI/desktop handlers; it is not a shared routing service. |

**Known architectural debt:** provider interpretation differs by surface.
Desktop's catch-all Codex branch can turn a misspelled provider into a networked
Codex request while CLI/gateway reject it.

## Cancellation boundary

**Confirmed current behaviour:** `ModelProvider` has a cooperative cancellable
streaming method and the kernel exposes a cancellable turn entry point. Providers
can observe a shared token during an active call. Codex bounds SSE read waits and
checks the token between reads/events.

**Unknown or unresolved behaviour:** synchronous `ureq` connection establishment
and request writes are not force-abortable. The OpenAI-compatible adapter therefore
cannot guarantee bounded mid-request cancellation beyond its HTTP timeout.

## Missing router contract

The following are **unknown or unresolved behaviour** because no shared router
or registry owns them:

- capability requests such as coding, visual understanding, extraction, or
  local-private processing;
- provider/model health and fallback order;
- cost, latency, token, and budget policy;
- privacy/data-residency restrictions;
- context-window and structured-output capability metadata;
- per-model tool-use reliability and evaluation evidence;
- local-model adapters;
- routing decision traces and stable decision IDs;
- automatic fallback criteria and loop bounds.

## Planned direction

**Planned behaviour:** agents request capabilities, and a versioned router
resolves them using policy plus evaluation evidence. Routing must be centralized
without moving provider-specific wire parsing out of the current adapters.

A first router contract should:

1. reject unknown provider/model identities;
2. record requested capability, selected provider/model, policy version, and
   reason;
3. bound retries/fallbacks;
4. separate provider retry from cross-provider fallback;
5. enforce privacy and budget before network calls;
6. retain the exact model parameters in the execution trace;
7. use CPU-capable operation when local GPU adapters are unavailable.
