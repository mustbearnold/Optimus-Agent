#!/usr/bin/env bash
# Rebuild Optimus Agent, install it for the current user, and relaunch it.
#
# Linux: native XDG user install (no sudo).
# Windows under Git Bash/MSYS: delegates to the canonical PowerShell installer.
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HOST_OS="$(uname -s)"

step() {
  printf '\n==> %s\n' "$*"
}

fail() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

usage() {
  cat <<'USAGE'
Usage: scripts/rebuild-install-relaunch.sh [options]

Options:
  --dev           Build/install the debug profile
  --release       Build/install the release profile (default)
  --no-build      Install an already-built profile
  --no-relaunch   Install without launching the desktop app
  -h, --help      Show this help

Linux environment overrides:
  CARGO_TARGET_DIR      Cargo output directory
  XDG_DATA_HOME         XDG data root (default: ~/.local/share)
  XDG_BIN_HOME          User executable directory (default: ~/.local/bin)
  OPTIMUS_INSTALL_ROOT  Stable install root (default: $XDG_DATA_HOME/optimus-agent)
USAGE
}

# Preserve the established Windows workflow for Git Bash/MSYS users.
case "$HOST_OS" in
  MINGW*|MSYS*|CYGWIN*)
    PS1="$ROOT/scripts/rebuild-install-relaunch.ps1"
    if command -v cygpath >/dev/null 2>&1; then
      PS1_WIN="$(cygpath -w "$PS1")"
      ROOT_WIN="$(cygpath -w "$ROOT")"
      DEFAULT_TARGET="$(cygpath -w "$ROOT/local/tmp/cargo-target")"
    else
      PS1_WIN="$PS1"
      ROOT_WIN="$ROOT"
      DEFAULT_TARGET="$ROOT/local/tmp/cargo-target"
    fi
    export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$DEFAULT_TARGET}"

    CONFIG="release"
    NO_BUILD=""
    NO_RELAUNCH=""
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --dev) CONFIG="dev"; shift ;;
        --release) CONFIG="release"; shift ;;
        --no-build) NO_BUILD="-NoBuild"; shift ;;
        --no-relaunch) NO_RELAUNCH="-NoRelaunch"; shift ;;
        -h|--help) usage; exit 0 ;;
        *) printf 'Unknown argument: %s\n' "$1" >&2; usage >&2; exit 2 ;;
      esac
    done

    exec powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$PS1_WIN" \
      -Configuration "$CONFIG" \
      -RepoRoot "$ROOT_WIN" \
      $NO_BUILD \
      $NO_RELAUNCH
    ;;
esac

[[ "$HOST_OS" == Linux* ]] || fail "native install is currently supported on Linux and Windows, not $HOST_OS"

PROFILE="release"
NO_BUILD=false
NO_RELAUNCH=false
while [[ $# -gt 0 ]]; do
  case "$1" in
    --dev) PROFILE="dev"; shift ;;
    --release) PROFILE="release"; shift ;;
    --no-build) NO_BUILD=true; shift ;;
    --no-relaunch) NO_RELAUNCH=true; shift ;;
    -h|--help) usage; exit 0 ;;
    *) printf 'Unknown argument: %s\n' "$1" >&2; usage >&2; exit 2 ;;
  esac
done

PROFILE_DIR="release"
[[ "$PROFILE" == "dev" ]] && PROFILE_DIR="debug"

require_command() {
  command -v "$1" >/dev/null 2>&1 || fail "required command not found: $1"
}

require_command readlink

assert_no_symlink_components() {
  local path="$1" label="$2" current="/" component
  local -a components=()
  IFS='/' read -r -a components <<<"${path#/}"
  for component in "${components[@]}"; do
    [[ -n "$component" ]] || continue
    [[ "$component" != ".." ]] || fail "refusing parent traversal in $label: $path"
    [[ "$component" != "." ]] || continue
    current="${current%/}/$component"
    [[ ! -L "$current" ]] || fail "refusing symlinked $label component: $current"
    if [[ "$current" != "$path" && -e "$current" && ! -d "$current" ]]; then
      fail "$label component is not a directory: $current"
    fi
  done
}

default_cache_home="${XDG_CACHE_HOME:-$HOME/.cache}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$default_cache_home/optimus-agent/cargo-target}"
if [[ "$CARGO_TARGET_DIR" != /* ]]; then
  export CARGO_TARGET_DIR="$ROOT/$CARGO_TARGET_DIR"
fi

raw_data_home="${XDG_DATA_HOME:-$HOME/.local/share}"
raw_bin_home="${XDG_BIN_HOME:-$HOME/.local/bin}"
raw_install_root="${OPTIMUS_INSTALL_ROOT:-$raw_data_home/optimus-agent}"
raw_electron_dist="${OPTIMUS_ELECTRON_DIST:-$ROOT/apps/optimus-electron/node_modules/electron/dist}"
raw_electron_app_source="${OPTIMUS_ELECTRON_APP_SOURCE:-$ROOT/apps/optimus-electron}"
raw_ui_dist="${OPTIMUS_UI_DIST:-$ROOT/apps/optimus-ui/dist}"
for path_value in \
  "$CARGO_TARGET_DIR" \
  "$raw_data_home" \
  "$raw_bin_home" \
  "$raw_install_root" \
  "$raw_electron_dist" \
  "$raw_electron_app_source" \
  "$raw_ui_dist"; do
  [[ "$path_value" == /* ]] || fail "Linux install paths must be absolute: $path_value"
  [[ "$path_value" != *$'\n'* && "$path_value" != *$'\r'* && "$path_value" != *$'\t'* ]] \
    || fail "Linux install paths must not contain control characters"
done

validate_lexical_install_paths() {
  assert_no_symlink_components "$raw_data_home" "data home"
  assert_no_symlink_components "$raw_data_home/applications" "applications directory"
  assert_no_symlink_components "$raw_data_home/icons/hicolor/scalable/apps" "icon directory"
  assert_no_symlink_components "$raw_bin_home" "binary home"
  assert_no_symlink_components "$raw_install_root" "install root"
  assert_no_symlink_components "$raw_install_root/bin" "install bin directory"
  assert_no_symlink_components "$raw_install_root/app-bundle" "installed Electron bundle"
  assert_no_symlink_components "$raw_install_root/.optimus-agent-install" "install marker"
}

validate_lexical_install_paths
CARGO_TARGET_DIR="$(readlink -m -- "$CARGO_TARGET_DIR")"
export CARGO_TARGET_DIR
DATA_HOME="$(readlink -m -- "$raw_data_home")"
BIN_HOME="$(readlink -m -- "$raw_bin_home")"
INSTALL_ROOT="$(readlink -m -- "$raw_install_root")"
ELECTRON_DIST_SOURCE="$(readlink -m -- "$raw_electron_dist")"
ELECTRON_APP_SOURCE="$(readlink -m -- "$raw_electron_app_source")"
UI_DIST_SOURCE="$(readlink -m -- "$raw_ui_dist")"
HOME_ROOT="$(readlink -m -- "$HOME")"
APPLICATIONS_DIR="$DATA_HOME/applications"
ICON_DIR="$DATA_HOME/icons/hicolor/scalable/apps"
DESKTOP_FILE="$APPLICATIONS_DIR/optimus-agent.desktop"
ICON_FILE="$ICON_DIR/optimus-agent.svg"
INSTALLED_DESKTOP="$INSTALL_ROOT/bin/optimus-desktop"
INSTALLED_HOST="$INSTALL_ROOT/bin/optimus-desktop-host"
INSTALLED_CLI="$INSTALL_ROOT/bin/optimus"
APP_BUNDLE="$INSTALL_ROOT/app-bundle"
INSTALLED_ELECTRON="$APP_BUNDLE/electron/optimus-agent"
INSTALLED_ELECTRON_APP="$APP_BUNDLE/electron/resources/app"
INSTALLED_UI_DIST="$INSTALLED_ELECTRON_APP/ui-dist"
INSTALL_MARKER="$INSTALL_ROOT/.optimus-agent-install"
INSTALL_MARKER_PREFIX="optimus-agent-user-install-v1"
INSTALL_MARKER_VALUE=""
EXISTING_INSTALL_OWNED=false
BUILD_DIR="$CARGO_TARGET_DIR/$PROFILE_DIR"
BUILT_DESKTOP="$BUILD_DIR/optimus-desktop"
BUILT_CLI="$BUILD_DIR/optimus"

[[ -f "$ROOT/Cargo.toml" ]] || fail "repository root not found: $ROOT"
[[ "$INSTALL_ROOT" != "/" && "$INSTALL_ROOT" != "$HOME_ROOT" ]] || fail "unsafe install root: $INSTALL_ROOT"

directory_has_entries() (
  shopt -s nullglob dotglob
  local entries=("$1"/*)
  (( ${#entries[@]} > 0 ))
)

directory_has_entries_other_than() (
  shopt -s nullglob dotglob
  local directory="$1" allowed="${2:-}" entry
  for entry in "$directory"/*; do
    [[ -n "$allowed" && "$entry" == "$allowed" ]] && continue
    return 0
  done
  return 1
)

is_known_legacy_install() {
  [[ -x "$INSTALLED_DESKTOP" && -f "$INSTALL_ROOT/install-meta.json" && -f "$INSTALL_ROOT/VERSION.txt" ]] \
    || return 1
  local metadata
  metadata="$(<"$INSTALL_ROOT/install-meta.json")"
  [[ "$metadata" == *'"name": "Optimus Agent"'* ]]
}

validate_install_root_ownership() {
  [[ ! -L "$INSTALL_ROOT" ]] || fail "refusing symlinked install root: $INSTALL_ROOT"
  [[ ! -e "$INSTALL_ROOT" || -d "$INSTALL_ROOT" ]] \
    || fail "install root exists and is not a directory: $INSTALL_ROOT"
  [[ ! -L "$INSTALL_ROOT/bin" ]] || fail "refusing symlinked install bin directory: $INSTALL_ROOT/bin"
  [[ ! -L "$INSTALL_MARKER" ]] || fail "refusing symlinked install marker: $INSTALL_MARKER"
  if [[ -f "$INSTALL_MARKER" ]]; then
    local marker_value
    marker_value="$(<"$INSTALL_MARKER")"
    [[ "$marker_value" == "$INSTALL_MARKER_PREFIX" || "$marker_value" == "$INSTALL_MARKER_PREFIX":* ]] \
      || fail "install root has an unrecognized ownership marker: $INSTALL_ROOT"
    EXISTING_INSTALL_OWNED=true
  elif [[ -d "$INSTALL_ROOT" ]] && directory_has_entries "$INSTALL_ROOT"; then
    if directory_has_entries_other_than "$INSTALL_ROOT" "${BUNDLE_STAGE:-}"; then
      is_known_legacy_install \
        || fail "refusing non-empty install root not owned by Optimus: $INSTALL_ROOT"
      EXISTING_INSTALL_OWNED=true
    fi
  fi
}

assert_replaceable_cli_link() {
  local link="$1" target
  if [[ -e "$link" || -L "$link" ]]; then
    [[ -L "$link" ]] || fail "refusing to replace non-symlink CLI path: $link"
    target="$(readlink -m -- "$link")"
    [[ "$target" == "$INSTALLED_CLI" || "$target" == "$INSTALL_ROOT"/bin/* ]] \
      || fail "refusing to replace foreign CLI symlink: $link -> $target"
  fi
}

assert_regular_destination() {
  local path="$1"
  [[ ! -L "$path" ]] || fail "refusing symlink destination: $path"
  [[ ! -d "$path" ]] || fail "refusing directory destination: $path"
  if [[ -e "$path" && "$EXISTING_INSTALL_OWNED" != true ]]; then
    fail "refusing to replace external file without an owned Optimus install: $path"
  fi
}

validate_install_root_ownership
assert_replaceable_cli_link "$BIN_HOME/optimus"
assert_replaceable_cli_link "$BIN_HOME/optimus-cli"
assert_regular_destination "$DESKTOP_FILE"
assert_regular_destination "$ICON_FILE"

if [[ -r /proc/sys/kernel/random/uuid ]]; then
  INSTALL_ID="$(< /proc/sys/kernel/random/uuid)"
else
  require_command sha256sum
  INSTALL_ID="$(printf '%s:%s:%s\n' "$$" "$(date +%s%N)" "$RANDOM" | sha256sum)"
  INSTALL_ID="${INSTALL_ID%% *}"
fi
INSTALL_MARKER_VALUE="$INSTALL_MARKER_PREFIX:$INSTALL_ID"

if [[ "$NO_BUILD" == false ]]; then
  require_command cargo
  require_command node
  require_command npm
  require_command pkg-config
  # auto_generate_cdp (via headless_chrome) invokes rustfmt from its build
  # script. Resolve the real toolchain binary because the rustup proxy can be
  # present on PATH yet unavailable to a nested Cargo build script.
  if [[ -z "${RUSTFMT:-}" ]]; then
    if command -v rustup >/dev/null 2>&1; then
      RUSTFMT="$(rustup which rustfmt 2>/dev/null || true)"
    fi
    [[ -n "${RUSTFMT:-}" ]] || RUSTFMT="$(command -v rustfmt 2>/dev/null || true)"
  elif [[ "$RUSTFMT" != /* ]]; then
    RUSTFMT="$(command -v "$RUSTFMT" 2>/dev/null || true)"
  fi
  [[ -n "${RUSTFMT:-}" && -x "$RUSTFMT" ]] \
    || fail "rustfmt is required for release dependencies; run: rustup component add rustfmt"
  export RUSTFMT
  missing_modules=()
  for module in gtk+-3.0 webkit2gtk-4.1; do
    pkg-config --exists "$module" || missing_modules+=("$module")
  done
  if (( ${#missing_modules[@]} > 0 )); then
    printf 'Missing Linux desktop development modules: %s\n' "${missing_modules[*]}" >&2
    printf 'Install them on Ubuntu with:\n' >&2
    printf '  sudo apt install bubblewrap build-essential pkg-config libgtk-3-dev libwebkit2gtk-4.1-dev libxdo-dev\n' >&2
    exit 1
  fi
fi

require_command bwrap
require_command cp
require_command find
require_command install
require_command sha256sum
require_command mktemp
require_command stat
require_command python3

validate_electron_sources() {
  local required
  [[ -d "$ELECTRON_DIST_SOURCE" && ! -L "$ELECTRON_DIST_SOURCE" ]] \
    || fail "Electron runtime directory missing or symlinked: $ELECTRON_DIST_SOURCE"
  [[ -x "$ELECTRON_DIST_SOURCE/electron" ]] \
    || fail "Electron runtime executable missing: $ELECTRON_DIST_SOURCE/electron"
  [[ -f "$ELECTRON_DIST_SOURCE/version" && ! -L "$ELECTRON_DIST_SOURCE/version" ]] \
    || fail "Electron runtime version file missing: $ELECTRON_DIST_SOURCE/version"
  [[ ! -e "$ELECTRON_DIST_SOURCE/resources/app" && ! -e "$ELECTRON_DIST_SOURCE/resources/app.asar" ]] \
    || fail "Electron runtime source already contains an application payload"
  [[ -d "$UI_DIST_SOURCE" && ! -L "$UI_DIST_SOURCE" && -f "$UI_DIST_SOURCE/index.html" ]] \
    || fail "React production assets missing: $UI_DIST_SOURCE"
  for required in package.json main.cjs preload.cjs browser-policy.cjs runtime-paths.cjs host-discovery.cjs; do
    [[ -f "$ELECTRON_APP_SOURCE/$required" && ! -L "$ELECTRON_APP_SOURCE/$required" ]] \
      || fail "Electron application file missing or symlinked: $ELECTRON_APP_SOURCE/$required"
  done
  if [[ -n "$(find "$ELECTRON_DIST_SOURCE" "$UI_DIST_SOURCE" -type l -print -quit)" ]]; then
    fail "Electron package sources must not contain symlinks"
  fi
}

file_sha256() {
  local path="$1" hash
  read -r hash _ < <(sha256sum -- "$path")
  [[ "$hash" =~ ^[0-9a-f]{64}$ ]] || fail "could not hash file: $path"
  printf '%s\n' "$hash"
}

new_destination_temp() {
  local destination="$1" directory basename
  directory="${destination%/*}"
  basename="${destination##*/}"
  mktemp --tmpdir="$directory" ".$basename.optimus-tmp.XXXXXXXX"
}

publish_temp_file() {
  local temporary="$1" destination="$2" mode="$3" links
  links="$(stat -c '%h' -- "$temporary")"
  if [[ "$links" != "1" ]]; then
    rm -f -- "$temporary"
    fail "refusing hard-linked temporary file for $destination"
  fi
  chmod "$mode" "$temporary"
  if ! mv -fT -- "$temporary" "$destination"; then
    rm -f -- "$temporary"
    fail "could not publish $destination"
  fi
}

atomic_install_file() {
  local source="$1" destination="$2" mode="$3" expected_hash="${4:-}" temporary actual_hash
  temporary="$(new_destination_temp "$destination")"
  if ! install -m "$mode" -- "$source" "$temporary"; then
    rm -f -- "$temporary"
    fail "could not stage $destination"
  fi
  if [[ -n "$expected_hash" ]]; then
    actual_hash="$(file_sha256 "$temporary")"
    if [[ "$actual_hash" != "$expected_hash" ]]; then
      rm -f -- "$temporary"
      fail "staged bytes for $destination do not match the validated artifact"
    fi
  fi
  publish_temp_file "$temporary" "$destination" "$mode"
}

atomic_write_file() {
  local destination="$1" mode="$2" temporary
  temporary="$(new_destination_temp "$destination")"
  if ! cat >"$temporary"; then
    rm -f -- "$temporary"
    fail "could not stage $destination"
  fi
  publish_temp_file "$temporary" "$destination" "$mode"
}

tree_sha256() {
  python3 - "$1" <<'PY'
import hashlib
import os
from pathlib import Path
import sys

root = Path(sys.argv[1])
digest = hashlib.sha256()
for path in sorted(candidate for candidate in root.rglob("*") if candidate.is_file()):
    relative = path.relative_to(root).as_posix().encode("utf-8")
    digest.update(len(relative).to_bytes(8, "big"))
    digest.update(relative)
    digest.update((path.stat().st_mode & 0o777).to_bytes(4, "big"))
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
print(digest.hexdigest())
PY
}

BUNDLE_STAGE=""
BUNDLE_BACKUP=""

cleanup_install_staging() {
  local candidate
  for candidate in "$BUNDLE_STAGE" "$BUNDLE_BACKUP"; do
    [[ -n "$candidate" && "$candidate" == "$INSTALL_ROOT"/.app-bundle.* ]] || continue
    [[ -d "$candidate" && ! -L "$candidate" ]] && rm -rf -- "$candidate"
  done
}
trap cleanup_install_staging EXIT

stage_electron_bundle() {
  local app_destination electron_destination
  mkdir -p "$INSTALL_ROOT"
  BUNDLE_STAGE="$(mktemp -d "$INSTALL_ROOT/.app-bundle.stage.XXXXXXXX")"
  electron_destination="$BUNDLE_STAGE/electron"
  app_destination="$electron_destination/resources/app"

  cp -a -- "$ELECTRON_DIST_SOURCE" "$electron_destination"
  [[ -x "$electron_destination/electron" ]] \
    || fail "staged Electron runtime is missing its executable"
  mv -- "$electron_destination/electron" "$electron_destination/optimus-agent"
  mkdir -p "$app_destination/ui-dist"
  for file in package.json main.cjs preload.cjs browser-policy.cjs runtime-paths.cjs host-discovery.cjs; do
    install -m 0644 -- "$ELECTRON_APP_SOURCE/$file" "$app_destination/$file"
  done
  cp -a -- "$UI_DIST_SOURCE/." "$app_destination/ui-dist/"
  [[ -x "$electron_destination/optimus-agent" && -f "$app_destination/ui-dist/index.html" ]] \
    || fail "staged Electron application is incomplete"
  BUNDLE_SHA256="$(tree_sha256 "$BUNDLE_STAGE")"
  [[ "$BUNDLE_SHA256" =~ ^[0-9a-f]{64}$ ]] || fail "could not hash staged Electron bundle"
}

publish_electron_bundle() {
  [[ -n "$BUNDLE_STAGE" && -d "$BUNDLE_STAGE" && ! -L "$BUNDLE_STAGE" ]] \
    || fail "Electron bundle was not staged"
  [[ ! -L "$APP_BUNDLE" ]] || fail "refusing symlinked installed Electron bundle"
  if [[ -e "$APP_BUNDLE" ]]; then
    [[ -d "$APP_BUNDLE" ]] || fail "installed Electron bundle is not a directory"
    BUNDLE_BACKUP="$INSTALL_ROOT/.app-bundle.backup.$INSTALL_ID"
    [[ ! -e "$BUNDLE_BACKUP" ]] || fail "stale Electron bundle backup exists"
    mv -- "$APP_BUNDLE" "$BUNDLE_BACKUP"
  fi
  if ! mv -- "$BUNDLE_STAGE" "$APP_BUNDLE"; then
    if [[ -n "$BUNDLE_BACKUP" && -d "$BUNDLE_BACKUP" && ! -e "$APP_BUNDLE" ]]; then
      mv -- "$BUNDLE_BACKUP" "$APP_BUNDLE" || true
      BUNDLE_BACKUP=""
    fi
    fail "could not publish installed Electron bundle"
  fi
  BUNDLE_STAGE=""
  [[ "$(tree_sha256 "$APP_BUNDLE")" == "$BUNDLE_SHA256" ]] \
    || fail "installed Electron bundle does not match the validated staged bytes"
  if [[ -n "$BUNDLE_BACKUP" ]]; then
    rm -rf -- "$BUNDLE_BACKUP"
    BUNDLE_BACKUP=""
  fi
}

process_start_time() {
  local pid="$1" stat_line rest fields=()
  [[ -r "/proc/$pid/stat" ]] || return 1
  stat_line="$(<"/proc/$pid/stat")"
  rest="${stat_line##*) }"
  read -r -a fields <<<"$rest"
  (( ${#fields[@]} > 19 )) || return 1
  printf '%s\n' "${fields[19]}"
}

process_is_installed_electron_main() {
  local pid="$1" exe argument
  exe="$(readlink -f "/proc/$pid/exe" 2>/dev/null || true)"
  [[ "$exe" == "$INSTALLED_ELECTRON" ]] || return 1
  while IFS= read -r -d '' argument; do
    [[ "$argument" == --type=* ]] && return 1
  done <"/proc/$pid/cmdline"
  return 0
}

process_is_installed_app() {
  local pid="$1" exe
  exe="$(readlink -f "/proc/$pid/exe" 2>/dev/null || true)"
  [[ "$exe" == "$INSTALLED_DESKTOP" || "$exe" == "$INSTALLED_HOST" ]] \
    && return 0
  process_is_installed_electron_main "$pid"
}

process_matches_identity() {
  local token="$1" pid expected_start current_start
  [[ "$token" == *:* ]] || return 1
  pid="${token%%:*}"
  expected_start="${token#*:}"
  [[ "$pid" =~ ^[0-9]+$ && "$expected_start" =~ ^[0-9]+$ ]] || return 1
  current_start="$(process_start_time "$pid" 2>/dev/null || true)"
  [[ "$current_start" == "$expected_start" ]] || return 1
  process_is_installed_app "$pid"
}

find_installed_pids() {
  local proc pid start_time
  for proc in /proc/[0-9]*; do
    [[ -d "$proc" ]] || continue
    pid="${proc#/proc/}"
    start_time="$(process_start_time "$pid" 2>/dev/null || true)"
    [[ "$start_time" =~ ^[0-9]+$ ]] || continue
    process_is_installed_app "$pid" \
      && printf '%s:%s\n' "$pid" "$start_time"
  done
}

find_installed_electron_pids() {
  local proc pid start_time
  for proc in /proc/[0-9]*; do
    [[ -d "$proc" ]] || continue
    pid="${proc#/proc/}"
    start_time="$(process_start_time "$pid" 2>/dev/null || true)"
    [[ "$start_time" =~ ^[0-9]+$ ]] || continue
    process_is_installed_electron_main "$pid" \
      && printf '%s:%s\n' "$pid" "$start_time"
  done
}

stop_installed_desktop() {
  local identities=() token pid alive
  mapfile -t identities < <(find_installed_pids)
  (( ${#identities[@]} == 0 )) && return 0

  step "Stopping installed Optimus desktop"
  printf '  pids:'
  for token in "${identities[@]}"; do printf ' %s' "${token%%:*}"; done
  printf '\n'
  for token in "${identities[@]}"; do
    if process_matches_identity "$token"; then
      pid="${token%%:*}"
      kill "$pid" 2>/dev/null || true
    fi
  done
  for _ in {1..30}; do
    alive=false
    for token in "${identities[@]}"; do
      if process_matches_identity "$token"; then alive=true; fi
    done
    [[ "$alive" == false ]] && return 0
    sleep 0.1
  done
  for token in "${identities[@]}"; do
    if process_matches_identity "$token"; then
      pid="${token%%:*}"
      kill -KILL "$pid" 2>/dev/null || true
    fi
  done
  for _ in {1..20}; do
    alive=false
    for token in "${identities[@]}"; do
      if process_matches_identity "$token"; then alive=true; fi
    done
    [[ "$alive" == false ]] && return 0
    sleep 0.1
  done
  fail "installed desktop did not stop cleanly"
}

printf 'Optimus Agent - native local install\n'
printf '  repo:     %s\n' "$ROOT"
printf '  profile:  %s\n' "$PROFILE"
printf '  target:   %s\n' "$CARGO_TARGET_DIR"
printf '  install:  %s\n' "$INSTALL_ROOT"

check_release_policy() {
  local label="$1" parity_status_json
  local -a parity_fields=()
  step "$label"
  (cd "$ROOT" && python3 scripts/optimus_version.py release-check)
  parity_status_json="$(cd "$ROOT" && python3 scripts/optimus_version.py status --json)"
  mapfile -t parity_fields < <(
    printf '%s' "$parity_status_json" | python3 -c '
import json, sys
status = json.load(sys.stdin)
print(status["product_version"])
print(status["hermes_target_version"])
print(status["hermes_parity_version"] or "")
print(status["claim_status"])
print(status["features"]["total"])
'
  )
  (( ${#parity_fields[@]} == 5 )) || fail "could not read parity version metadata"
  PRODUCT_VERSION="${parity_fields[0]}"
  HERMES_TARGET_VERSION="${parity_fields[1]}"
  HERMES_PARITY_VERSION="${parity_fields[2]}"
  HERMES_PARITY_STATUS="${parity_fields[3]}"
  HERMES_FEATURE_CONTRACTS="${parity_fields[4]}"
}

verify_built_versions() {
  version_line="$("$BUILT_DESKTOP" --version)"
  version="${version_line##* }"
  [[ "$version" == "$PRODUCT_VERSION" ]] \
    || fail "built desktop version $version does not match policy version $PRODUCT_VERSION"
  cli_version_line="$("$BUILT_CLI" --version)"
  cli_version="${cli_version_line##* }"
  [[ "$cli_version" == "$PRODUCT_VERSION" ]] \
    || fail "built CLI version $cli_version does not match policy version $PRODUCT_VERSION"
}

check_release_policy "Checking Optimus/Hermes version policy"

if [[ "$NO_BUILD" == false ]]; then
  step "Building optimus-desktop and optimus-cli ($PROFILE)"
  build_args=(build -p optimus-desktop -p optimus-cli)
  [[ "$PROFILE" == "release" ]] && build_args+=(--release)
  (cd "$ROOT" && cargo "${build_args[@]}")
  step "Building React production assets"
  (cd "$ROOT" && npm --prefix apps/optimus-ui run build)
fi

[[ -x "$BUILT_DESKTOP" ]] || fail "missing built binary: $BUILT_DESKTOP"
if [[ ! -x "$BUILT_CLI" && -x "$BUILD_DIR/optimus-cli" ]]; then
  BUILT_CLI="$BUILD_DIR/optimus-cli"
fi
[[ -x "$BUILT_CLI" ]] || fail "missing built binary: $BUILT_CLI"

verify_built_versions
BUILT_DESKTOP_SHA256="$(file_sha256 "$BUILT_DESKTOP")"
BUILT_CLI_SHA256="$(file_sha256 "$BUILT_CLI")"
validate_electron_sources
ELECTRON_VERSION="$(tr -d '\r\n' <"$ELECTRON_DIST_SOURCE/version")"
[[ "$ELECTRON_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+ ]] \
  || fail "invalid Electron runtime version: $ELECTRON_VERSION"
ELECTRON_APP_VERSION="$(
  python3 -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["version"])' \
    "$ELECTRON_APP_SOURCE/package.json"
)"
[[ "$ELECTRON_APP_VERSION" == "$PRODUCT_VERSION" ]] \
  || fail "Electron application version $ELECTRON_APP_VERSION does not match policy version $PRODUCT_VERSION"
step "Staging self-contained Electron application"
stage_electron_bundle

# The build may take minutes. Revalidate the ownership boundary immediately
# before stopping or replacing the installed application.
check_release_policy "Rechecking Optimus/Hermes version policy"
verify_built_versions
[[ "$(file_sha256 "$BUILT_DESKTOP")" == "$BUILT_DESKTOP_SHA256" ]] \
  || fail "built desktop changed after version validation"
[[ "$(file_sha256 "$BUILT_CLI")" == "$BUILT_CLI_SHA256" ]] \
  || fail "built CLI changed after version validation"
validate_lexical_install_paths
validate_install_root_ownership
assert_replaceable_cli_link "$BIN_HOME/optimus"
assert_replaceable_cli_link "$BIN_HOME/optimus-cli"
assert_regular_destination "$DESKTOP_FILE"
assert_regular_destination "$ICON_FILE"
stop_installed_desktop
installed_at="$(date -u +'%Y-%m-%dT%H:%M:%SZ')"

step "Installing binaries"
mkdir -p "$INSTALL_ROOT/bin" "$APPLICATIONS_DIR" "$ICON_DIR" "$BIN_HOME"
atomic_install_file "$BUILT_DESKTOP" "$INSTALLED_HOST" 0755 "$BUILT_DESKTOP_SHA256"
atomic_install_file "$BUILT_CLI" "$INSTALLED_CLI" 0755 "$BUILT_CLI_SHA256"
ln -sfn optimus "$INSTALL_ROOT/bin/optimus-cli"
ln -sfn "$INSTALLED_CLI" "$BIN_HOME/optimus"
ln -sfn "$INSTALLED_CLI" "$BIN_HOME/optimus-cli"
publish_electron_bundle
printf '  %s\n' "$INSTALLED_HOST"
printf '  %s\n' "$INSTALLED_ELECTRON"
printf '  %s\n' "$INSTALLED_CLI"

atomic_install_file "$ROOT/assets/optimus-agent.svg" "$ICON_FILE" 0644

printf -v q_install_root '%q' "$INSTALL_ROOT"
printf -v q_installed_host '%q' "$INSTALLED_HOST"
printf -v q_installed_electron '%q' "$INSTALLED_ELECTRON"
printf -v q_installed_electron_app '%q' "$INSTALLED_ELECTRON_APP"
printf -v q_installed_ui_dist '%q' "$INSTALLED_UI_DIST"
printf -v q_default_optimus_home '%q' "$DATA_HOME/optimus"
printf -v q_product_version '%q' "$version"
atomic_write_file "$INSTALLED_DESKTOP" 0755 <<EOF
#!/usr/bin/env bash
set -Eeuo pipefail
INSTALL_ROOT=$q_install_root
HOST_BINARY=$q_installed_host
ELECTRON_BINARY=$q_installed_electron
ELECTRON_APP=$q_installed_electron_app
UI_DIST=$q_installed_ui_dist
DEFAULT_OPTIMUS_HOME=$q_default_optimus_home
PRODUCT_VERSION=$q_product_version

case "\${1:-}" in
  --version|-V)
    printf 'optimus-desktop %s\\n' "\$PRODUCT_VERSION"
    exit 0
    ;;
esac

case "\${OPTIMUS_DESKTOP_SHELL:-electron}" in
  electron)
    [[ -x "\$ELECTRON_BINARY" && -f "\$ELECTRON_APP/main.cjs" && -f "\$ELECTRON_APP/host-discovery.cjs" && -f "\$UI_DIST/index.html" ]] || {
      printf 'Installed Optimus Electron application is incomplete: %s\\n' "\$INSTALL_ROOT" >&2
      exit 1
    }
    export OPTIMUS_APP_ROOT="\$INSTALL_ROOT"
    export OPTIMUS_DESKTOP_BIN="\$HOST_BINARY"
    export OPTIMUS_UI_DIST="\$UI_DIST"
    export OPTIMUS_ELECTRON_UI="\${OPTIMUS_ELECTRON_UI:-react}"
    export OPTIMUS_HOME="\${OPTIMUS_HOME:-\$DEFAULT_OPTIMUS_HOME}"
    export OPTIMUS_ELECTRON_USER_DATA="\${OPTIMUS_ELECTRON_USER_DATA:-\$OPTIMUS_HOME/electron}"
    # Coding agents and some shells export ELECTRON_RUN_AS_NODE=1, which turns the
    # packaged binary into plain Node and breaks GUI launch (and rejects Chromium
    # flags such as --class). Always clear it for the desktop shell.
    unset ELECTRON_RUN_AS_NODE
    # User-local installs cannot root-own chrome-sandbox (mode 4755). Without that,
    # Chromium aborts; disable the SUID helper so namespace sandboxing can proceed.
    electron_dir="\$(dirname -- "\$ELECTRON_BINARY")"
    if [[ ! -u "\$electron_dir/chrome-sandbox" ]]; then
      export ELECTRON_DISABLE_SANDBOX=1
    fi
    exec >>"\$INSTALL_ROOT/optimus-desktop.log" 2>&1
    # Do not pass --class: with RUN_AS_NODE or Node-first argv parsing it is rejected.
    # WM class comes from the binary name (optimus-agent) and StartupWMClass.
    exec "\$ELECTRON_BINARY" "\$@"
    ;;
  wry)
    exec "\$HOST_BINARY" "\$@"
    ;;
  *)
    printf 'Unknown OPTIMUS_DESKTOP_SHELL: %s\\n' "\$OPTIMUS_DESKTOP_SHELL" >&2
    exit 2
    ;;
esac
EOF

desktop_quote() {
  local value="$1"
  value="${value//\\/\\\\}"
  value="${value//\"/\\\"}"
  value="${value//\`/\\\`}"
  value="${value//\$/\\$}"
  value="${value//%/%%}"
  printf '"%s"' "$value"
}

atomic_write_file "$DESKTOP_FILE" 0644 <<EOF
[Desktop Entry]
Version=1.0
Type=Application
Name=Optimus Agent
GenericName=AI Agent Desktop
Comment=Durable local AI agent workspace
Exec=$(desktop_quote "$INSTALLED_DESKTOP")
TryExec=$INSTALLED_DESKTOP
Icon=optimus-agent
Terminal=false
Categories=Development;
Keywords=AI;Agent;Assistant;Automation;
StartupNotify=true
StartupWMClass=optimus-agent
X-Optimus-Install-ID=$INSTALL_ID
X-Optimus-UI=react-electron
Actions=OpenData;LegacyWry;

[Desktop Action OpenData]
Name=Open Optimus data folder
Exec=xdg-open $(desktop_quote "$DATA_HOME/optimus")

[Desktop Action LegacyWry]
Name=Launch legacy Wry shell
Exec=env OPTIMUS_DESKTOP_SHELL=wry $(desktop_quote "$INSTALLED_DESKTOP")
EOF

atomic_write_file "$INSTALL_ROOT/VERSION.txt" 0644 <<EOF
Optimus Agent $version
profile=$PROFILE
shell=react-electron
electron=$ELECTRON_VERSION
installed=$installed_at
source=$ROOT
platform=linux-$(uname -m)
hermes_target=$HERMES_TARGET_VERSION
hermes_parity=${HERMES_PARITY_VERSION:-unverified}
hermes_parity_status=$HERMES_PARITY_STATUS
hermes_feature_contracts=$HERMES_FEATURE_CONTRACTS
EOF

json_escape() {
  local value="$1"
  value="${value//\\/\\\\}"
  value="${value//\"/\\\"}"
  value="${value//$'\n'/\\n}"
  printf '%s' "$value"
}

if [[ -n "$HERMES_PARITY_VERSION" ]]; then
  hermes_parity_json="\"$(json_escape "$HERMES_PARITY_VERSION")\""
else
  hermes_parity_json="null"
fi

atomic_write_file "$INSTALL_ROOT/install-meta.json" 0644 <<EOF
{
  "name": "Optimus Agent",
  "version": "$(json_escape "$version")",
  "hermes_target_version": "$(json_escape "$HERMES_TARGET_VERSION")",
  "hermes_parity_version": $hermes_parity_json,
  "hermes_parity_status": "$(json_escape "$HERMES_PARITY_STATUS")",
  "hermes_feature_contracts": $HERMES_FEATURE_CONTRACTS,
  "configuration": "$(json_escape "$PROFILE")",
  "desktop_shell": "react-electron",
  "electron_version": "$(json_escape "$ELECTRON_VERSION")",
  "installed_at": "$(json_escape "$installed_at")",
  "install_id": "$(json_escape "$INSTALL_ID")",
  "install_root": "$(json_escape "$INSTALL_ROOT")",
  "source_repo": "$(json_escape "$ROOT")",
  "cargo_target": "$(json_escape "$CARGO_TARGET_DIR")",
  "desktop_binary": "$(json_escape "$INSTALLED_DESKTOP")",
  "host_binary": "$(json_escape "$INSTALLED_HOST")",
  "electron_binary": "$(json_escape "$INSTALLED_ELECTRON")",
  "electron_app": "$(json_escape "$INSTALLED_ELECTRON_APP")",
  "ui_dist": "$(json_escape "$INSTALLED_UI_DIST")",
  "bundle_sha256": "$(json_escape "$BUNDLE_SHA256")",
  "cli_binary": "$(json_escape "$INSTALLED_CLI")",
  "desktop_entry": "$(json_escape "$DESKTOP_FILE")"
}
EOF

printf '%s\n' "$INSTALL_MARKER_VALUE" | atomic_write_file "$INSTALL_MARKER" 0644

desktop_file_hash="$(sha256sum -- "$DESKTOP_FILE")"
desktop_file_hash="${desktop_file_hash%% *}"
icon_file_hash="$(sha256sum -- "$ICON_FILE")"
icon_file_hash="${icon_file_hash%% *}"
printf -v q_install_root '%q' "$INSTALL_ROOT"
printf -v q_install_marker '%q' "$INSTALL_MARKER"
printf -v q_install_marker_value '%q' "$INSTALL_MARKER_VALUE"
printf -v q_desktop '%q' "$INSTALLED_DESKTOP"
printf -v q_host '%q' "$INSTALLED_HOST"
printf -v q_electron '%q' "$INSTALLED_ELECTRON"
printf -v q_desktop_file '%q' "$DESKTOP_FILE"
printf -v q_icon_file '%q' "$ICON_FILE"
printf -v q_desktop_file_hash '%q' "$desktop_file_hash"
printf -v q_icon_file_hash '%q' "$icon_file_hash"
printf -v q_bin_optimus '%q' "$BIN_HOME/optimus"
printf -v q_bin_cli '%q' "$BIN_HOME/optimus-cli"
printf -v q_applications '%q' "$APPLICATIONS_DIR"
uninstall_temp="$(new_destination_temp "$INSTALL_ROOT/uninstall.sh")"
cat >"$uninstall_temp" <<EOF
#!/usr/bin/env bash
set -Eeuo pipefail
INSTALL_ROOT=$q_install_root
INSTALL_MARKER=$q_install_marker
INSTALL_MARKER_VALUE=$q_install_marker_value
DESKTOP_BINARY=$q_desktop
HOST_BINARY=$q_host
ELECTRON_BINARY=$q_electron
DESKTOP_FILE=$q_desktop_file
ICON_FILE=$q_icon_file
DESKTOP_FILE_HASH=$q_desktop_file_hash
ICON_FILE_HASH=$q_icon_file_hash
BIN_OPTIMUS=$q_bin_optimus
BIN_CLI=$q_bin_cli
APPLICATIONS_DIR=$q_applications
EOF
cat >>"$uninstall_temp" <<'EOF'
[[ -n "$INSTALL_ROOT" && "$INSTALL_ROOT" != "/" && "$INSTALL_ROOT" != "$HOME" ]] || {
  printf 'Refusing unsafe uninstall root: %s\n' "$INSTALL_ROOT" >&2
  exit 1
}
resolved_root="$(readlink -m -- "$INSTALL_ROOT")"
[[ "$resolved_root" == "$INSTALL_ROOT" && ! -L "$INSTALL_MARKER" && -f "$INSTALL_MARKER" ]] || {
  printf 'Refusing unowned or non-canonical uninstall root: %s\n' "$INSTALL_ROOT" >&2
  exit 1
}
[[ "$(<"$INSTALL_MARKER")" == "$INSTALL_MARKER_VALUE" ]] || {
  printf 'Refusing stale or invalid install ownership marker: %s\n' "$INSTALL_ROOT" >&2
  exit 1
}

process_start_time() {
  local pid="$1" stat_line rest fields=()
  [[ -r "/proc/$pid/stat" ]] || return 1
  stat_line="$(<"/proc/$pid/stat")"
  rest="${stat_line##*) }"
  read -r -a fields <<<"$rest"
  (( ${#fields[@]} > 19 )) || return 1
  printf '%s\n' "${fields[19]}"
}

process_is_electron_main() {
  local pid="$1" exe argument
  exe="$(readlink -f "/proc/$pid/exe" 2>/dev/null || true)"
  [[ "$exe" == "$ELECTRON_BINARY" ]] || return 1
  while IFS= read -r -d '' argument; do
    [[ "$argument" == --type=* ]] && return 1
  done <"/proc/$pid/cmdline"
  return 0
}

process_is_installed_app() {
  local pid="$1" exe
  exe="$(readlink -f "/proc/$pid/exe" 2>/dev/null || true)"
  [[ "$exe" == "$HOST_BINARY" ]] && return 0
  process_is_electron_main "$pid"
}

process_matches_identity() {
  local token="$1" pid expected_start current_start
  [[ "$token" == *:* ]] || return 1
  pid="${token%%:*}"
  expected_start="${token#*:}"
  current_start="$(process_start_time "$pid" 2>/dev/null || true)"
  [[ "$pid" =~ ^[0-9]+$ && "$current_start" == "$expected_start" ]] || return 1
  process_is_installed_app "$pid"
}

stop_desktop() {
  local identities=() proc pid start token alive
  for proc in /proc/[0-9]*; do
    [[ -d "$proc" ]] || continue
    pid="${proc#/proc/}"
    start="$(process_start_time "$pid" 2>/dev/null || true)"
    [[ "$start" =~ ^[0-9]+$ ]] || continue
    process_is_installed_app "$pid" && identities+=("$pid:$start")
  done
  for token in "${identities[@]}"; do
    process_matches_identity "$token" && kill "${token%%:*}" 2>/dev/null || true
  done
  for _ in {1..30}; do
    alive=false
    for token in "${identities[@]}"; do
      process_matches_identity "$token" && alive=true
    done
    [[ "$alive" == false ]] && return 0
    sleep 0.1
  done
  for token in "${identities[@]}"; do
    process_matches_identity "$token" && kill -KILL "${token%%:*}" 2>/dev/null || true
  done
  for _ in {1..20}; do
    alive=false
    for token in "${identities[@]}"; do
      process_matches_identity "$token" && alive=true
    done
    [[ "$alive" == false ]] && return 0
    sleep 0.1
  done
  printf 'Refusing uninstall while Optimus desktop is still running\n' >&2
  return 1
}

file_matches_hash() {
  local path="$1" expected="$2" actual
  command -v sha256sum >/dev/null 2>&1 || return 1
  [[ -f "$path" && ! -L "$path" ]] || return 1
  actual="$(sha256sum -- "$path")"
  actual="${actual%% *}"
  [[ "$actual" == "$expected" ]]
}

remove_owned_file() {
  local path="$1" expected="$2" label="$3"
  [[ -e "$path" || -L "$path" ]] || return 0
  if file_matches_hash "$path" "$expected"; then
    rm -f -- "$path"
  else
    printf 'Preserving modified or replaced %s: %s\n' "$label" "$path" >&2
  fi
}

stop_desktop
for link in "$BIN_OPTIMUS" "$BIN_CLI"; do
  if [[ -L "$link" ]]; then
    target="$(readlink -m -- "$link")"
    if [[ "$target" == "$INSTALL_ROOT/bin/optimus" ]]; then
      rm -f -- "$link"
    else
      printf 'Preserving replaced CLI link: %s\n' "$link" >&2
    fi
  elif [[ -e "$link" ]]; then
    printf 'Preserving non-symlink CLI path: %s\n' "$link" >&2
  fi
done
remove_owned_file "$DESKTOP_FILE" "$DESKTOP_FILE_HASH" 'desktop entry'
remove_owned_file "$ICON_FILE" "$ICON_FILE_HASH" 'icon'
rm -rf -- "$INSTALL_ROOT"
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$APPLICATIONS_DIR" >/dev/null 2>&1 || true
fi
printf 'Uninstalled Optimus Agent from %s\n' "$INSTALL_ROOT"
EOF
publish_temp_file "$uninstall_temp" "$INSTALL_ROOT/uninstall.sh" 0755

atomic_write_file "$INSTALL_ROOT/README-INSTALL.txt" 0644 <<EOF
Optimus Agent - native Linux user install
==========================================
Install root: $INSTALL_ROOT
Desktop entry: $DESKTOP_FILE
Desktop shell: React + Electron $ELECTRON_VERSION
Rust host: $INSTALLED_HOST

Launch:
  - Open the application menu and choose Optimus Agent
  - $INSTALLED_DESKTOP

Legacy Wry rollback:
  OPTIMUS_DESKTOP_SHELL=wry $INSTALLED_DESKTOP

CLI:
  $BIN_HOME/optimus --help

Uninstall:
  $INSTALL_ROOT/uninstall.sh

Rebuild + reinstall + relaunch from the repository:
  bash scripts/rebuild-install-relaunch.sh
EOF

if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$APPLICATIONS_DIR" >/dev/null 2>&1 || true
fi

printf '  desktop entry: %s\n' "$DESKTOP_FILE"
printf '  icon:          %s\n' "$ICON_FILE"
printf '  version:       %s\n' "$version"
if [[ ":$PATH:" != *":$BIN_HOME:"* ]]; then
  printf '  note: add %s to PATH for the optimus CLI symlinks\n' "$BIN_HOME"
fi

launch_installed() {
  local log_file="$INSTALL_ROOT/optimus-desktop.log"
  local log_offset=0
  step "Launching installed Optimus desktop"
  [[ -f "$log_file" ]] && log_offset="$(stat -c '%s' "$log_file" 2>/dev/null || printf 0)"
  printf '[optimus-installer] launch %s\n' "$INSTALL_ID" >>"$log_file"
  if command -v setsid >/dev/null 2>&1; then
    (cd "$INSTALL_ROOT" && setsid -f "$INSTALLED_DESKTOP" >>"$log_file" 2>&1)
  else
    (cd "$INSTALL_ROOT" && nohup "$INSTALLED_DESKTOP" >>"$log_file" 2>&1 &)
  fi

  local identity="" pid=""
  for _ in {1..100}; do
    identity="$(find_installed_electron_pids | head -n 1 || true)"
    if [[ -n "$identity" ]]; then
      pid="${identity%%:*}"
      break
    fi
    sleep 0.1
  done
  if [[ -z "$pid" ]]; then
    printf 'Installed app did not stay running. Log: %s\n' "$log_file" >&2
    [[ -f "$log_file" ]] && tail -n 30 "$log_file" >&2
    return 1
  fi
  local ready=false
  for _ in {1..150}; do
    if tail -c "+$((log_offset + 1))" "$log_file" 2>/dev/null \
      | grep -Fq '[optimus-electron] ready ui=react'; then
      ready=true
      break
    fi
    process_is_installed_electron_main "$pid" || break
    sleep 0.1
  done
  if [[ "$ready" != true ]]; then
    printf 'Installed Electron shell did not report ready. Log: %s\n' "$log_file" >&2
    tail -n 40 "$log_file" >&2
    return 1
  fi
  printf '  running pid=%s path=%s\n' "$pid" "$(readlink -f "/proc/$pid/exe")"
}

if [[ "$NO_RELAUNCH" == false ]]; then
  launch_installed
fi

printf '\nDone. Optimus Agent %s (%s) is installed at:\n  %s\n' "$version" "$PROFILE" "$INSTALL_ROOT"
