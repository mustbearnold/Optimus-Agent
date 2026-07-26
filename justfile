# Optimus Agent task runner.
#
# One memorable command per job. Gate logic lives in scripts/verify.sh so the
# hook, the justfile, humans, and coding agents all run the same thing.
#
#   just            list recipes
#   just check      static gates + compile        (~35s, the inner loop)
#   just verify     everything                    (pre-push gate)

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

# JS unit suites + Playwright (~2min).
ui:
    @bash scripts/verify.sh ui

# Every tier. This is what the pre-push hook runs.
verify:
    @bash scripts/verify.sh all

# --- build and run -----------------------------------------------------------

# Debug build of the whole workspace.
build:
    cargo build --workspace --all-targets

# Release build of the shipped binaries.
build-release:
    cargo build --release -p optimus-desktop -p optimus-cli

# Default desktop: React workbench over the Rust host.
dev:
    npm --prefix apps/optimus-ui run build
    npm --prefix apps/optimus-electron run dev

# Rust host in browser-testable HTTP mode on :8787.
serve:
    cargo run -p optimus-desktop -- --http 8787

# Legacy Wry native shell (no Electron).
dev-legacy:
    cargo run -p optimus-desktop

# --- fix ---------------------------------------------------------------------

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

# Engineering Memory: staleness check.
em-check:
    python3 scripts/engineering_memory.py check

# Engineering Memory: regenerate then validate.
em-generate:
    python3 scripts/engineering_memory.py generate
    python3 scripts/engineering_memory.py validate --quick

# Engineering Memory: budgeted context pack for agent prompts.
em-context budget="3000":
    python3 scripts/engineering_memory.py context --budget {{budget}}

# Hermes parity gate. Fail-closed by design — BLOCKED is the expected
# answer until feature and performance evidence exists. Informational only;
# deliberately excluded from `just verify`.
parity:
    -python3 scripts/optimus_version.py gate

# Install for the current user, then relaunch.
install:
    bash scripts/rebuild-install-relaunch.sh

# --- setup -------------------------------------------------------------------

# Point git at the versioned hooks in .githooks (run once per clone).
setup-hooks:
    git config core.hooksPath .githooks
    @echo "core.hooksPath -> .githooks"

# Install npm dependencies for all JS surfaces.
setup-npm:
    npm --prefix apps/optimus-ui ci
    npm --prefix apps/optimus-electron ci
    npm --prefix apps/optimus-desktop ci
    npm --prefix apps/optimus-desktop exec -- playwright install chromium

# One-time setup for a fresh clone.
setup: setup-hooks setup-npm
