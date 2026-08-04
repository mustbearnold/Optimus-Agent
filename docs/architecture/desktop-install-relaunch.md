---
doc_id: architecture-desktop-install-relaunch
doc_type: explanation
plane: current
status: current
authority: supporting
summary: Confirmed current behaviour: the installer stages Tauri + React as the desktop entry and keeps Wry as the legacy rollback shell.
reviewed_on: 2026-07-31
review_by: 2026-10-31
---

# Desktop install and relaunch convention

## Default shell

**Confirmed current behaviour:** the installer stages **Tauri + React** as the
desktop entry and retains the **Wry/Tao** binary as a legacy
rollback (`LegacyWry`). The Rust host remains authority for IPC and durable
effects. See
[desktop-shell-and-ipc-matrix.md](../contracts/desktop-shell-and-ipc-matrix.md).

**Program P29:** there is **no in-app auto-updater** (ADR-0043). Upgrade path is
re-run `scripts/rebuild-install-relaunch.sh`. Doctor reports
`updater_channel: "none"`.

## Rule

After rebuilding the host and Tauri app, install and relaunch the stable user
copy before calling the desktop ready.

Do not leave the user on an older binary from a Cargo target directory.

## Ubuntu command

From the repository root:

```bash
bash scripts/rebuild-install-relaunch.sh          # release, install, relaunch
bash scripts/rebuild-install-relaunch.sh --dev    # debug, install, relaunch
```

Other modes:

```bash
bash scripts/rebuild-install-relaunch.sh --no-build
bash scripts/rebuild-install-relaunch.sh --no-relaunch
bash scripts/rebuild-install-relaunch.sh --dev --no-build --no-relaunch
```

The script is a native Linux workflow. It:

1. Checks Cargo, GTK 3, WebKitGTK 4.1, and build prerequisites.
2. Builds `optimus-tauri`, the rollback host, and `optimus-cli`.
3. Stops only an Optimus process running from the stable install path.
4. Atomically replaces both installed binaries.
5. Creates CLI symlinks, an XDG desktop entry, and an SVG icon.
6. Writes version and install metadata.
7. Writes an ownership marker and generates a marker-gated scoped uninstaller.
8. Relaunches the installed desktop and verifies its executable path through `/proc`.

Application installation does not use `sudo`.

## Ubuntu prerequisites

```bash
sudo apt-get update
sudo apt-get install -y \
  bubblewrap \
  build-essential \
  pkg-config \
  libgtk-3-dev \
  libwebkit2gtk-4.1-dev \
  libxdo-dev \
  desktop-file-utils
```

Playwright verification uses Bun and Chromium:

```bash
cd apps/optimus-desktop
bun install --frozen-lockfile
bunx playwright install chromium
```

## Ubuntu install layout

Default paths:

```text
~/.local/share/optimus-agent/
  .optimus-agent-install
  bin/
    optimus-desktop
    optimus-agent-tauri
    optimus
    optimus-cli -> optimus
  VERSION.txt
  install-meta.json
  uninstall.sh
  README-INSTALL.txt
  optimus-desktop.log

~/.local/share/applications/optimus-agent.desktop
~/.local/share/icons/hicolor/scalable/apps/optimus-agent.svg
~/.local/bin/optimus -> ~/.local/share/optimus-agent/bin/optimus
~/.local/bin/optimus-cli -> ~/.local/share/optimus-agent/bin/optimus
```

The application state remains separate under the platform-local Optimus data directory. On Ubuntu that defaults to `~/.local/share/optimus`.

## Ubuntu environment overrides

The installer honors:

| Variable | Default | Purpose |
|---|---|---|
| `CARGO_TARGET_DIR` | `${XDG_CACHE_HOME:-~/.cache}/optimus-agent/cargo-target` | Cargo output outside the repository |
| `XDG_DATA_HOME` | `~/.local/share` | XDG data, application, and icon root |
| `XDG_BIN_HOME` | `~/.local/bin` | User CLI symlink directory |
| `OPTIMUS_INSTALL_ROOT` | `$XDG_DATA_HOME/optimus-agent` | Stable application install root |

These overrides also make isolated installer testing possible without touching the live desktop menu.

## Ubuntu verification

After installation:

```bash
desktop-file-validate ~/.local/share/applications/optimus-agent.desktop
~/.local/share/optimus-agent/bin/optimus-desktop --version
~/.local/share/optimus-agent/bin/optimus --version
readlink -f ~/.local/bin/optimus
pgrep -a optimus-desktop
```

To confirm the running process came from the stable install:

```bash
pid="$(pgrep -x optimus-desktop | head -n 1)"
readlink -f "/proc/$pid/exe"
```

Expected path:

```text
/home/<user>/.local/share/optimus-agent/bin/optimus-desktop
```

Startup output is appended to:

```text
~/.local/share/optimus-agent/optimus-desktop.log
```

## Ubuntu uninstall

```bash
~/.local/share/optimus-agent/uninstall.sh
```

The installer refuses a foreign non-empty install root, non-Optimus CLI links,
and symlinked desktop-entry or icon destinations. The generated uninstaller
requires the Optimus ownership marker, stops only the installed desktop binary,
removes Optimus-owned symlinks, removes the desktop entry and icon, then removes
the stable install root. It does not delete application state from
`~/.local/share/optimus`.

## Windows

PowerShell remains the canonical Windows installer:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\rebuild-install-relaunch.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\rebuild-install-relaunch.ps1 -Configuration dev
```

Git Bash, MSYS, and Cygwin can use the shared Bash entry point. It detects Windows and delegates to PowerShell:

```bash
bash scripts/rebuild-install-relaunch.sh
bash scripts/rebuild-install-relaunch.sh --dev
```

The Windows install remains under:

```text
%LOCALAPPDATA%\Programs\OptimusAgent\
```

The default Windows Cargo target is outside the repository at
`%LOCALAPPDATA%\OptimusAgent\cargo-target`. The installer rejects reparse points
in existing install, Start Menu, Desktop, and target-path components. Binary and
metadata publication uses random same-directory `CreateNew` files, verifies each
temporary has one hard link, then performs a non-overwriting rename. Existing
shortcuts are replaced or removed only when their resolved target is the owned
installed desktop binary.

## Agent checklist

When changing Desktop UI, IPC, kernel chat paths, or native runtime behavior:

1. Run focused tests.
2. Run the relevant Rust and Playwright suites.
3. Run `scripts/rebuild-install-relaunch.sh` or the PowerShell equivalent.
4. Confirm the running executable is the stable install path.
5. Only then report the desktop ready for use.
