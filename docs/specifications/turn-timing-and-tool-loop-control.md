---
knowledge_type: specification
status: historical
issue: https://github.com/mustbearnold/Optimus-Agent/issues/4
covers:
  - crates/optimus-kernel/src/lib.rs
  - crates/optimus-kernel/src/execution.rs
  - crates/optimus-kernel/tests/kernel_turn.rs
  - apps/optimus-desktop/src/ipc/chat.rs
  - apps/optimus-desktop/ui/index.html
  - apps/optimus-desktop/ui/app.js
  - apps/optimus-desktop/ui/style.css
  - apps/optimus-desktop/e2e/**
---

# Turn timing and repeated tool-loop control

## Problem

**Confirmed pre-change behaviour:** a Desktop request for current AI news ended with
`max steps exceeded (8)` while the UI displayed `web_search ×52` and
`activate_pack ×2`. Review found that the task UI incremented on both running and
completion details, so the exact original call count cannot be recovered from the
screenshot. The kernel validated call IDs, advertised tools, and arguments before
effects, but did not bound an executable model-response batch or detect repeated
semantic calls with different provider call IDs. Execution manifests retained
hashes but no call or turn durations. Desktop exposed neither live nor terminal
timing.

## Accepted behaviour

1. `KernelConfig` owns a default execution budget of eight provider tool calls per
   model step. Overflow calls receive typed suppressed outcomes and force a
   synthesis-only next step. A separate hard ceiling of 64 rejects pathological
   responses before sibling effects execute.
2. Within one turn, an exact normalized signature for read-only evidence tools may
   execute once. Repeated calls receive a typed non-retryable suppressed outcome,
   do not execute the effector again, and force the next model request to advertise
   no tools with a synthesis-only instruction. This applies to `web_search`,
   `memory_recall`, and `skill_resolve`; mutable or context-sensitive tools such as
   file reads, browser snapshots/navigation, and durable effects are never
   semantic-deduplicated.
3. Monotonic integer-millisecond events cover turn start/finish, model start/finish,
   first provider response, and tool start/finish. Every terminal turn event states
   `succeeded`, `failed`, or `cancelled`. Durations never participate in replay or
   evaluation hashes.
4. Execution manifests durably retain terminal turn duration; successful model and
   tool-call records retain durations. An ordered timing-event table preserves
   success, failure, cancellation, and suppressed-call evidence and supports a
   public timing summary.
5. Desktop stream JSON preserves typed timing fields. The chat UI shows a compact
   live turn timer and session elapsed timer, terminal total/first-response/model/
   tool timing pills, and task/tool durations. It uses `performance.now()` only for
   live repaint; canonical terminal values come from kernel events/results.
6. Timing presentation remains compact, zero-radius, keyboard-neutral, and safe at
   minimum window size. No titlebar action clutter is added.

## Non-goals

- Wall-clock timestamps as deterministic evaluation inputs.
- Provider billing/token telemetry, OpenTelemetry export, SLOs, or universal traces.
- Automatic quality claims derived only from lower latency.
- Deduplication of mutating, approval-sensitive, or browser-navigation effects.

## TDD and acceptance evidence

- RED: the focused kernel contract failed because timing types/summary and
  synthesis-only duplicate suppression did not exist; the Desktop contract failed
  because session/turn timer elements did not exist.
- GREEN: deterministic kernel tests cover success, failure, cancellation, exact
  canonical `web_search` signatures, eight-call overflow suppression, the 64-call
  hard ceiling, one real evidence execution, and durable timing summaries.
- GREEN: Desktop unit/Playwright contracts cover typed timing JSON, live clocks,
  terminal timing pills, exact call-ID task counts, and per-tool duration display.
- Visual correction: the first 420×320 geometry run found footer timer overflow;
  path/branch flex truncation fixed it and the unchanged overflow assertion passed.
- Native delivery: the release installer relaunched the executable from
  `%LOCALAPPDATA%\\Programs\\OptimusAgent`; cua-driver capture was unavailable due
  to an ended driver session, so rendered visual evidence used the supported HTTP
  harness against the same embedded document.
