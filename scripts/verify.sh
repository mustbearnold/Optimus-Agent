#!/usr/bin/env bash
# Single source of truth for Optimus local gates.
#
# Every caller — humans, coding agents, and the justfile —
# runs this script. There is no second list of commands to drift out of sync.
#
# Usage: scripts/verify.sh [tier]
#
#   gates   static gates + fmt                        (~1s)
#   check   gates + cargo check + clippy              (~35s)
#   test    check + cargo test                        (~60s)
#   ui      JS unit suites + Playwright               (~2min)
#   all     every tier above (default; release gate)
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

# Reuse the repository-managed Playwright payload when this checkout has
# one. CI and ordinary clones fall back to Playwright's default cache. Keeping
# this path discovery here makes every gate caller see the same browser rather
# than reporting dozens of assertion failures caused by one missing executable.
if [ -z "${PLAYWRIGHT_BROWSERS_PATH:-}" ]; then
  for browser_cache in \
    "$ROOT/Development/tools/playwright-browsers" \
    "$ROOT/../../tools/playwright-browsers"
  do
    if [ -d "$browser_cache" ]; then
      PLAYWRIGHT_BROWSERS_PATH="$(cd "$browser_cache" && pwd -P)"
      export PLAYWRIGHT_BROWSERS_PATH
      break
    fi
  done
fi

# Compile caching, shared across worktrees. A fresh checkout otherwise pays
# the full cold build on its first land (E42.6). Missing sccache changes
# nothing; OPTIMUS_VERIFY_NO_SCCACHE=1 opts a run out when the compiler
# itself is under suspicion.
if [ -z "${RUSTC_WRAPPER:-}" ] && [ -z "${OPTIMUS_VERIFY_NO_SCCACHE:-}" ]; then
  if command -v sccache >/dev/null 2>&1; then
    export RUSTC_WRAPPER=sccache
  fi
fi

if ! command -v tmux >/dev/null 2>&1; then
  for tmux_bin_dir in \
    "$ROOT/Development/tools/tmux-root/usr/bin" \
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

# seconds_since <start> — EPOCHREALTIME delta with one decimal, for the
# per-stage durations that make evidence logs answer "where does the wall
# clock go" without a profiler.
seconds_since() {
  awk -v a="$1" -v b="$EPOCHREALTIME" 'BEGIN { printf "%.1f", b - a }'
}

# run <name> <command...>
run() {
  local name="$1"; shift
  printf '%-46s' "  $name"
  local out started="$EPOCHREALTIME"
  if out=$("$@" 2>&1); then
    printf '%s (%ss)\n' "${G}ok${X}" "$(seconds_since "$started")"
    PASSED+=("$name")
  else
    printf '%s (%ss)\n' "${R}FAIL${X}" "$(seconds_since "$started")"
    FAILED+=("$name")
    printf '%s\n' "$out" | tail -15 | sed 's/^/      /'
    # Reported *and* returned, so a caller can decline to run the gate that
    # this step was preparing for. Nothing runs under `set -e`, so callers that
    # ignore the status behave exactly as they did before.
    return 1
  fi
}

# retry <name> <tries> <cmd...>: run a step up to <tries> times, reporting
# once. Guards the cargo steps against transient build-lock contention with
# concurrent sessions; a genuine failure fails every attempt. The command is
# executed in a subshell each attempt, so it must be an external command
# (or stateless shell function), not a stateful one.
retry() {
  local name="$1" tries="$2"; shift 2
  local out attempt=1 started="$EPOCHREALTIME"
  while true; do
    if out=$("$@" 2>&1); then
      if [ "$attempt" -eq 1 ]; then
        printf '%-46s%s (%ss)\n' "  $name" "${G}ok${X}" "$(seconds_since "$started")"
      else
        printf '%-46s%s (%ss, retry %s)\n' "  $name" "${G}ok${X}" "$(seconds_since "$started")" "$((attempt - 1))"
      fi
      PASSED+=("$name")
      return 0
    fi
    if [ "$attempt" -ge "$tries" ]; then
      printf '%-46s%s (%ss)\n' "  $name" "${R}FAIL${X}" "$(seconds_since "$started")"
      FAILED+=("$name")
      printf '%s\n' "$out" | tail -15 | sed 's/^/      /'
      return 1
    fi
    attempt=$((attempt + 1))
  done
}

# skip <name> <reason>
skip() {
  printf '%-46s%s\n' "  $1" "${Y}skip${X} ($2)"
  SKIPPED+=("$1")
  # The reason is already phrased as the missing prerequisite at every call
  # site, so keeping it alongside the name is all a strict caller needs to
  # explain what did not run and why.
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
    started="$EPOCHREALTIME"
    if out=$("$@" 2>&1); then
      printf 'ok' > "$JOBS_DIR/$slug.status"
    else
      printf 'fail' > "$JOBS_DIR/$slug.status"
    fi
    seconds_since "$started" > "$JOBS_DIR/$slug.duration"
    printf '%s' "$out" > "$JOBS_DIR/$slug.out"
  ) &
  SPAWNED+=("$PENDING_SECTION|$name")
  PENDING_SECTION=""
}

# spawn_dir <name> <dir> <shell-command>
# Background twin of `in_dir`: these suites resolve config and test globs
# relative to cwd, so each Bun command runs from its package directory.
spawn_dir() {
  local name="$1" dir="$2" cmd="$3"
  if [ ! -d "$dir/node_modules" ]; then
    skip "$name" "bun install in workspace root"
    return
  fi
  spawn "$name" bash -c "cd '$dir' && $cmd"
}

# reap — wait for every spawned gate, then report in spawn order.
reap() {
  [ "${#SPAWNED[@]}" -eq 0 ] && return 0
  wait
  local slug=0 entry name title status duration
  for entry in "${SPAWNED[@]}"; do
    title="${entry%%|*}"
    name="${entry#*|}"
    [ -n "$title" ] && section "$title"
    status="$(cat "$JOBS_DIR/$slug.status" 2>/dev/null || printf 'fail')"
    duration="$(cat "$JOBS_DIR/$slug.duration" 2>/dev/null || printf '?')"
    printf '%-46s' "  $name"
    if [ "$status" = "ok" ]; then
      printf '%s (%ss)\n' "${G}ok${X}" "$duration"
      PASSED+=("$name")
    else
      printf '%s (%ss)\n' "${R}FAIL${X}" "$duration"
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
  spawn "architecture-marks"         python3 scripts/gates/check-architecture-marks.py
  spawn "crate-layers"               python3 scripts/gates/check-crate-layers.py
  spawn "domain-modularity"          python3 scripts/gates/check-domain-modularity.py
  spawn "instruction-planes"         python3 scripts/gates/check-instruction-planes.py
  spawn "surface-contract"           python3 scripts/gates/check-surface-contract.py
  spawn "autonomy-profiles"          python3 scripts/gates/check-autonomy-profiles.py
  spawn "project-scope"              python3 scripts/gates/check-project-scope-assertions.py
  spawn "project-bleed"              python3 scripts/gates/check-project-bleed.py
  spawn "tool-coverage"              python3 scripts/gates/check-tool-coverage.py
  spawn "observability"              python3 scripts/gates/check-observability-gate.py
  spawn "module-size"                python3 scripts/gates/check-module-size.py
  spawn "product-complete-install"   python3 scripts/gates/check-product-complete-install.py
  spawn "parity-ledger"              python3 scripts/gates/check-parity-ledger.py
  spawn "neutral-fixtures"           python3 scripts/gates/check-neutral-fixtures.py
  spawn "version-validate"           python3 scripts/tools/optimus_version.py validate
  spawn "version-release-check"      python3 scripts/tools/optimus_version.py release-check
  spawn "lockfile-discipline"        python3 scripts/gates/check-lockfile-discipline.py
  spawn "token-budget"               python3 scripts/gates/check-repo-token-budget.py
  spawn "engineering-memory"         python3 scripts/tools/engineering_memory.py check
  spawn "engineering-memory-valid"   python3 scripts/tools/engineering_memory.py validate
  spawn "documentation-contract"     python3 scripts/tools/docs_system.py check
  spawn "repository-ontology"        python3 scripts/tools/repository_ontology.py check
  spawn "temporal-project-knowledge" python3 scripts/tools/project_knowledge.py check

  spawn_section "gate self-tests"
  spawn "test_architecture_marks"    python3 scripts/tests/test_architecture_marks.py
  spawn "test_surface_contract"      python3 scripts/tests/test_surface_contract.py
  spawn "test_engineering_memory"    python3 scripts/tests/test_engineering_memory.py
  spawn "test_docs_system"           python3 scripts/tests/test_docs_system.py
  spawn "test_repository_ontology"   python3 scripts/tests/test_repository_ontology.py
  spawn "test_project_knowledge"      python3 scripts/tests/test_project_knowledge.py
  spawn "test_managed_project_cleanup" python3 scripts/tests/test_managed_project_cleanup.py
  spawn "test_impact_select"         python3 scripts/tests/test_impact_select.py
  spawn "test_instruction_planes"    python3 scripts/tests/test_instruction_planes.py
  spawn "test_perf_harness"          python3 scripts/tests/test_perf_harness.py
  spawn "test_project_hygiene"       python3 scripts/tests/test_project_hygiene.py
  spawn "test_live_smoke"            python3 scripts/tests/test_live_smoke.py
  spawn "test_synthetic_user_lab"    python3 scripts/tests/test_synthetic_user_lab.py
  spawn "test_desktop_task_suite"    python3 scripts/tests/test_desktop_task_suite.py
  spawn "test_desktop_self_improvement_loop" python3 scripts/tests/test_desktop_self_improvement_loop.py
  spawn "test_synthetic_simulator"   python3 scripts/tests/test_synthetic_user_simulator.py
  spawn "test_tool_coverage_gate"    python3 scripts/tests/test_tool_coverage_gate.py
  spawn "test_module_size"           python3 scripts/tests/test_module_size.py
  spawn "test_autonomy_profiles"     python3 scripts/tests/test_autonomy_profiles.py
  spawn "test_optimus_version"       python3 scripts/tests/test_optimus_version.py
  spawn "test_rebuild_install"       python3 scripts/tests/test_rebuild_install_safety.py
  spawn "test_verify_skip_report"    python3 scripts/tests/test_verify_skip_report.py
  spawn "test_tui_feature_matrix"    python3 scripts/tests/test_tui_feature_matrix.py
  spawn "test_verify_gate_parity"    python3 scripts/tests/test_verify_gate_parity.py
  spawn "test_lockfile_discipline"   python3 scripts/tests/test_lockfile_discipline.py
  spawn "test_repo_token_budget"     python3 scripts/tests/test_repo_token_budget.py
  reap
}

# --- tier: check -------------------------------------------------------------
tier_check() {
  section "compile"
  # Clippy surfaces every compile error `cargo check` would, so hosts with
  # clippy installed skip the redundant check pass: both commands serialise on
  # the same target-dir lock, and the second pass was pure queue time. Hosts
  # without clippy keep the plain check so the compile gate never disappears.
  if cargo clippy --version >/dev/null 2>&1; then
    run "clippy" cargo clippy --workspace --all-targets -- -D warnings
  else
    run "cargo check" cargo check --workspace --all-targets
    skip "clippy" "not installed: rustup component add clippy"
  fi
}

# --- tier: test --------------------------------------------------------------
tier_test() {
  section "rust tests"
  # Retried once: a concurrent session's cargo build can hold the target-dir
  # lock briefly; a transient lock stall must not fail the gate (a genuine
  # test failure fails both attempts).
  if cargo nextest --version >/dev/null 2>&1; then
    retry "cargo nextest" 2 cargo nextest run --workspace
  else
    retry "cargo test" 2 cargo test --workspace --all-targets -- --test-threads=1
  fi
  # Pinned split suites (spec-015 A5): gate self-containment — the surface
  # protocol conformance suite and the serve capability probe run by pinned
  # command alongside the workspace tier (the double-run is harmless).
  run "surface-protocol conformance" cargo test -p optimus-host --test serve_protocol -- --test-threads=1
  run "serve capability probe" cargo test -p optimus-cli --test capability_probe
}

# --- tier: live --------------------------------------------------------------
# Real model, real credential, real cost — deliberately invoked, never implied
# by `all` (CI holds no credential, and C6 bans gates that quietly skip).
tier_live() {
  section "live model"
  if [ ! -x target/debug/optimus ]; then
    run "build optimus cli" cargo build -p optimus-cli
  fi
  run "live smoke (codex)" python3 scripts/tools/live_smoke.py
  # Browser success needs the public web (loopback is refused by the SSRF
  # law), so its dispatch-path test is network-marked and runs here.
  run "browser success (network)" \
    cargo test -p optimus-kernel --test tool_coverage -- --ignored
  # leg 3 — desktop face: playwright drives the REACT workbench over the
  # WS transport against a spawned `optimus serve` on the credentialed
  # home (the #82 contract: boots on codex, real model answers a nonce).
  # Run from the package cwd: playwright resolves its config from cwd
  # (see the ui-tier note).
  if [ ! -x target/debug/optimus ]; then
    run "build optimus cli" cargo build -p optimus-cli
  fi
  if [ ! -f apps/optimus-ui/dist/index.html ]; then
    run "build react ui" bun --cwd apps/optimus-ui run build
  fi
  run "live desktop (codex)" bash -c \
    'cd apps/optimus-desktop && OPTIMUS_E2E_HOME="${OPTIMUS_LIVE_HOME:-$HOME/.local/share/optimus}" bunx playwright test --config=playwright.live.config.js'
}

# --- tier: ui ----------------------------------------------------------------
# in_dir <name> <dir> <shell-command>
# Runs from <dir>. These suites resolve their config and test globs relative to
# the working directory, so each Bun command runs from the package cwd and
# Playwright cannot pick up another package's config.
in_dir() {
  local name="$1" dir="$2" cmd="$3"
  if [ ! -d "$dir/node_modules" ]; then
    skip "$name" "bun install in workspace root"
    return
  fi
  run "$name" bash -c "cd '$dir' && $cmd"
}

tier_ui() {
  # Unit suites share an initial phase. The browser-driven suites run one at a
  # time afterwards: they all drive Chromium/PTY scheduling, and running them
  # side by side can starve a renderer event loop long enough for a click to sit
  # unconsumed even though the identical flow is consistently sub-three-seconds
  # on an idle host.
  spawn_section "ui suites"
  spawn_dir "optimus-ui vitest" apps/optimus-ui       "bun run test"

  # The terminal face gets the same treatment as the desktop: the real binary,
  # driven end to end, deterministically (offline provider, temp home). These
  # three suites deliberately run one at a time. They all drive tmux event
  # loops and Chromium/PTY scheduling at once; parallelising them made a slow
  # host occasionally capture a pre-paint frame and report a false Unicode
  # layout failure.
  if command -v tmux >/dev/null 2>&1; then
    reap
    # The Tauri binary embeds the React bundle at compile time, so the UI
    # build must precede the shell build.
    run "build react ui" bun run --cwd apps/optimus-ui build
    run "build optimus cli" cargo build -p optimus-cli
    run "build tauri shell" cargo build -p optimus-tauri --features optimus-tauri/custom-protocol
    run "tui e2e" python3 scripts/tools/tui_e2e.py
    run "tui feature matrix" python3 scripts/tools/tui_feature_matrix.py
    # Launch acceptance for the Tauri desktop shell: supervised launch,
    # readiness marker, windowed surface, and stable process. Requires a
    # display like the TUI tiers above.
    if [ -n "${DISPLAY:-}" ] || [ -n "${WAYLAND_DISPLAY:-}" ] || command -v xvfb-run >/dev/null 2>&1; then
      if [ -z "${DISPLAY:-}" ] && [ -z "${WAYLAND_DISPLAY:-}" ]; then
        run "tauri launch acceptance" xvfb-run -a python3 scripts/gates/check-tauri-launch.py
      else
        run "tauri launch acceptance" python3 scripts/gates/check-tauri-launch.py
      fi
    else
      skip "tauri launch acceptance" "no display and no xvfb-run"
    fi
    # Self-development acceptance (spec-013): the real Developer Full Access
    # lifecycle — grant enable, supervisor build+launch, handoff snapshot,
    # failed-build preservation, restart, emergency stop, revoke — against
    # the host-only binary, and against a windowed Tauri child when a
    # display exists (same guard as the launch acceptance gate).
    if [ ! -x target/debug/optimus-desktop ]; then
      run "build self-development host" cargo build -p optimus-desktop
    fi
    run "self-development acceptance (host)" python3 scripts/tests/test_self_development.py
    if [ -n "${DISPLAY:-}" ] || [ -n "${WAYLAND_DISPLAY:-}" ] || command -v xvfb-run >/dev/null 2>&1; then
      if [ -z "${DISPLAY:-}" ] && [ -z "${WAYLAND_DISPLAY:-}" ]; then
        run "self-development acceptance (desktop)" xvfb-run -a python3 scripts/tests/test_self_development.py --surface desktop
      else
        run "self-development acceptance (desktop)" python3 scripts/tests/test_self_development.py --surface desktop
      fi
    else
      skip "self-development acceptance (desktop)" "no display and no xvfb-run"
    fi
    if [ -d node_modules ] && (bunx playwright --version >/dev/null 2>&1); then
      run "tui layout (playwright)" bun scripts/tests/tui_layout_playwright.cjs
    else
      skip "tui layout (playwright)" "bun install in workspace root"
    fi
    # Desktop task suite (easy tier) + self-improvement loop gates. Both
    # self-skip when their prerequisites (binary, websockets, display,
    # credentials, ollama) are absent, so they stay hermetic everywhere.
    if command -v xvfb-run >/dev/null 2>&1; then
      # Xvfb is the deterministic capture environment: the harness pins
      # GDK_BACKEND=x11 + software compositing, and ImageMagick `import`
      # grabs a real Xvfb root. Running natively on a Wayland session
      # (XWayland root) makes the capture backend fail (import can't grab).
      run "desktop task suite" xvfb-run -a env -u WAYLAND_DISPLAY python3 scripts/tools/desktop_task_suite.py --task easy
      run "desktop self-improvement loop" xvfb-run -a env -u WAYLAND_DISPLAY python3 scripts/tools/desktop_self_improvement_loop.py --iterations 1 --timeout 600
    elif [ -n "${DISPLAY:-}" ] || [ -n "${WAYLAND_DISPLAY:-}" ]; then
      run "desktop task suite" env -u WAYLAND_DISPLAY python3 scripts/tools/desktop_task_suite.py --task easy
      run "desktop self-improvement loop" env -u WAYLAND_DISPLAY python3 scripts/tools/desktop_self_improvement_loop.py --iterations 1 --timeout 600
    else
      skip "desktop task suite" "no display and no xvfb-run"
      skip "desktop self-improvement loop" "no display and no xvfb-run"
    fi

    # Geometry invariants for the React shell. Self-tests its own rules first,
    # so a rule that stops detecting its defect fails the gate rather than
    # reporting a clean shell.
    if [ -d node_modules ] && [ -f apps/optimus-ui/dist/index.html ]; then
      run "ui layout audit" node scripts/tests/ui_layout_audit.cjs
      # Same rules, the engine the product ships: WebKitGTK. Skips itself when
      # the introspection bindings are absent.
      run "ui layout audit (webkit)" python3 scripts/tools/ui_layout_audit_webkit.py
    else
      skip "ui layout audit" "bun install + bun run --cwd apps/optimus-ui build"
      skip "ui layout audit (webkit)" "bun install + bun run --cwd apps/optimus-ui build"
    fi
  else
    skip "tui e2e" "tmux not installed"
    skip "tui feature matrix" "tmux not installed"
    skip "tui layout (playwright)" "tmux not installed"
    skip "self-development acceptance (host)" "tmux not installed"
    skip "self-development acceptance (desktop)" "tmux not installed"
  fi

  # Playwright drives the real host binary (e2e/support.js spawns
  # target/debug/optimus serve per worker, and serves the built React
  # workbench dist), so it needs a *built* CLI and UI -- `cargo check` from
  # the compile tier does not produce them.
  local playwright_ready=1
  if [ ! -d apps/optimus-desktop/node_modules ]; then
    skip "playwright" "bun install in workspace root"
    playwright_ready=0
  elif ! (cd apps/optimus-desktop && bunx playwright --version >/dev/null 2>&1); then
    skip "playwright" "bunx playwright install chromium"
    playwright_ready=0
  fi

  if [ "$playwright_ready" = 1 ]; then
    run "build desktop host" cargo build -p optimus-cli
    run "build react ui" bun --cwd apps/optimus-ui run build
  fi
  [ "$playwright_ready" = 1 ] && spawn_dir "playwright" apps/optimus-desktop "bunx playwright test"
  reap
}

# --- tier: all ---------------------------------------------------------------
# Running every gate serially leaves most cores idle, while launching the Rust,
# browser, DOM, and real-terminal suites all at once can starve their
# event loops badly enough to manufacture input and timeout failures. Build the
# host first, run static/compile/Rust work as one parallel phase, then run the
# UI suites as a second parallel phase. Each phase still reaps in stable order.
#
# Concurrent cargo invocations serialise on the target-dir lock by themselves, so
# fmt/check/clippy/nextest queue rather than corrupt anything — they simply stop
# blocking the suites that have no cargo dependency.
tier_all() {
  # The one true ordering constraint: Playwright spawns this binary per worker.
  local host_built=0
  if [ -d apps/optimus-desktop/node_modules ]; then
    section "build"
    run "build desktop host" cargo build -p optimus-cli
    run "build react ui" bun --cwd apps/optimus-ui run build
    host_built=1
  fi

  spawn_section "gates"
  spawn "fmt"                        cargo fmt --all -- --check
  spawn "architecture-marks"         python3 scripts/gates/check-architecture-marks.py
  spawn "crate-layers"               python3 scripts/gates/check-crate-layers.py
  spawn "domain-modularity"          python3 scripts/gates/check-domain-modularity.py
  spawn "instruction-planes"         python3 scripts/gates/check-instruction-planes.py
  spawn "surface-contract"           python3 scripts/gates/check-surface-contract.py
  spawn "autonomy-profiles"          python3 scripts/gates/check-autonomy-profiles.py
  spawn "project-scope"              python3 scripts/gates/check-project-scope-assertions.py
  spawn "project-bleed"              python3 scripts/gates/check-project-bleed.py
  spawn "tool-coverage"              python3 scripts/gates/check-tool-coverage.py
  spawn "observability"              python3 scripts/gates/check-observability-gate.py
  spawn "module-size"                python3 scripts/gates/check-module-size.py
  spawn "product-complete-install"   python3 scripts/gates/check-product-complete-install.py
  spawn "parity-ledger"              python3 scripts/gates/check-parity-ledger.py
  spawn "neutral-fixtures"           python3 scripts/gates/check-neutral-fixtures.py
  spawn "version-validate"           python3 scripts/tools/optimus_version.py validate
  spawn "version-release-check"      python3 scripts/tools/optimus_version.py release-check
  spawn "lockfile-discipline"        python3 scripts/gates/check-lockfile-discipline.py
  spawn "token-budget"               python3 scripts/gates/check-repo-token-budget.py
  spawn "engineering-memory"         python3 scripts/tools/engineering_memory.py check
  spawn "engineering-memory-valid"   python3 scripts/tools/engineering_memory.py validate
  spawn "documentation-contract"     python3 scripts/tools/docs_system.py check
  spawn "repository-ontology"        python3 scripts/tools/repository_ontology.py check
  spawn "temporal-project-knowledge" python3 scripts/tools/project_knowledge.py check

  spawn_section "gate self-tests"
  spawn "test_architecture_marks"    python3 scripts/tests/test_architecture_marks.py
  spawn "test_surface_contract"      python3 scripts/tests/test_surface_contract.py
  spawn "test_engineering_memory"    python3 scripts/tests/test_engineering_memory.py
  spawn "test_docs_system"           python3 scripts/tests/test_docs_system.py
  spawn "test_repository_ontology"   python3 scripts/tests/test_repository_ontology.py
  spawn "test_project_knowledge"      python3 scripts/tests/test_project_knowledge.py
  spawn "test_managed_project_cleanup" python3 scripts/tests/test_managed_project_cleanup.py
  spawn "test_impact_select"         python3 scripts/tests/test_impact_select.py
  spawn "test_instruction_planes"    python3 scripts/tests/test_instruction_planes.py
  spawn "test_perf_harness"          python3 scripts/tests/test_perf_harness.py
  spawn "test_project_hygiene"       python3 scripts/tests/test_project_hygiene.py
  spawn "test_live_smoke"            python3 scripts/tests/test_live_smoke.py
  spawn "test_synthetic_user_lab"    python3 scripts/tests/test_synthetic_user_lab.py
  spawn "test_desktop_task_suite"    python3 scripts/tests/test_desktop_task_suite.py
  spawn "test_desktop_self_improvement_loop" python3 scripts/tests/test_desktop_self_improvement_loop.py
  spawn "test_synthetic_simulator"   python3 scripts/tests/test_synthetic_user_simulator.py
  spawn "test_tool_coverage_gate"    python3 scripts/tests/test_tool_coverage_gate.py
  spawn "test_module_size"           python3 scripts/tests/test_module_size.py
  spawn "test_autonomy_profiles"     python3 scripts/tests/test_autonomy_profiles.py
  spawn "test_optimus_version"       python3 scripts/tests/test_optimus_version.py
  spawn "test_rebuild_install"       python3 scripts/tests/test_rebuild_install_safety.py
  spawn "test_verify_skip_report"    python3 scripts/tests/test_verify_skip_report.py
  spawn "test_tui_feature_matrix"    python3 scripts/tests/test_tui_feature_matrix.py
  spawn "test_verify_gate_parity"    python3 scripts/tests/test_verify_gate_parity.py
  spawn "test_lockfile_discipline"   python3 scripts/tests/test_lockfile_discipline.py
  spawn "test_repo_token_budget"     python3 scripts/tests/test_repo_token_budget.py

  spawn_section "compile"
  # Same rationale as tier_check: clippy subsumes cargo check, and both queue
  # on the target-dir lock — one compile pass instead of two.
  if cargo clippy --version >/dev/null 2>&1; then
    spawn "clippy" cargo clippy --workspace --all-targets -- -D warnings
  else
    spawn "cargo check" cargo check --workspace --all-targets
    skip "clippy" "not installed: rustup component add clippy"
  fi

  spawn_section "rust tests"
  if cargo nextest --version >/dev/null 2>&1; then
    spawn "cargo nextest" cargo nextest run --workspace
  else
    spawn "cargo test" cargo test --workspace --all-targets -- --test-threads=1
  fi

  # Keep interactive/event-loop suites out of the CPU-heavy compile phase.
  # This is a synchronization boundary, not a reduced gate: PASSED/FAILED and
  # skip evidence accumulate across both reaps into the same final summary.
  reap

  spawn_section "ui suites"
  spawn_dir "optimus-ui vitest" apps/optimus-ui       "bun run test"

  # Terminal face, same standard as the desktop: real binary, deterministic
  # pty. Reap the browser/unit jobs first, then keep all three terminal suites
  # serial for the same tmux/event-loop reason as tier_ui above.
  if command -v tmux >/dev/null 2>&1; then
    reap
    # The Tauri binary embeds the React bundle at compile time, so the UI
    # build must precede the shell build.
    run "build react ui" bun run --cwd apps/optimus-ui build
    run "build optimus cli" cargo build -p optimus-cli
    run "build tauri shell" cargo build -p optimus-tauri --features optimus-tauri/custom-protocol
    run "tui e2e" python3 scripts/tools/tui_e2e.py
    run "tui feature matrix" python3 scripts/tools/tui_feature_matrix.py
    # Launch acceptance for the Tauri desktop shell: supervised launch,
    # readiness marker, windowed surface, and stable process. Requires a
    # display like the TUI tiers above.
    if [ -n "${DISPLAY:-}" ] || [ -n "${WAYLAND_DISPLAY:-}" ] || command -v xvfb-run >/dev/null 2>&1; then
      if [ -z "${DISPLAY:-}" ] && [ -z "${WAYLAND_DISPLAY:-}" ]; then
        run "tauri launch acceptance" xvfb-run -a python3 scripts/gates/check-tauri-launch.py
      else
        run "tauri launch acceptance" python3 scripts/gates/check-tauri-launch.py
      fi
    else
      skip "tauri launch acceptance" "no display and no xvfb-run"
    fi
    # Self-development acceptance (spec-013): the real Developer Full Access
    # lifecycle — grant enable, supervisor build+launch, handoff snapshot,
    # failed-build preservation, restart, emergency stop, revoke — against
    # the host-only binary, and against a windowed Tauri child when a
    # display exists (same guard as the launch acceptance gate).
    if [ ! -x target/debug/optimus-desktop ]; then
      run "build self-development host" cargo build -p optimus-desktop
    fi
    run "self-development acceptance (host)" python3 scripts/tests/test_self_development.py
    if [ -n "${DISPLAY:-}" ] || [ -n "${WAYLAND_DISPLAY:-}" ] || command -v xvfb-run >/dev/null 2>&1; then
      if [ -z "${DISPLAY:-}" ] && [ -z "${WAYLAND_DISPLAY:-}" ]; then
        run "self-development acceptance (desktop)" xvfb-run -a python3 scripts/tests/test_self_development.py --surface desktop
      else
        run "self-development acceptance (desktop)" python3 scripts/tests/test_self_development.py --surface desktop
      fi
    else
      skip "self-development acceptance (desktop)" "no display and no xvfb-run"
    fi
    if [ -d node_modules ] && (bunx playwright --version >/dev/null 2>&1); then
      run "tui layout (playwright)" bun scripts/tests/tui_layout_playwright.cjs
    else
      skip "tui layout (playwright)" "bun install in workspace root"
    fi
    # Desktop task suite (easy tier) + self-improvement loop gates. Both
    # self-skip when their prerequisites (binary, websockets, display,
    # credentials, ollama) are absent, so they stay hermetic everywhere.
    if command -v xvfb-run >/dev/null 2>&1; then
      # Xvfb is the deterministic capture environment: the harness pins
      # GDK_BACKEND=x11 + software compositing, and ImageMagick `import`
      # grabs a real Xvfb root. Running natively on a Wayland session
      # (XWayland root) makes the capture backend fail (import can't grab).
      run "desktop task suite" xvfb-run -a env -u WAYLAND_DISPLAY python3 scripts/tools/desktop_task_suite.py --task easy
      run "desktop self-improvement loop" xvfb-run -a env -u WAYLAND_DISPLAY python3 scripts/tools/desktop_self_improvement_loop.py --iterations 1 --timeout 600
    elif [ -n "${DISPLAY:-}" ] || [ -n "${WAYLAND_DISPLAY:-}" ]; then
      run "desktop task suite" env -u WAYLAND_DISPLAY python3 scripts/tools/desktop_task_suite.py --task easy
      run "desktop self-improvement loop" env -u WAYLAND_DISPLAY python3 scripts/tools/desktop_self_improvement_loop.py --iterations 1 --timeout 600
    else
      skip "desktop task suite" "no display and no xvfb-run"
      skip "desktop self-improvement loop" "no display and no xvfb-run"
    fi

    # Geometry invariants for the React shell. Self-tests its own rules first,
    # so a rule that stops detecting its defect fails the gate rather than
    # reporting a clean shell.
    if [ -d node_modules ] && [ -f apps/optimus-ui/dist/index.html ]; then
      run "ui layout audit" node scripts/tests/ui_layout_audit.cjs
      # Same rules, the engine the product ships: WebKitGTK. Skips itself when
      # the introspection bindings are absent.
      run "ui layout audit (webkit)" python3 scripts/tools/ui_layout_audit_webkit.py
    else
      skip "ui layout audit" "bun install + bun run --cwd apps/optimus-ui build"
      skip "ui layout audit (webkit)" "bun install + bun run --cwd apps/optimus-ui build"
    fi
  else
    skip "tui e2e" "tmux not installed"
    skip "tui feature matrix" "tmux not installed"
    skip "tui layout (playwright)" "tmux not installed"
    skip "self-development acceptance (host)" "tmux not installed"
    skip "self-development acceptance (desktop)" "tmux not installed"
  fi

  if [ "$host_built" = 1 ] && (cd apps/optimus-desktop && bunx playwright --version >/dev/null 2>&1); then
    spawn_dir "playwright" apps/optimus-desktop "bunx playwright test"
  else
    skip "playwright" "bun install + bunx playwright install chromium"
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

# Hand the skip list to whoever invoked us. Managed land forbids skips; local
# callers may render the missing prerequisites. Nothing changes when unset.
if [ -n "${OPTIMUS_VERIFY_SKIP_REPORT:-}" ]; then
  : >"$OPTIMUS_VERIFY_SKIP_REPORT"
  [ "${#SKIPPED_DETAIL[@]}" -gt 0 ] &&
    printf '%s\n' "${SKIPPED_DETAIL[@]}" >"$OPTIMUS_VERIFY_SKIP_REPORT"
fi

if [ "${#FAILED[@]}" -gt 0 ]; then
  printf '\n  failed: %s\n\n' "${FAILED[*]}"
  exit 1
fi

# Skip-is-failure mode. Locally a skip identifies a missing development
# prerequisite; managed land sets OPTIMUS_VERIFY_FORBID_SKIPS=1 so a gate that
# silently did not run can never become delivery evidence.
if [ -n "${OPTIMUS_VERIFY_FORBID_SKIPS:-}" ] && [ "${#SKIPPED[@]}" -gt 0 ]; then
  printf '\n  skipped (forbidden by OPTIMUS_VERIFY_FORBID_SKIPS): %s\n\n' "${SKIPPED[*]}"
  exit 1
fi
printf '\n'
