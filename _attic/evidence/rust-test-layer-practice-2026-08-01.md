---
doc_id: evidence-rust-test-layer-practice-2026-08-01
doc_type: evidence
plane: evidence
status: historical
authority: record
summary: Dated check of current Rust integration-testing practice and dev-dependency maturity, made before building the first integration test layer for optimus-ops, with the reasons the layer follows house style rather than the tooling the wider ecosystem now recommends.
reviewed_on: 2026-08-01
review_by: never
---

# Rust test-layer practice check — 2026-08-01

AGENTS.md development workflow step 6 requires that a *new* test layer be
preceded by a dated check of current practice and tooling, by search rather than
from memory, with the finding and its sources recorded. This is that record for
the first integration test layer of `crates/optimus-ops` (task
`b-qual-03-optimus-ops-integration-tests`). It is bound to its date; a later
pass should re-check rather than inherit it.

## What was checked

Current guidance for structuring Rust test suites in 2026, and the maturity of
every crate the new layer would depend on.

- [Rust Testing Strategies: Unit, Integration, and Property Tests](https://dasroot.net/posts/2026/03/rust-testing-strategies-unit-integration-property-tests/)
- [The Complete Guide to Rust Testing](https://blog.blackwell-systems.com/posts/rust-testing-comprehensive-guide/)
- [`rstest` on crates.io](https://crates.io/crates/rstest) — 0.26.1
- [`proptest` on crates.io](https://crates.io/crates/proptest) — resolved 1.11.0 in this workspace

For the crash-and-reopen shape the cron and ledger tests needed, a search for
SQLite crash-recovery testing returned mostly engine-internals material
([frankensqlite](https://github.com/Dicklesworthstone/frankensqlite),
[concurrency control and recovery in SQLite](https://dev.to/lovestaco/concurrency-control-and-database-recovery-in-sqlite-2pmo))
rather than consumer-side guidance. The transferable idea is the only part used:
fabricate the state a crash would leave behind, then reopen the database and
assert on what the file — not the connection — actually holds.

## Where this suite sits against that bar

Current guidance recommends three layers: `rstest` for fixtures and
parameterised cases, `insta` for snapshots, and `proptest` for randomised
invariants.

**`rstest` and `insta` appear nowhere in this workspace.** House style across all
thirteen existing `tests/` directories is plain `#[test]` with
`tempfile::tempdir()` and direct calls into the crate's public API. Adopting a
fixture or snapshot framework for one crate's first integration layer would fork
the workspace's testing idiom on the authority of a coverage task. That is a
workspace-wide decision and belongs to its own change, so this layer follows
house style deliberately, not by default.

**`proptest` is already blessed in-tree** — a workspace dependency, consumed by
`apps/optimus-tui` with a committed `proptest-regressions/composer.txt`. It
therefore carries no new supply-chain surface, and it is used in exactly the one
place it earns its keep: `tests/outbound_ledger_invariants.rs`, where the claim
under test (architectural law 10 — exactly one terminal outcome) is a statement
about *all* interleavings of send, sweep, crash, and operator resolution rather
than the handful anyone would think to write down by hand.

New dev-dependencies added to `crates/optimus-ops` are `proptest`, `tempfile`,
and `uuid`, all pinned through existing workspace entries; `uuid` was already a
normal dependency of the crate.

## Non-vacuity check

A randomised suite that never reaches the interesting states is green for the
wrong reason. Before accepting the layer, an environment-selected probe asserted
each ledger state unreachable in turn, run at `PROPTEST_CASES=400`. All five —
`pending`, `sending`, `delivered`, `ambiguous`, `abandoned` — were independently
reached, so every invariant in the file is exercised rather than trivially true.
The probe and the regression seeds its deliberate failures wrote were then
removed.

## Result

27 integration tests across four files, on a crate that previously had none:

| File | Tests | What it pins |
|---|---|---|
| `tests/gateway_delivery_spine.rs` | 13 | The inbound claim/complete/fail spine and the debts a turn does and does not mint |
| `tests/channel_seam_contracts.rs` | 8 | Adapter address round-trips, whole-reply delivery, and channel isolation |
| `tests/cron_restart_contracts.rs` | 4 | What the schedule store owes a worker that never came back |
| `tests/outbound_ledger_invariants.rs` | 2 | Ledger safety and liveness under arbitrary step orderings |

Every public entry point in `optimus-ops` reopens the SQLite database from its
path, so a sequence of public API calls is structurally a sequence of process
lifetimes. Restricting these tests to the crate's re-export surface is therefore
not only an external-consumer contract check — it is what makes them restart
tests at all.
