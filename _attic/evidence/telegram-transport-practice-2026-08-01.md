---
doc_id: evidence-telegram-transport-practice-2026-08-01
doc_type: evidence
plane: evidence
status: historical
authority: record
summary: Dated check of the Telegram Bot API contract and of the HTTP client this workspace would depend on to speak it, made before the first live messaging transport, with the reason the new dependency is pinned to ureq 2 rather than the current 3.x.
reviewed_on: 2026-08-01
review_by: never
---

# Telegram transport practice check — 2026-08-01

AGENTS.md development workflow step 6 requires a dated, search-backed check
before a *new* test layer is built and before anything new is depended on. This
work does both: it adds `ureq` to `crates/optimus-ops`, and it adds a test layer
of a kind this workspace did not have — a loopback HTTP server the transport
talks to as if it were Telegram. This is that record for task
`b-cap-02-live-telegram-transport`. It is bound to its date; a later pass should
re-check rather than inherit it.

## What was checked

The Bot API contract the transport encodes, and the maturity of the HTTP client
used to reach it.

- [Telegram Bot API](https://core.telegram.org/bots/api) — request shape,
  response envelope, `getUpdates`, `sendMessage`, and `ResponseParameters`
- [Telegram Bot FAQ](https://core.telegram.org/bots/faq) — polling cadence
- [`ureq` on crates.io](https://crates.io/crates/ureq) — 3.3.0 latest
  (March 2026); resolved 2.12.1 for first-party use in this workspace

What the API documents, and where each fact is now encoded:

| Fact | Encoded at |
|---|---|
| Methods are `https://api.telegram.org/bot<token>/METHOD_NAME` — the credential is a path segment, not a header or a body field | `telegram/live.rs` `DEFAULT_API_BASE`, and `the_credential_travels_in_the_path_and_never_in_the_body` |
| Every method answers `{ok, result}` or `{ok:false, error_code, description}` — HTTP status alone is not the outcome | `read_envelope`, and `a_refusal_carries_telegrams_own_description` |
| A rate limit arrives as `retry_after` inside `parameters`, not at the top level | `describe_error`'s `/parameters/retry_after` pointer |
| `getUpdates` takes `offset`, `limit`, `timeout`, `allowed_updates`; `timeout: 0` is short polling, and the docs ask production callers to hold the connection instead | `ALLOWED_UPDATES`, and `a_poll_asks_telegram_to_hold_the_connection_rather_than_spin`; the CLI defaults to a 30s hold |
| An update is only confirmed once an `offset` past it is requested — unconfirmed updates are redelivered | the durable cursor in `apps/optimus-cli/src/telegram_cmd.rs`, which replays rather than skips when unreadable |
| `sendMessage` caps one message at 4096 UTF-16 code units | `MAX_MESSAGE_UNITS`, and `a_reply_past_the_cap_arrives_whole_across_several_messages` |

The last two are the ones a from-memory implementation gets wrong. A cursor held
only in memory looks correct for the life of one process and answers every
outstanding message a second time on restart; and a cap counted in `char`s
rather than UTF-16 code units passes every ASCII test and truncates the first
reply containing an emoji.

## Where this suite sits against that bar

The pre-existing `MockTelegramTransport` is a fixture that implements the
adapter's own trait. It proves the claim→turn→settle spine and nothing about
Telegram, because it *is* the contract it would be checked against — it stays
green whether or not the code agrees with the platform. Mock-only coverage of an
external protocol is the self-serving green the north-star criteria ban.

`crates/optimus-ops/tests/telegram_bot_api_contracts.rs` is therefore a new
layer rather than new cases: a real HTTP listener on loopback, scripted with
responses Telegram could actually send, asserting on the requests Telegram would
actually have received. It uses `std::net::TcpListener` and the crate's existing
`serde_json`, adding no dependency of its own — consistent with the house style
recorded in [the test-layer check of the same
date](rust-test-layer-practice-2026-08-01.md), where `rstest` and `insta` were
found to appear nowhere in this workspace and adopting either was judged a
workspace-wide decision belonging to its own change.

## Why the new dependency is pinned to `ureq` 2

`ureq` 3.3.0 is current. The pin is deliberate and the honest reason is narrower
than "3.x is unproven":

`Cargo.lock` already resolves **both** 2.12.1 and 3.3.0. The 3.x copy arrives
transitively through `auto_generate_cdp`, so the tree carries it either way and
the pin buys no reduction in compiled surface. What it buys is that every
*first-party* HTTP call stays on one client API and one TLS configuration.
Before this change that was six modules — `browser.rs`, `web_search.rs`,
`codex_device_login.rs`, `codex_oauth.rs`, `openai_compat.rs`, `vision.rs` — all
on 2.12.1 through `optimus-kernel`, plus `apps/optimus-cli`. Introducing the
first 3.x call site here would fork the workspace's HTTP idiom on the authority
of a messaging task, and would leave the next reader unable to tell which
convention is current. A 2→3 migration touches all seven and is its own change.

Recorded so the next pass does not mistake the pin for an unexamined default:
the block at `crates/optimus-ops/Cargo.toml` points back here for exactly that
reason.

## Result

The live transport is checked in three independent places, each of which fails
for a different reason:

| File | What it pins |
|---|---|
| `crates/optimus-ops/tests/telegram_bot_api_contracts.rs` | What goes on the wire and what is concluded from what comes back — 8 tests against a loopback fake |
| `crates/optimus-ops/tests/channel_seam_contracts.rs` | That the adapter's translation is lossless in both directions, and that a polled message is routed rather than pinned to the scripted model |
| `crates/optimus-host/tests/gateway_address_contracts.rs` | That a routing address is never parsed as a session id, and never leaks back out as a reply target (ADR-0071) |

None of the three can be satisfied by agreeing with the mock.
