# Desktop install / relaunch convention

## Rule

**After every rebuild of `optimus-desktop`, run the unsigned local install + relaunch.**

Do not leave the user on a stale `cargo-target/debug` binary from a previous session.

## Canonical command

From repo root (Git Bash / Hermes):

```bash
export CARGO_TARGET_DIR="E:/Projects/Optimus Agent/local/tmp/cargo-target"
export TEMP=C:/Users/mustb/AppData/Local/Temp
export TMP=C:/Users/mustb/AppData/Local/Temp
bash scripts/rebuild-install-relaunch.sh          # release + install + relaunch
bash scripts/rebuild-install-relaunch.sh --dev    # faster debug install + relaunch
```

PowerShell:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\rebuild-install-relaunch.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\rebuild-install-relaunch.ps1 -Configuration dev
```

## Install layout (unsigned)

```text
%LOCALAPPDATA%\Programs\OptimusAgent\
  optimus-desktop.exe
  optimus.exe              # CLI
  optimus-cli.exe          # alias copy of optimus.exe
  VERSION.txt
  install-meta.json
  uninstall.ps1
  README-INSTALL.txt
```

Shortcuts:

- Start Menu → **Optimus Agent**
- Desktop → **Optimus Agent**

## Agent checklist

When you change desktop/UI/kernel chat paths:

1. `cargo build` / tests as needed  
2. **`scripts/rebuild-install-relaunch.sh`** (or `.ps1`)  
3. Confirm process is the install path (`Get-Process optimus-desktop`)  
4. Only then claim “ready to use”

## Uninstall

```powershell
powershell -ExecutionPolicy Bypass -File "$env:LOCALAPPDATA\Programs\OptimusAgent\uninstall.ps1"
```
