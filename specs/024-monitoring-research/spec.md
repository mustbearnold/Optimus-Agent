---
doc_id: spec-024-monitoring-research
doc_type: reference
plane: work
status: current
authority: canonical
summary: Monitoring and research verticals for Optimus — script jobs (no-LLM ticks with silent-on-empty and error alerts), URL change monitors (versioned extract, stable hash baseline, unified-diff delivery), and research monitors (scheduled query briefings with provenance and dedupe) on the existing claim-lease cron engine, with failure honesty (bounded retry, archive-not-carry per ADR-0073) and gateway-outbox delivery under spec-017's durable-obligation contract.
reviewed_on: 2026-08-08
review_by: 2026-11-08
knowledge_type: specification
covers:
  - crates/optimus-ops/src/cron.rs
  - crates/optimus-ops/src/gateway.rs
depends_on:
  - docs/decisions/0073-an-unreachable-vertical-is-archived-not-carried.md
  - specs/007-ops/spec.md
  - specs/017-gateway-breadth/spec.md
---

# Spec-024: Monitoring & research verticals — script jobs, URL and research monitors

Status: current
Owner: optimus-agent-development (prompt-only owner)

## Revision table

| Round | Verdict | Findings | Fixes |
|---|---|---|---|
| 1 | REJECTED | B1: spec-007 R1 amendment referenced but never made normative (no revised-R1 clause, new outcome classes not folded into "exactly one terminal outcome"); 5 nits (archive store rep, dedupe hash location, A6 outcome rows, retry-counter conflation, first-tick baseline) | R0 added (amended R1 clause verbatim + same-commit landing + outcome-class enumeration); archive = distinct CronStore status; last_delivered_hash column; retry counters declared independent; baseline silent + byte-identical hash pinned (round 2) |
| 2 | REJECTED | B1: R0 five classes vs R5 six classes contradictory on same observability field; B2: same-commit/semantics-preservation MUSTs uncriteria'd; 6 nits (sandbox, redirect-hop + baseline, zero-source, cron exposure, retry-once, R6 outbox assertions) | Single enumeration unified (5 terminal classes + kind refinements); A7 (same-commit diff + regression-free suites); A8-A11 added; A1-A6 name outcome classes (round 3) |
| 3 | REJECTED | B1: delivered ("content OR alert") overlaps failed (exactly-one violation); B2: unpinned diagnostics; B3: no-agent-turn MUSTs uncriteria'd; B4: hash durability uncriteria'd; B5: timeout variant untested; 5 nits (A6 per-kind ambiguity + script archive, retry independence, ONE-turn assertion, archived-vs-paused, ADR-0071 in A11) | delivered = content-only; diagnostics pinned (script_sandbox_denied, monitor_redirect_chain_too_long, monitor_no_sources); A12 (zero model turns + fixture counter); A13 (restart hash persistence); A2 timeout variant + MAX_ATTEMPTS independence; A6 per-kind classes; R4 archive covers script jobs; A4 ONE turn; A10 archived-vs-paused; A11 ADR-0071 (round 4) |
| 4 | REJECTED | B1: R5 exactly-one MUST uncriteria'd (zero/two-class rows still pass); 4 nits (unpinned delivery-failure diagnostic, robots-politeness unmeasurable, MAX_SEND_ATTEMPTS independence untested, row-ordering/receipt fields uncriteria'd) | A6 exclusivity assertion (zero/two-class rows fail); outbound_send_failed pinned; politeness pinned (1 req/origin/tick, no prefetch, single bounded GET); A2 covers MAX_SEND_ATTEMPTS=1; A10 asserts ordering + receipt column (round 5) |
| 5 | APPROVED | 3 non-blocking nits (politeness pin untested, law-11 mis-citation on exactly-one, kind-surface config fields unexercised) | Applied 2026-08-08 (A3 GET-count assertion; law 10/11 split; A1 args+cwd, A3 cap+headers+target, A4 prompt/scope/cadence) |

## Purpose

Optimus's cron engine (spec-007) is a durable claim-lease store but
every tick is an agent turn: there is no no-LLM script job, no URL
change monitoring, no scheduled research briefing, and no watchdog
semantics. Hermes, the parity target, ships script crons with
silent-on-empty watchdog semantics, monitor scripts with change
detection, and recurring research briefings.

This spec adds the monitoring/research verticals as job KINDS on the
existing cron engine — script jobs (run, deliver stdout verbatim or
stay silent), URL change monitors (hash baseline + diff delivery), and
research monitors (scheduled query briefings with provenance and
dedupe) — with failure honesty (bounded retry, error alerts, and
archive-not-carry per ADR-0073) and delivery through the spec-017
gateway outbox under the durable-obligation contract (ADR-0070). The
claim-lease engine, exactly-one-terminal-outcome, and observability
law are unchanged.

## Current state (Confirmed behaviour)

- `crates/optimus-ops/src/cron.rs` `CronStore` provides a durable
  claim-lease cron engine: add/list/due/claim_due/renew_claim/
  complete_claim/release_claim/cancel_running/attempt_status/
  set_next_run/set_enabled/remove (Confirmed: source).
- Every tick today is an LLM agent turn; there is no script-only job
  kind (Confirmed: spec-007 R1 + cron surface).
- The gateway is a durable local delivery authority with inbox/outbox,
  enqueue, and ack'd delivery (spec-007 R2; `crates/optimus-ops/src/gateway.rs`
  claim-lease engine) — the delivery path monitor outputs will use
  (Confirmed: source).
- ADR-0073 ("an unreachable vertical is archived, not carried") is
  the archive-not-carry law (Confirmed: ADR).
- Bounded, versioned web extraction with provenance exists (P23:
  "Web search versioned extract + provenance URL" is a shipped parity
  slice) — the URL monitor's fetch path (Confirmed: scorecard).
- ADR-0071: a routing address is not a session identity — monitor
  deliveries target routing addresses (Confirmed: ADR).

## Requirements

### R0. spec-007 R1 amendment (normative)

- This spec AMENDS spec-007 R1: the job-lifecycle terminal-outcome
  contract MUST gain the monitor/script job kinds and their outcome
  classes. The amended R1 clause reads: "A job MUST have exactly one
  terminal outcome per tick. The outcome classes are: `delivered`
  (CONTENT enqueued to the outbox — kind-refined to
  `changed` for URL monitors and `briefing` for research monitors),
  `silent` (no delivery — kind-refined to `unchanged` or `deduped`;
  empty-stdout scripts are `silent`), `failed` (ERROR ALERT
  enqueued), `archived` (job archived per ADR-0073 and no longer
  scheduled), or `skipped` (no tick ran). A tick MUST record exactly
  one of these classes in the observability plane (spec-024 R5)"
  (MUST).
- The spec-007 R1 revision MUST land in the same commit as the
  spec-024 implementation and MUST NOT change lifecycle semantics for
  existing job kinds (claim-lease, retry, delivery) (MUST).
- A1/A2/A3/A4/A5 MUST each assert the outcome class their scenario
  produces, so the R1 enumeration is exercised (MUST).

### R1. Script jobs (no-LLM ticks)

- `cron_add` MUST accept a `script` job kind: command + args + cwd +
  timeout, with NO agent turn on the tick path (MUST).
- A script tick with EMPTY stdout MUST be SILENT: no delivery, no
  agent run, recorded as a silent tick (MUST; the watchdog pattern —
  a quiet job is a healthy job).
- A script tick with non-empty stdout MUST deliver stdout verbatim as
  an outbound message through the gateway outbox (R6) (MUST).
- A non-zero exit or timeout MUST send an error alert (R4) even when
  stdout is empty (MUST).
- Script jobs MUST be sandboxed like any tool execution (no ambient
  home access beyond the configured cwd; bounded runtime); a sandbox
  denial MUST record `failed` with the named diagnostic
  `script_sandbox_denied` (MUST).

### R2. URL change monitors

- `cron_add` MUST accept a `monitor-url` job kind: URL + fetch policy
  (bounded GET, cap, headers) + delivery target (MUST).
- Each tick MUST fetch the URL via the bounded/versioned extract
  mechanism, canonicalize the content (normalized body), and compute a
  stable hash; the baseline hash MUST be stored durably (MUST).
- The FIRST tick establishes the baseline: it fetches, stores the
  hash, and delivers nothing (baseline is silent — a monitor is a
  change detector, not a delivery service). Two ticks are "identical"
  when their canonicalized content hashes match byte-for-byte (MUST).
- A changed tick MUST deliver a unified diff (old vs new) plus the
  provenance URL; an unchanged tick MUST be silent (MUST).
- Fetch failures MUST follow R4 (bounded retry then error alert;
  archive-not-carry) (MUST).
- The monitor MUST NOT follow unbounded redirect chains and MUST
  respect robots-appropriate politeness — pinned as: at most ONE
  request per origin per tick, no background prefetch, a single
  bounded GET (bounded single redirect hop) (MUST); a chain longer
  than one hop MUST record `failed` with the named diagnostic
  `monitor_redirect_chain_too_long` (MUST).

### R3. Research monitors

- `cron_add` MUST accept a `monitor-query` job kind: a fixed prompt +
  source scope + cadence; each tick runs ONE agent turn with the
  bounded research toolset (web extract/search) (MUST).
- The briefing MUST include provenance URLs for every factual claim
  (MUST; the grounded-citations discipline).
- Dedupe: if a tick's briefing content hashes identically to the last
  DELIVERED briefing, the tick MUST deliver nothing (silent) and
  record the dedupe; the last-DELIVERED briefing hash MUST be stored
  on the job's own CronStore row (`last_delivered_hash`), not derived
  from history (MUST).
- A tick MUST NOT deliver a briefing that failed to gather any
  source (R4: named diagnostic `monitor_no_sources` instead) (MUST).

### R4. Failure honesty

- Transient failures (network, timeout) MUST retry ONCE within the
  tick's lease, then deliver an error alert naming the monitor and
  the failure class; a failure MUST never be silently dropped. The
  per-tick retry is the tick's own counter — INDEPENDENT of gateway
  MAX_ATTEMPTS (gateway.rs) and outbound MAX_SEND_ATTEMPTS
  (outbound_ledger.rs); implementers MUST NOT conflate them (MUST).
- N consecutive failures (N config, default 3) MUST archive the
  monitor (or script job) with the named event
  `monitor_archived_unreachable` and stop scheduling it; archiving
  MUST persist a distinct `archived` status on the CronStore row (a
  status flag separate from `set_enabled=false`), so `cron_list`
  distinguishes archived from paused (MUST; ADR-0073 — archived, not
  carried).
- Error alerts MUST be delivered through the same outbox path as
  content (R6) (MUST).

### R5. Observability

- Every tick MUST record an ordered, durable event row: job id, kind,
  terminal outcome class (delivered / silent / failed / archived /
  skipped), kind-level refinement (changed / unchanged / briefing /
  deduped where applicable), and delivery receipt (MUST; exactly-one
  terminal outcome = AGENTS.md law 10; ordered durable row = law 11);
  a tick row MUST carry EXACTLY ONE terminal outcome class (MUST).
- `cron_history` and `cron_list` MUST expose kind + last outcome per
  job (MUST).

### R6. Delivery through the gateway outbox

- All monitor deliveries MUST enqueue through the gateway outbox as
  durable obligations (ADR-0070) targeted at a routing address
  (ADR-0071) (MUST).
- A delivery failure MUST surface per spec-017 R8: marked failed with
  the named diagnostic, never reported as success (MUST).

## Acceptance criteria

- [ ] A1. Given a script job whose command prints nothing and exits 0,
  when the tick runs, then the tick is silent (outcome class
  `silent`, no delivery) and recorded; given a script that prints
  text, then the stdout is delivered verbatim (outcome class
  `delivered`); with configured args and cwd, the tick runs the
  command with exactly those args in that cwd (R1).
- [ ] A2. Given a script job that exits 3 with empty stdout, when the
  tick runs, then it retries exactly once within the lease and an
  error alert is delivered (outcome class `failed`) naming the exit
  (R1, R4); given a script job that exceeds the bounded runtime, then
  an error alert is delivered (outcome class `failed`) naming the
  timeout; the in-lease retry count is independent of a low gateway
  MAX_ATTEMPTS config and of a low outbound MAX_SEND_ATTEMPTS
  (MAX_ATTEMPTS=1 and MAX_SEND_ATTEMPTS=1 configs do not reduce it)
  (R1, R4).
- [ ] A3. Given a URL monitor whose content changes between ticks,
  when the second tick runs, then a unified diff + provenance URL is
  delivered (`delivered`/`changed`); the FIRST tick was a silent
  baseline; an identical third tick delivers nothing
  (`silent`/`unchanged`); a redirect chain longer than one hop is
  refused with `monitor_redirect_chain_too_long`; with a configured
  fetch-policy cap and headers, EXACTLY one GET is issued to the
  origin per tick (no background prefetch), the cap and headers are
  honored, and the delivery target comes from config (R2).
- [ ] A4. Given a monitor-query job, when a tick gathers sources, then
  the briefing delivers with provenance URLs (`delivered`/`briefing`);
  when the briefing content matches the last delivered one, then the
  tick is silent and recorded as `silent`/`deduped`; the tick's
  observability row records exactly ONE model turn and only the
  bounded research toolset was offered; with a configured prompt +
  source scope + cadence, the tick runs against exactly those (R3).
- [ ] A5. Given a monitor whose source fails 3 consecutive ticks, when
  the failures accumulate, then it is archived with
  `monitor_archived_unreachable` (outcome class `archived`, CronStore
  status `archived`) and no longer scheduled (R4).
- [ ] A6. Given the full implementation, when `just verify` runs, then
  the script/URL/query monitor suites pass with zero skips and every
  terminal outcome class is recorded in the observability plane with
  per-kind outcome rows for the classes each kind can produce
  (script: delivered/silent/failed/archived/skipped; URL monitor:
  delivered/changed, silent/unchanged, failed, archived, skipped;
  research monitor: delivered/briefing, silent/deduped, failed,
  archived, skipped); a row recording ZERO or TWO terminal outcome
  classes fails the suite (exactly-one exclusivity) (R0, R5).
- [ ] A7. Given the merged commit, when the spec-007 R1 amendment diff
  is inspected, then the amended R1 clause is present in the SAME
  commit as the cron.rs job-kind change, and the pre-existing
  cron/gateway suites pass unchanged (R0).
- [ ] A8. Given a script job that attempts out-of-cwd filesystem
  access, when the tick runs, then the access is denied and the tick
  records `failed` with `script_sandbox_denied` (R1).
- [ ] A9. Given a monitor-query tick that gathers no sources, when
  the tick completes, then no briefing is delivered and the R4 named
  diagnostic `monitor_no_sources` is recorded (`failed`) (R3, R4).
- [ ] A10. Given jobs of each kind with recorded outcomes, when
  `cron_list`/`cron_history` run, then kind + last outcome are
  exposed per job and `cron_list` distinguishes `archived` from
  `paused` (disabled) rows; the event rows are ordered (monotonic
  sequence per job) and each row carries a delivery-receipt column
  (null for silent/skipped) (R5).
- [ ] A11. Given a monitor delivery whose outbound send fails, when
  the outbox processes it, then the item is marked failed per
  spec-017 R8 with the named diagnostic `outbound_send_failed`
  (pinned here; spec-017 R8 does not name it), never reported as
  success, the enqueue created a durable outbox ledger row, and that
  row carries a routing address (ADR-0071), not a session identity
  (R6).
- [ ] A12. Given a script job tick and a URL-monitor tick, when each
  runs, then their observability rows record ZERO model turns and the
  no-LLM fixture counter stays at zero (R1, R2).
- [ ] A13. Given an established baseline hash and a recorded
  `last_delivered_hash`, when the process restarts, then both hashes
  are still present on the job's row (row inspection) and the next
  tick compares against them (R2, R3).

## Out of scope

- Vertical-specific connectors (Polymarket/arxiv/feed parsing) — the
  generic mechanism is this spec; connectors are integrations
  (spec-020 pattern).
- Alert fan-out to multiple targets and escalations.
- ML-based anomaly detection over monitor histories.
- Feed auto-discovery (RSS sniffing) — MAY later.

## Open questions

- Whether script jobs need a delivery-target override (default: the
  configured gateway chat) — default: per-job target, falling back to
  the global default.
- N (consecutive failures before archive) default of 3 — configurable
  per job (MAY).

## Links

- `crates/optimus-ops/src/cron.rs` — the claim-lease engine the new
  job kinds extend.
- `crates/optimus-ops/src/gateway.rs` — the outbox delivery path
  (R6).
- `specs/007-ops/spec.md` — the owning operator-services spec
  (amendment pattern: R1 must gain the new job kinds).
- `specs/017-gateway-breadth/spec.md` — delivery + failure contract
  (R6).
- `docs/decisions/0070-an-outbound-send-is-a-durable-obligation.md`,
  `docs/decisions/0071-a-routing-address-is-not-a-session-identity.md`,
  `docs/decisions/0073-an-unreachable-vertical-is-archived-not-carried.md`
  — the delivery, routing, and archive laws.
- `docs/architecture/sota-scorecard.md` — P23 versioned extract +
  provenance slice (R2's fetch path).
