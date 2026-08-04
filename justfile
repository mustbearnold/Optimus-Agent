# Optimus Agent task runner.
#
# One memorable command per job. Gate logic lives in scripts/verify.sh so the
# hook, the justfile, humans, and coding agents all run the same thing.
#
#   just            list recipes
#   just check      static gates + compile        (~35s, the inner loop)
#   just verify     everything                    (release gate)

set shell := ["bash", "-uc"]

_default:
    @just --list --unsorted

# --- gates -------------------------------------------------------------------

# Static gates + fmt only (~1s).
gates:
    @bash scripts/verify.sh gates

# Gates + cargo check + clippy (~35s). The default inner loop.
check:
    @bash scripts/verify.sh check

# Gates + compile + Rust tests (~60s).
test:
    @bash scripts/verify.sh test

# JS unit suites + Playwright + TUI pty e2e (~2min).
ui:
    @bash scripts/verify.sh ui

# Real binary in a pty, with Playwright asserting a DOM projection of each
# terminal frame across wide, normal, and narrow workbench sizes.
tui-layout:
    cargo build -p optimus-cli
    bun scripts/tui_layout_playwright.cjs

# Every tier. Run this before pushing; skips are forbidden.
verify:
    @bash scripts/verify.sh all

# Bounded startup card for every coding agent.
orient:
    @python3 scripts/repository_ontology.py orient

# Explain what a path is, whether it ships, and whether it is removable.
explain-path path:
    @python3 scripts/repository_ontology.py explain-path {{quote(path)}}

# Atomically rebuild the indexed SQLite file/component/commit provenance graph.
project-graph:
    @python3 scripts/project_knowledge.py generate

# Current source history, lifecycle deadlines, and machine-local disk state.
project-status:
    @python3 scripts/project_knowledge.py status

# Ranked cleanup candidates with deletion authority kept separate from age.
cleanup-candidates:
    @python3 scripts/project_knowledge.py cleanup

# Freeze the exact inactive generated-output set before destructive cleanup.
project-cleanup-plan:
    @python3 scripts/managed_project_cleanup.py plan

# Remove only the unchanged generated paths from a reviewed exact plan.
project-cleanup plan_sha256:
    @python3 scripts/managed_project_cleanup.py execute {{quote(plan_sha256)}}

# Show the complete retained Git history for a current or deleted path.
path-history path:
    @python3 scripts/project_knowledge.py history {{quote(path)}}

# Query a path as it existed at an exact commit or ISO-8601 timestamp.
path-at path point:
    @python3 scripts/project_knowledge.py at {{quote(path)}} {{quote(point)}}

# Traverse indexed property-graph relations from a file, component, package, or commit.
project-neighbors entity depth="1" at="":
    @python3 scripts/project_knowledge.py neighbors {{quote(entity)}} --depth {{quote(depth)}} {{ if at == "" { "" } else { "--at " + quote(at) } }}

# Run one arbitrary read-only SQL query against the generated database.
project-query query:
    @python3 scripts/project_knowledge.py query {{quote(query)}}

# Append an immutable local observation under Development/land.
project-snapshot:
    @python3 scripts/project_knowledge.py snapshot

# --- focused verification (program P42) ---------------------------------------

# What this patch can break, and why. Reports only; runs nothing.
impact:
    @python3 scripts/impact_select.py

# Static gates + the tests this patch can actually break (~10s on a leaf crate).
# Escalates to the whole workspace whenever the selector cannot prove a
# narrower answer, so a green `dev-check` is never narrower than the truth.
dev-check:
    @bash scripts/verify.sh gates
    @python3 scripts/impact_select.py
    @cargo test $(python3 scripts/impact_select.py --cargo-args) --all-targets

# Impact-selected tests only, no gates. Fails when nothing is selected: "no
# tests ran" and "the tests passed" are different sentences.
test-changed:
    @python3 scripts/impact_select.py --require-selection
    @cargo test $(python3 scripts/impact_select.py --cargo-args) --all-targets

# Real-model smoke: real Codex through the host and the TUI pty. Spends
# tokens; needs the installed home's credential. Release / live-surface gate.
live:
    @bash scripts/verify.sh live

# Build and exercise the real scoped self-development supervisor lifecycle.
self-development:
    cargo build --locked -p optimus-desktop --bin optimus-desktop
    @python3 scripts/test_self_development.py

# Build and exercise the real windowed Tauri child through the stable host.
self-development-desktop:
    bun run --cwd apps/optimus-ui build
    cargo build --locked -p optimus-tauri --bin optimus-agent --features optimus-tauri/custom-protocol
    cargo build --locked -p optimus-desktop --bin optimus-desktop
    @python3 scripts/test_self_development.py --surface desktop

# --- performance baseline (informational until ADR-0069 is accepted) ----------

# Measure one or all perf scenarios offline (quick 5x2; pass a scenario id).
perf-run scenario="all":
    @python3 scripts/perf_harness.py run --scenario {{quote(scenario)}}

# Deliberately overwrite docs/architecture/perf-baseline.json with a fresh run.
perf-baseline:
    @python3 scripts/perf_harness.py baseline

# Compare a fresh quick run against the committed baseline; report only.
perf-compare:
    @python3 scripts/perf_harness.py compare

# Compact wall-time, model/token, tool, and Developer Full Access action report.
efficiency db actions:
    @python3 scripts/development_efficiency.py --db {{quote(db)}} --actions {{quote(actions)}}

# --- build and run -----------------------------------------------------------

# Debug build of the whole workspace.
build:
    cargo build --workspace --all-targets

# Release build of the shipped binaries.
build-release:
    cargo build --release -p optimus-tauri -p optimus-desktop -p optimus-cli --features optimus-tauri/custom-protocol

# Default desktop: Tauri v2 shell over the Rust host, React workbench.
dev:
    bun run --cwd apps/optimus-tauri dev

# Rust host in browser-testable HTTP mode on :8787.
serve:
    cargo run -p optimus-desktop -- --http 8787

# Legacy Wry native shell (no Electron).
dev-legacy:
    cargo run -p optimus-desktop

# --- fix ---------------------------------------------------------------------

# Report rebuildable artifacts and shared report-only candidates.
clean-report:
    python3 scripts/project_hygiene.py report

# Delete only the closed, verified allowlist of generated outputs.
clean:
    python3 scripts/project_hygiene.py clean

# Apply rustfmt and machine-applicable clippy fixes.
fix:
    cargo fmt --all
    cargo clippy --workspace --all-targets --fix --allow-dirty --allow-staged

# Full clippy report. Enforced as a hard gate in `just verify`.
lint:
    cargo clippy --workspace --all-targets -- -D warnings

# Ranked module sizes against the 800-line law.
modules:
    python3 scripts/check-module-size.py --report

# Ratchet the module-size baseline down after a split.
modules-ratchet:
    python3 scripts/check-module-size.py --update

# --- project systems ---------------------------------------------------------

# Documentation authority, metadata, local links, staleness and retrieval.
docs-check:
    python3 scripts/docs_system.py check

# Regenerate the deterministic catalog. This never refreshes source bindings.
docs-generate:
    python3 scripts/docs_system.py generate

# Acknowledge reviewed source changes for named current documents only.
docs-refresh *doc_ids:
    python3 scripts/docs_system.py refresh {{doc_ids}}

# Search current authority by intent. History and evidence are opt-in.
docs-search query:
    python3 scripts/docs_system.py search {{quote(query)}}

# Return the bounded primary/supporting pack for one authority route.
docs-context route:
    python3 scripts/docs_system.py context {{quote(route)}}

# Prove representative fresh-agent questions resolve to the expected authority.
docs-benchmark:
    python3 scripts/docs_system.py benchmark

# Engineering Memory: staleness check.
em-check:
    python3 scripts/engineering_memory.py check

# Engineering Memory: regenerate then validate.
em-generate:
    python3 scripts/engineering_memory.py generate
    python3 scripts/project_knowledge.py generate
    python3 scripts/engineering_memory.py validate --quick

# Engineering Memory: budgeted context pack for agent prompts.
em-context budget="3000":
    python3 scripts/engineering_memory.py context --budget {{budget}}

# Hermes comparison scorecard — informational (ADR-0069); the release bar
# is `just release-gate`.
parity:
    -python3 scripts/optimus_version.py hermes-scorecard

# ADR-0069 native release bar: ledger green + no perf regression vs the
# committed same-machine baseline. Not in `just verify`; run for releases.
release-gate:
    python3 scripts/optimus_version.py gate

# Install for the current user, then relaunch.
install:
    bash scripts/rebuild-install-relaunch.sh

# --- setup -------------------------------------------------------------------

# Install the Bun workspace and browser dependency from the frozen lockfile.
setup-bun:
    bun install --frozen-lockfile
    bunx playwright install chromium

# One-time setup for a fresh clone.
setup: setup-bun
