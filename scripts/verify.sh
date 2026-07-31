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
#   live    real-model smoke: real Codex through the host and the TUI pty.
#           Spends tokens and needs a real credential, so it is NOT in `all`;
#           it is the gate for releases and for changes to live surfaces.
#           Missing creds/tmux FAIL — a live tier must never quietly skip.
#
# Exits non-zero if any gate fails. All gates run to completion first so one
# invocation reports the whole picture rather than only the first failure.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$ROOT" || exit 1

TIER="${1:-all}"

# Reuse the repository-managed Playwright payload when this linked worktree has
# one. CI and ordinary clones fall back to Playwright's default cache. Keeping
# this path discovery here makes every gate caller see the same browser rather
# than reporting dozens of assertion failures caused by one missing executable.
if [ -z "${PLAYWRIGHT_BROWSERS_PATH:-}" ]; then
  for browser_cache in \
    "$ROOT/local/tools/playwright-browsers" \
    "$ROOT/../../tools/playwright-browsers"
  do
    if [ -d "$browser_cache" ]; then
      PLAYWRIGHT_BROWSERS_PATH="$(cd "$browser_cache" && pwd -P)"
      export PLAYWRIGHT_BROWSERS_PATH
      break
    fi
  done
fi

if ! command -v tmux >/dev/null 2>&1; then
  for tmux_bin_dir in \
    "$ROOT/local/tools/tmux-root/usr/bin" \
    "$ROOT/../../tools/tmux-root/usr/bin"
  do
    if [ -x "$tmux_bin_dir/tmux" ]; then
      PATH="$tmux_bin_dir:$PATH"
      export PATH
      break
    fi
  done
fi

# Normalise the Rust toolchain for this script's lifetime.
#
# This host has two Rust installations: a distribution cargo/rustc in /usr/bin
# and rustup's shims in ~/.cargo/bin, with /usr/bin first on PATH. Subcommands
# are resolved independently, so `cargo` came from /usr/bin (1.93.1) while
# `cargo-clippy` came from rustup (1.97.1) — the dependency graph was then built
# by one rustc and read by another, which fails with E0514 "compiled by an
# incompatible version of rustc" and takes the whole gate run with it.
#
# Putting rustup's shims first makes every cargo subcommand resolve to the
# toolchain that rust-toolchain.toml pins. Scoped to this process: the user's
# shell configuration is left alone.
if [ -x "$HOME/.cargo/bin/cargo" ]; then
  PATH="$HOME/.cargo/bin:$PATH"
  export PATH
fi

PASSED=(); FAILED=(); SKIPPED=(); SKIPPED_DETAIL=()

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
    # Reported *and* returned, so a caller can decline to run the gate that
    # this step was preparing for. Nothing runs under `set -e`, so callers that
    # ignore the status behave exactly as they did before.
    return 1
  fi
}

# skip <name> <reason>
skip() {
  printf '%-46s%s\n' "  $1" "${Y}skip${X} ($2)"
  SKIPPED+=("$1")
  # The reason is already phrased as the missing prerequisite at every call
  # site, so keeping it alongside the name is all the pre-push hook needs to
  # tell the pusher what did not run and why (#98).
  SKIPPED_DETAIL+=("$1	$2")
}

section() { printf '\n%s\n' "${B}$1${X}"; }

# --- parallel gate execution -------------------------------------------------
#
# Gates are independent processes, so running them one at a time wasted most of
# the wall clock: two self-tests alone (engineering-memory ~25s, rebuild-install
# ~12s) serialised ahead of seventeen gates that finish in under a second
# combined.
#
# `spawn` starts a gate in the background; `reap` waits, then reports every gate
# in spawn order so output is deterministic no matter which finished first.
# Section headers are recorded per gate and printed during reap, so the report
# reads exactly as it did when this ran serially. Same commands, same pass/fail
# rule, same summary — only the scheduling changes.
#
# OPTIMUS_VERIFY_SERIAL=1 forces one-at-a-time when bisecting a flaky gate.
JOBS_DIR=""
SPAWNED=()
PENDING_SECTION=""

cleanup_jobs() { [ -n "$JOBS_DIR" ] && rm -rf "$JOBS_DIR"; }
trap cleanup_jobs EXIT

# spawn_section <title> — header shown before the next spawned gate's result.
spawn_section() {
  if [ -n "${OPTIMUS_VERIFY_SERIAL:-}" ]; then
    section "$1"
    return
  fi
  PENDING_SECTION="$1"
}

# spawn <name> <command...>
spawn() {
  local name="$1"; shift
  if [ -n "${OPTIMUS_VERIFY_SERIAL:-}" ]; then
    run "$name" "$@"
    return
  fi
  [ -n "$JOBS_DIR" ] || JOBS_DIR="$(mktemp -d)"
  local slug="${#SPAWNED[@]}"
  (
    if out=$("$@" 2>&1); then
      printf 'ok' > "$JOBS_DIR/$slug.status"
    else
      printf 'fail' > "$JOBS_DIR/$slug.status"
    fi
    printf '%s' "$out" > "$JOBS_DIR/$slug.out"
  ) &
  SPAWNED+=("$PENDING_SECTION|$name")
  PENDING_SECTION=""
}

# spawn_dir <name> <dir> <shell-command>
# Background twin of `in_dir`: these suites resolve config and test globs
# relative to cwd, so `npm --prefix` is not enough.
spawn_dir() {
  local name="$1" dir="$2" cmd="$3"
  if [ ! -d "$dir/node_modules" ]; then
    skip "$name" "npm ci in $dir"
    return
  fi
  spawn "$name" bash -c "cd '$dir' && $cmd"
}

# reap — wait for every spawned gate, then report in spawn order.
reap() {
  [ "${#SPAWNED[@]}" -eq 0 ] && return 0
  wait
  local slug=0 entry name title status
  for entry in "${SPAWNED[@]}"; do
    title="${entry%%|*}"
    name="${entry#*|}"
    [ -n "$title" ] && section "$title"
    status="$(cat "$JOBS_DIR/$slug.status" 2>/dev/null || printf 'fail')"
    printf '%-46s' "  $name"
    if [ "$status" = "ok" ]; then
      printf '%s\n' "${G}ok${X}"
      PASSED+=("$name")
    else
      printf '%s\n' "${R}FAIL${X}"
      FAILED+=("$name")
      tail -15 "$JOBS_DIR/$slug.out" 2>/dev/null | sed 's/^/      /'
    fi
    slug=$((slug + 1))
  done
  SPAWNED=()
}

# --- tier: gates -------------------------------------------------------------
tier_gates() {
  spawn_section "gates"
  spawn "fmt"                        cargo fmt --all -- --check
  spawn "architecture-marks"         python3 scripts/check-architecture-marks.py
  spawn "crate-layers"               python3 scripts/check-crate-layers.py
  spawn "domain-modularity"          python3 scripts/check-domain-modularity.py
  spawn "instruction-planes"         python3 scripts/check-instruction-planes.py
  spawn "desktop-ipc-matrix"         python3 scripts/check-desktop-ipc-matrix.py
  spawn "autonomy-profiles"          python3 scripts/check-autonomy-profiles.py
  spawn "project-scope"              python3 scripts/check-project-scope-assertions.py
  spawn "project-bleed"              python3 scripts/check-project-bleed.py
  spawn "tool-coverage"              python3 scripts/check-tool-coverage.py
  spawn "observability"              python3 scripts/check-observability-gate.py
  spawn "module-size"                python3 scripts/check-module-size.py
  spawn "product-complete-install"   python3 scripts/check-product-complete-install.py
  spawn "parity-ledger"              python3 scripts/check-parity-ledger.py
  spawn "neutral-fixtures"           python3 scripts/check-neutral-fixtures.py
  spawn "version-validate"           python3 scripts/optimus_version.py validate
  spawn "version-release-check"      python3 scripts/optimus_version.py release-check
  spawn "engineering-memory"         python3 scripts/engineering_memory.py check
  spawn "engineering-memory-valid"   python3 scripts/engineering_memory.py validate
  spawn "documentation-contract"     python3 scripts/docs_system.py check

  spawn_section "gate self-tests"
  spawn "test_architecture_marks"    python3 scripts/test_architecture_marks.py
  spawn "test_desktop_ipc_matrix"    python3 scripts/test_desktop_ipc_matrix.py
  spawn "test_engineering_memory"    python3 scripts/test_engineering_memory.py
  spawn "test_docs_system"           python3 scripts/test_docs_system.py
  spawn "test_impact_select"         python3 scripts/test_impact_select.py
  spawn "test_instruction_planes"    python3 scripts/test_instruction_planes.py
  spawn "test_managed_delivery"      python3 scripts/test_managed_delivery.py
  spawn "test_branch_retirement"     python3 scripts/test_managed_branch_retirement.py
  spawn "test_project_hygiene"       python3 scripts/test_project_hygiene.py
  spawn "test_workspace_layout"      python3 scripts/test_workspace_layout.py
  spawn "test_live_smoke"            python3 scripts/test_live_smoke.py
  spawn "test_synthetic_user_lab"    python3 scripts/test_synthetic_user_lab.py
  spawn "test_synthetic_simulator"   python3 scripts/test_synthetic_user_simulator.py
  spawn "test_tool_coverage_gate"    python3 scripts/test_tool_coverage_gate.py
  spawn "test_module_size"           python3 scripts/test_module_size.py
  spawn "test_autonomy_profiles"     python3 scripts/test_autonomy_profiles.py
  spawn "test_optimus_version"       python3 scripts/test_optimus_version.py
  spawn "test_rebuild_install"       python3 scripts/test_rebuild_install_safety.py
  spawn "test_verify_skip_report"    python3 scripts/test_verify_skip_report.py
  spawn "test_verify_gate_parity"    python3 scripts/test_verify_gate_parity.py
  reap
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

# --- tier: live --------------------------------------------------------------
# Real model, real credential, real cost — deliberately invoked, never implied
# by `all` (CI holds no credential, and C6 bans gates that quietly skip).
tier_live() {
  section "live model"
  if [ ! -x target/debug/optimus ]; then
    run "build optimus cli" cargo build -p optimus-cli
  fi
  run "live smoke (codex)" python3 scripts/live_smoke.py
  # Browser success needs the public web (loopback is refused by the SSRF
  # law), so its dispatch-path test is network-marked and runs here.
  run "browser success (network)" \
    cargo test -p optimus-kernel --test tool_coverage -- --ignored
  # leg 3 — desktop face: playwright drives the real optimus-desktop binary
  # over its HTTP shell against the credentialed home (the #82 contract:
  # boots on codex, real model answers a nonce). cd, not npm --prefix:
  # playwright resolves its config from cwd (see the ui-tier note).
  if [ ! -x target/debug/optimus-desktop ]; then
    run "build optimus desktop" cargo build -p optimus-desktop
  fi
  run "live desktop (codex)" bash -c \
    'cd apps/optimus-desktop && OPTIMUS_E2E_HOME="${OPTIMUS_LIVE_HOME:-$HOME/.local/share/optimus}" npx playwright test --config=playwright.live.config.js'
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

# Electron opens real windows, so the e2e tier needs a display. Wrap in xvfb-run
# when there is none (headless agents, CI) and leave a real session alone so its
# native paint stays authentic.
electron_e2e_command() {
  if [ -n "${DISPLAY:-}" ] || [ -n "${WAYLAND_DISPLAY:-}" ]; then
    printf 'npx playwright test'
  elif command -v xvfb-run >/dev/null 2>&1; then
    # xvfb-run's default server is 640x480x8 — too small for the workbench
    # window, so layout assertions (execution dock height) fail headless.
    printf 'xvfb-run -a -s "-screen 0 1920x1080x24" npx playwright test'
  else
    printf ''
  fi
}

tier_ui() {
  # The two node unit suites and the two Playwright suites are independent of
  # each other; only the Playwright suites need the built host binary and UI
  # bundle, so that build is the one serial step.
  spawn_section "ui suites"
  spawn_dir "optimus-ui vitest" apps/optimus-ui       "npm test"
  spawn_dir "optimus-electron"  apps/optimus-electron "npm test"

  # The terminal face gets the same treatment as the desktop: the real binary,
  # driven end to end, deterministically (offline provider, temp home).
  if command -v tmux >/dev/null 2>&1; then
    run "build optimus cli" cargo build -p optimus-cli
    spawn "tui e2e" python3 scripts/tui_e2e.py
  else
    skip "tui e2e" "tmux not installed"
  fi

  # Playwright drives the real host binary (e2e/support.js spawns
  # target/debug/optimus-desktop per worker), so it needs a *built* binary --
  # `cargo check` from the compile tier does not produce one.
  local playwright_ready=1
  if [ ! -d apps/optimus-desktop/node_modules ]; then
    skip "playwright" "npm ci in apps/optimus-desktop"
    playwright_ready=0
  elif ! (cd apps/optimus-desktop && npx playwright --version >/dev/null 2>&1); then
    skip "playwright" "npx playwright install chromium"
    playwright_ready=0
  fi

  local electron_ready=1
  if [ ! -d apps/optimus-electron/node_modules ]; then
    skip "electron e2e" "npm ci in apps/optimus-electron"
    electron_ready=0
  elif ! (cd apps/optimus-electron && npx playwright --version >/dev/null 2>&1); then
    skip "electron e2e" "npx playwright install chromium"
    electron_ready=0
  elif [ ! -d apps/optimus-ui/node_modules ]; then
    skip "electron e2e" "npm ci in apps/optimus-ui"
    electron_ready=0
  elif [ -z "$(electron_e2e_command)" ]; then
    skip "electron e2e" "no display and no xvfb-run"
    electron_ready=0
  fi

  if [ "$playwright_ready" = 1 ] || [ "$electron_ready" = 1 ]; then
    run "build desktop host" cargo build -p optimus-desktop
  fi
  # The bundle is gitignored, so it is never checked out, never updated by a
  # merge, and never invalidated by a rebase -- the gate would otherwise test
  # whichever JavaScript happened to be sitting on disk (#107). That reads both
  # ways: a stale bundle fails a branch that changed nothing near it, and an
  # unbuilt edit passes while CI, which builds from scratch, does not. Built
  # here for the same reason the host is built above.
  if [ "$electron_ready" = 1 ]; then
    if ! run "build react ui" npm --prefix apps/optimus-ui run build; then
      # A failed build leaves the *previous* bundle in place, so running the
      # gate now would assert against code nobody wrote. The build failure is
      # already the report; a second, misleading one adds nothing.
      electron_ready=0
    fi
  fi
  [ "$playwright_ready" = 1 ] && spawn_dir "playwright" apps/optimus-desktop "npx playwright test"
  [ "$electron_ready" = 1 ] && spawn_dir "electron e2e" apps/optimus-electron "$(electron_e2e_command)"
  reap
}

# --- tier: all ---------------------------------------------------------------
# Running the tiers back to back leaves most cores idle: the Python gates never
# touch cargo, the node unit suites never touch cargo, and Playwright only needs
# the host binary. So build that binary first, then run everything else at once
# and reap in a stable order.
#
# Concurrent cargo invocations serialise on the target-dir lock by themselves, so
# fmt/check/clippy/nextest queue rather than corrupt anything — they simply stop
# blocking the suites that have no cargo dependency.
tier_all() {
  # The one true ordering constraint: Playwright spawns this binary per worker.
  local host_built=0
  if [ -d apps/optimus-desktop/node_modules ] || [ -d apps/optimus-electron/node_modules ]; then
    section "build"
    run "build desktop host" cargo build -p optimus-desktop
    host_built=1
  fi

  spawn_section "gates"
  spawn "fmt"                        cargo fmt --all -- --check
  spawn "architecture-marks"         python3 scripts/check-architecture-marks.py
  spawn "crate-layers"               python3 scripts/check-crate-layers.py
  spawn "domain-modularity"          python3 scripts/check-domain-modularity.py
  spawn "instruction-planes"         python3 scripts/check-instruction-planes.py
  spawn "desktop-ipc-matrix"         python3 scripts/check-desktop-ipc-matrix.py
  spawn "autonomy-profiles"          python3 scripts/check-autonomy-profiles.py
  spawn "project-scope"              python3 scripts/check-project-scope-assertions.py
  spawn "project-bleed"              python3 scripts/check-project-bleed.py
  spawn "tool-coverage"              python3 scripts/check-tool-coverage.py
  spawn "observability"              python3 scripts/check-observability-gate.py
  spawn "module-size"                python3 scripts/check-module-size.py
  spawn "product-complete-install"   python3 scripts/check-product-complete-install.py
  spawn "parity-ledger"              python3 scripts/check-parity-ledger.py
  spawn "neutral-fixtures"           python3 scripts/check-neutral-fixtures.py
  spawn "version-validate"           python3 scripts/optimus_version.py validate
  spawn "version-release-check"      python3 scripts/optimus_version.py release-check
  spawn "engineering-memory"         python3 scripts/engineering_memory.py check
  spawn "engineering-memory-valid"   python3 scripts/engineering_memory.py validate
  spawn "documentation-contract"     python3 scripts/docs_system.py check

  spawn_section "gate self-tests"
  spawn "test_architecture_marks"    python3 scripts/test_architecture_marks.py
  spawn "test_desktop_ipc_matrix"    python3 scripts/test_desktop_ipc_matrix.py
  spawn "test_engineering_memory"    python3 scripts/test_engineering_memory.py
  spawn "test_docs_system"           python3 scripts/test_docs_system.py
  spawn "test_impact_select"         python3 scripts/test_impact_select.py
  spawn "test_instruction_planes"    python3 scripts/test_instruction_planes.py
  spawn "test_managed_delivery"      python3 scripts/test_managed_delivery.py
  spawn "test_branch_retirement"     python3 scripts/test_managed_branch_retirement.py
  spawn "test_project_hygiene"       python3 scripts/test_project_hygiene.py
  spawn "test_workspace_layout"      python3 scripts/test_workspace_layout.py
  spawn "test_live_smoke"            python3 scripts/test_live_smoke.py
  spawn "test_synthetic_user_lab"    python3 scripts/test_synthetic_user_lab.py
  spawn "test_synthetic_simulator"   python3 scripts/test_synthetic_user_simulator.py
  spawn "test_tool_coverage_gate"    python3 scripts/test_tool_coverage_gate.py
  spawn "test_module_size"           python3 scripts/test_module_size.py
  spawn "test_autonomy_profiles"     python3 scripts/test_autonomy_profiles.py
  spawn "test_optimus_version"       python3 scripts/test_optimus_version.py
  spawn "test_rebuild_install"       python3 scripts/test_rebuild_install_safety.py
  spawn "test_verify_skip_report"    python3 scripts/test_verify_skip_report.py
  spawn "test_verify_gate_parity"    python3 scripts/test_verify_gate_parity.py

  spawn_section "compile"
  spawn "cargo check" cargo check --workspace --all-targets
  if cargo clippy --version >/dev/null 2>&1; then
    spawn "clippy" cargo clippy --workspace --all-targets -- -D warnings
  else
    skip "clippy" "not installed: rustup component add clippy"
  fi

  spawn_section "rust tests"
  if cargo nextest --version >/dev/null 2>&1; then
    spawn "cargo nextest" cargo nextest run --workspace
  else
    spawn "cargo test" cargo test --workspace --all-targets -- --test-threads=1
  fi

  spawn_section "ui suites"
  spawn_dir "optimus-ui vitest" apps/optimus-ui       "npm test"
  spawn_dir "optimus-electron"  apps/optimus-electron "npm test"

  # Terminal face, same standard as the desktop: real binary, deterministic pty.
  if command -v tmux >/dev/null 2>&1; then
    run "build optimus cli" cargo build -p optimus-cli
    spawn "tui e2e" python3 scripts/tui_e2e.py
  else
    skip "tui e2e" "tmux not installed"
  fi

  if [ "$host_built" = 1 ] && (cd apps/optimus-desktop && npx playwright --version >/dev/null 2>&1); then
    spawn_dir "playwright" apps/optimus-desktop "npx playwright test"
  else
    skip "playwright" "npm ci + npx playwright install chromium in apps/optimus-desktop"
  fi

  if [ "$host_built" != 1 ]; then
    skip "electron e2e" "npm ci in apps/optimus-electron"
  elif [ ! -d apps/optimus-ui/node_modules ]; then
    skip "electron e2e" "npm ci in apps/optimus-ui"
  elif [ -z "$(electron_e2e_command)" ]; then
    skip "electron e2e" "no display and no xvfb-run"
  else
    # Same reason as the ui tier: the bundle is gitignored, so building it is
    # the only way this gate is testing the source it was handed (#107). A
    # failed build stands as its own report -- asserting against the bundle it
    # did not replace would only add a second, misleading failure.
    if run "build react ui" npm --prefix apps/optimus-ui run build; then
      spawn_dir "electron e2e" apps/optimus-electron "$(electron_e2e_command)"
    fi
  fi

  reap
}

case "$TIER" in
  gates) tier_gates ;;
  check) tier_gates; tier_check ;;
  test)  tier_gates; tier_check; tier_test ;;
  ui)    tier_ui ;;
  live)  tier_live ;;
  all)   tier_all ;;
  *)
    printf 'unknown tier: %s\n' "$TIER" >&2
    printf 'expected one of: gates check test ui live all\n' >&2
    exit 2
    ;;
esac

section "summary"
printf '  %s%d passed%s' "$G" "${#PASSED[@]}" "$X"
[ "${#SKIPPED[@]}" -gt 0 ] && printf ', %s%d skipped%s' "$Y" "${#SKIPPED[@]}" "$X"
[ "${#FAILED[@]}" -gt 0 ] && printf ', %s%d failed%s' "$R" "${#FAILED[@]}" "$X"
printf '\n'

# Hand the skip list to whoever invoked us. The pre-push hook uses it to stop
# calling a partly-run push `clean` (#98); nothing else reads it, and nothing
# here changes if the variable is unset.
if [ -n "${OPTIMUS_VERIFY_SKIP_REPORT:-}" ]; then
  : >"$OPTIMUS_VERIFY_SKIP_REPORT"
  [ "${#SKIPPED_DETAIL[@]}" -gt 0 ] &&
    printf '%s\n' "${SKIPPED_DETAIL[@]}" >"$OPTIMUS_VERIFY_SKIP_REPORT"
fi

if [ "${#FAILED[@]}" -gt 0 ]; then
  printf '\n  failed: %s\n\n' "${FAILED[*]}"
  exit 1
fi

# Skip-is-failure mode (success criterion C6, north-star-2026-07.md). Locally a
# skip is a nudge to install a dev dependency; on a bare CI runner it is a gate
# silently not running — green with silent skips is exactly the self-serving
# shape the criteria ban. CI sets OPTIMUS_VERIFY_FORBID_SKIPS=1.
if [ -n "${OPTIMUS_VERIFY_FORBID_SKIPS:-}" ] && [ "${#SKIPPED[@]}" -gt 0 ]; then
  printf '\n  skipped (forbidden by OPTIMUS_VERIFY_FORBID_SKIPS): %s\n\n' "${SKIPPED[*]}"
  exit 1
fi
printf '\n'
