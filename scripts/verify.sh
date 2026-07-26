#!/usr/bin/env bash
# Single source of truth for Optimus local gates.
#
# Every caller — humans, coding agents, the pre-push hook, and the justfile —
# runs this script. There is no second list of commands to drift out of sync.
#
# Usage: scripts/verify.sh [tier]
#
#   gates   static gates + fmt                        (~1s)
#   check   gates + cargo check + clippy              (~35s)
#   test    check + cargo test                        (~60s)
#   ui      JS unit suites + Playwright               (~2min)
#   all     every tier above (default; pre-push gate)
#
# Exits non-zero if any gate fails. All gates run to completion first so one
# invocation reports the whole picture rather than only the first failure.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$ROOT" || exit 1

TIER="${1:-all}"

PASSED=(); FAILED=(); SKIPPED=()

if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
  R=$'\033[31m'; G=$'\033[32m'; Y=$'\033[33m'; B=$'\033[1m'; X=$'\033[0m'
else
  R=""; G=""; Y=""; B=""; X=""
fi

# run <name> <command...>
run() {
  local name="$1"; shift
  printf '%-46s' "  $name"
  local out
  if out=$("$@" 2>&1); then
    printf '%s\n' "${G}ok${X}"
    PASSED+=("$name")
  else
    printf '%s\n' "${R}FAIL${X}"
    FAILED+=("$name")
    printf '%s\n' "$out" | tail -15 | sed 's/^/      /'
  fi
}

# skip <name> <reason>
skip() {
  printf '%-46s%s\n' "  $1" "${Y}skip${X} ($2)"
  SKIPPED+=("$1")
}

section() { printf '\n%s\n' "${B}$1${X}"; }

# --- tier: gates -------------------------------------------------------------
tier_gates() {
  section "gates"
  run "fmt"                        cargo fmt --all -- --check
  run "architecture-marks"         python3 scripts/check-architecture-marks.py
  run "crate-layers"               python3 scripts/check-crate-layers.py
  run "domain-modularity"          python3 scripts/check-domain-modularity.py
  run "desktop-ipc-matrix"         python3 scripts/check-desktop-ipc-matrix.py
  run "observability"              python3 scripts/check-observability-gate.py
  run "module-size"                python3 scripts/check-module-size.py
  run "product-complete-install"   python3 scripts/check-product-complete-install.py
  run "parity-ledger"              python3 scripts/check-parity-ledger.py
  run "version-validate"           python3 scripts/optimus_version.py validate
  run "version-release-check"      python3 scripts/optimus_version.py release-check
  run "engineering-memory"         python3 scripts/engineering_memory.py check

  section "gate self-tests"
  run "test_architecture_marks"    python3 scripts/test_architecture_marks.py
  run "test_desktop_ipc_matrix"    python3 scripts/test_desktop_ipc_matrix.py
  run "test_engineering_memory"    python3 scripts/test_engineering_memory.py
  run "test_github_pr_branch"      python3 scripts/test_github_pr_branch.py
  run "test_module_size"           python3 scripts/test_module_size.py
  run "test_optimus_version"       python3 scripts/test_optimus_version.py
  run "test_rebuild_install"       python3 scripts/test_rebuild_install_safety.py
}

# --- tier: check -------------------------------------------------------------
tier_check() {
  section "compile"
  run "cargo check" cargo check --workspace --all-targets
  if cargo clippy --version >/dev/null 2>&1; then
    run "clippy" cargo clippy --workspace --all-targets -- -D warnings
  else
    skip "clippy" "not installed: rustup component add clippy"
  fi
}

# --- tier: test --------------------------------------------------------------
tier_test() {
  section "rust tests"
  if cargo nextest --version >/dev/null 2>&1; then
    run "cargo nextest" cargo nextest run --workspace
  else
    run "cargo test" cargo test --workspace --all-targets -- --test-threads=1
  fi
}

# --- tier: ui ----------------------------------------------------------------
# in_dir <name> <dir> <shell-command>
# Runs from <dir>. These suites resolve their config and test globs relative to
# the working directory, so `npm --prefix` is not enough — it sets the package
# root but leaves cwd at the repo root, which makes Playwright pick up the
# wrong project's config.
in_dir() {
  local name="$1" dir="$2" cmd="$3"
  if [ ! -d "$dir/node_modules" ]; then
    skip "$name" "npm ci in $dir"
    return
  fi
  run "$name" bash -c "cd '$dir' && $cmd"
}

tier_ui() {
  section "ui suites"
  in_dir "optimus-ui vitest" apps/optimus-ui       "npm test"
  in_dir "optimus-electron"  apps/optimus-electron "npm test"

  if [ ! -d apps/optimus-desktop/node_modules ]; then
    skip "playwright" "npm ci in apps/optimus-desktop"
  elif ! (cd apps/optimus-desktop && npx playwright --version >/dev/null 2>&1); then
    skip "playwright" "npx playwright install chromium"
  else
    in_dir "playwright" apps/optimus-desktop "npx playwright test"
  fi
}

case "$TIER" in
  gates) tier_gates ;;
  check) tier_gates; tier_check ;;
  test)  tier_gates; tier_check; tier_test ;;
  ui)    tier_ui ;;
  all)   tier_gates; tier_check; tier_test; tier_ui ;;
  *)
    printf 'unknown tier: %s\n' "$TIER" >&2
    printf 'expected one of: gates check test ui all\n' >&2
    exit 2
    ;;
esac

section "summary"
printf '  %s%d passed%s' "$G" "${#PASSED[@]}" "$X"
[ "${#SKIPPED[@]}" -gt 0 ] && printf ', %s%d skipped%s' "$Y" "${#SKIPPED[@]}" "$X"
[ "${#FAILED[@]}" -gt 0 ] && printf ', %s%d failed%s' "$R" "${#FAILED[@]}" "$X"
printf '\n'

if [ "${#FAILED[@]}" -gt 0 ]; then
  printf '\n  failed: %s\n\n' "${FAILED[*]}"
  exit 1
fi
printf '\n'
