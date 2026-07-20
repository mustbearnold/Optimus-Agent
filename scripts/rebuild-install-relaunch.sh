#!/usr/bin/env bash
# Rebuild → unsigned local Windows install → relaunch
# Wrapper so Git Bash / Hermes terminal can invoke the canonical PowerShell installer.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PS1="$ROOT/scripts/rebuild-install-relaunch.ps1"

# Normalize MSYS path for PowerShell when needed
if command -v cygpath >/dev/null 2>&1; then
  PS1_WIN="$(cygpath -w "$PS1")"
  ROOT_WIN="$(cygpath -w "$ROOT")"
else
  PS1_WIN="$PS1"
  ROOT_WIN="$ROOT"
fi

export TEMP="${TEMP:-C:/Users/mustb/AppData/Local/Temp}"
export TMP="${TMP:-$TEMP}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/local/tmp/cargo-target}"

CONFIG="release"
NO_BUILD=""
NO_RELAUNCH=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --dev) CONFIG="dev"; shift ;;
    --release) CONFIG="release"; shift ;;
    --no-build) NO_BUILD="-NoBuild"; shift ;;
    --no-relaunch) NO_RELAUNCH="-NoRelaunch"; shift ;;
    -h|--help)
      echo "Usage: $0 [--dev|--release] [--no-build] [--no-relaunch]"
      exit 0
      ;;
    *) echo "Unknown arg: $1" >&2; exit 2 ;;
  esac
done

exec powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$PS1_WIN" \
  -Configuration "$CONFIG" \
  -RepoRoot "$ROOT_WIN" \
  $NO_BUILD \
  $NO_RELAUNCH
