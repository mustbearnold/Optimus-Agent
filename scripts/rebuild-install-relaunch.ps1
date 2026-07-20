#Requires -Version 5.1
<#
.SYNOPSIS
  Rebuild Optimus desktop, install unsigned to a stable local path, relaunch.

.PARAMETER Configuration
  cargo profile: release (default) or dev

.PARAMETER NoBuild
  Skip cargo build; reinstall whatever is already in CARGO_TARGET_DIR

.PARAMETER NoRelaunch
  Install only; do not start the app

.PARAMETER RepoRoot
  Optimus Agent repo root (auto-detected if omitted)
#>
[CmdletBinding()]
param(
  [ValidateSet('release', 'dev')]
  [string]$Configuration = 'release',
  [switch]$NoBuild,
  [switch]$NoRelaunch,
  [string]$RepoRoot = ''
)

$ErrorActionPreference = 'Stop'

function Write-Step([string]$msg) {
  Write-Host ""
  Write-Host "==> $msg" -ForegroundColor Cyan
}

function Get-RepoRoot {
  if ($RepoRoot -and (Test-Path -LiteralPath $RepoRoot)) {
    return (Resolve-Path -LiteralPath $RepoRoot).Path
  }
  if ($PSScriptRoot) {
    $candidate = Resolve-Path (Join-Path $PSScriptRoot '..') -ErrorAction SilentlyContinue
    if ($candidate -and (Test-Path (Join-Path $candidate.Path 'Cargo.toml'))) {
      return $candidate.Path
    }
  }
  $cwd = (Get-Location).Path
  if (Test-Path (Join-Path $cwd 'Cargo.toml')) { return $cwd }
  throw "Could not locate Optimus Agent repo root (Cargo.toml). Pass -RepoRoot."
}

function Stop-OptimusProcesses {
  Write-Step "Stopping running Optimus processes"
  foreach ($name in @('optimus-desktop', 'optimus-cli')) {
    Get-Process -Name $name -ErrorAction SilentlyContinue | ForEach-Object {
      Write-Host ("  kill pid={0} {1}" -f $_.Id, $_.ProcessName)
      Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue
    }
  }
  Start-Sleep -Milliseconds 400
}

function Ensure-Dir([string]$path) {
  if (-not (Test-Path -LiteralPath $path)) {
    New-Item -ItemType Directory -Path $path -Force | Out-Null
  }
}

function New-Shortcut {
  param(
    [Parameter(Mandatory = $true)][string]$LinkPath,
    [Parameter(Mandatory = $true)][string]$TargetPath,
    [string]$Arguments = '',
    [string]$WorkingDirectory = '',
    [string]$Description = 'Optimus Agent'
  )
  $dir = Split-Path -Parent $LinkPath
  Ensure-Dir $dir
  $w = New-Object -ComObject WScript.Shell
  $sc = $w.CreateShortcut($LinkPath)
  $sc.TargetPath = $TargetPath
  if ($Arguments) { $sc.Arguments = $Arguments }
  if ($WorkingDirectory) { $sc.WorkingDirectory = $WorkingDirectory }
  $sc.Description = $Description
  if (Test-Path -LiteralPath $TargetPath) { $sc.IconLocation = ($TargetPath + ',0') }
  $sc.Save()
  Write-Host ("  shortcut: {0}" -f $LinkPath)
}

function Install-Binary {
  param(
    [string]$Src,
    [string]$InstallRoot,
    [string]$Name
  )
  $dest = Join-Path $InstallRoot $Name
  $tmp = $dest + '.new'
  Copy-Item -LiteralPath $Src -Destination $tmp -Force
  if (Test-Path -LiteralPath $dest) {
    Remove-Item -LiteralPath $dest -Force -ErrorAction SilentlyContinue
  }
  Move-Item -LiteralPath $tmp -Destination $dest -Force
  $item = Get-Item -LiteralPath $dest
  Write-Host ("  {0}  {1:N0} bytes  {2}" -f $Name, $item.Length, $item.LastWriteTime.ToString('s'))
}

# --- paths ---
$root = Get-RepoRoot
$installRoot = Join-Path $env:LOCALAPPDATA 'Programs\OptimusAgent'
$startMenu = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs'
$desktop = [Environment]::GetFolderPath('Desktop')

if (-not $env:CARGO_TARGET_DIR) {
  $env:CARGO_TARGET_DIR = (Join-Path $root 'local\tmp\cargo-target')
}
$env:TEMP = 'C:\Users\mustb\AppData\Local\Temp'
$env:TMP = $env:TEMP

$profileDir = if ($Configuration -eq 'release') { 'release' } else { 'debug' }
$targetDir = Join-Path $env:CARGO_TARGET_DIR $profileDir
$builtDesktop = Join-Path $targetDir 'optimus-desktop.exe'
# CLI package binary name is `optimus` (see apps/optimus-cli Cargo.toml [[bin]])
$builtCli = Join-Path $targetDir 'optimus.exe'
if (-not (Test-Path -LiteralPath $builtCli)) {
  $alt = Join-Path $targetDir 'optimus-cli.exe'
  if (Test-Path -LiteralPath $alt) { $builtCli = $alt }
}

Write-Host "Optimus Agent - rebuild + local install (unsigned)"
Write-Host ("  repo:     {0}" -f $root)
Write-Host ("  profile:  {0}" -f $Configuration)
Write-Host ("  target:   {0}" -f $env:CARGO_TARGET_DIR)
Write-Host ("  install:  {0}" -f $installRoot)

Stop-OptimusProcesses

if (-not $NoBuild) {
  Write-Step ("cargo build -p optimus-desktop -p optimus-cli ({0})" -f $Configuration)
  Push-Location $root
  try {
    if ($Configuration -eq 'release') {
      & cargo build -p optimus-desktop -p optimus-cli --release
    } else {
      & cargo build -p optimus-desktop -p optimus-cli
    }
    if (-not $?) { throw "cargo build failed" }
  } finally {
    Pop-Location
  }
}

if (-not (Test-Path -LiteralPath $builtDesktop)) {
  throw ("Missing built binary: {0}" -f $builtDesktop)
}
if (-not (Test-Path -LiteralPath $builtCli)) {
  throw ("Missing built binary: {0}" -f $builtCli)
}

Write-Step ("Installing binaries to {0}" -f $installRoot)
Ensure-Dir $installRoot
Install-Binary -Src $builtDesktop -InstallRoot $installRoot -Name 'optimus-desktop.exe'
Install-Binary -Src $builtCli -InstallRoot $installRoot -Name 'optimus.exe'
# Convenience alias name used in docs
Copy-Item -LiteralPath (Join-Path $installRoot 'optimus.exe') -Destination (Join-Path $installRoot 'optimus-cli.exe') -Force
Write-Host "  optimus-cli.exe  (copy of optimus.exe)"

$version = '0.1.0'
try {
  Push-Location $root
  $metaJson = & cargo metadata --no-deps --format-version 1 | Out-String
  $meta = $metaJson | ConvertFrom-Json
  foreach ($pkg in $meta.packages) {
    if ($pkg.name -eq 'optimus-desktop') {
      $version = $pkg.version
      break
    }
  }
} catch {
  # keep default
} finally {
  Pop-Location -ErrorAction SilentlyContinue
}

$stamp = Get-Date -Format o
$versionLines = @(
  ("Optimus Agent {0}" -f $version),
  ("profile={0}" -f $Configuration),
  ("installed={0}" -f $stamp),
  ("source={0}" -f $root),
  'signed=false'
)
$versionPath = Join-Path $installRoot 'VERSION.txt'
$versionLines | Set-Content -LiteralPath $versionPath -Encoding UTF8

$desktopExe = Join-Path $installRoot 'optimus-desktop.exe'
$cliExe = Join-Path $installRoot 'optimus-cli.exe'
$metaPath = Join-Path $installRoot 'install-meta.json'
$metaMap = @{
  name          = 'Optimus Agent'
  version       = $version
  configuration = $Configuration
  installed_at  = $stamp
  install_root  = $installRoot
  source_repo   = $root
  cargo_target  = $env:CARGO_TARGET_DIR
  signed        = $false
  desktop_exe   = $desktopExe
  cli_exe       = $cliExe
}
($metaMap | ConvertTo-Json) | Set-Content -LiteralPath $metaPath -Encoding UTF8

$uninstallPath = Join-Path $installRoot 'uninstall.ps1'
$uninstallBody = @(
  '#Requires -Version 5.1'
  '$ErrorActionPreference = "Stop"'
  '$root = Split-Path -Parent $MyInvocation.MyCommand.Path'
  'Get-Process optimus-desktop,optimus-cli -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue'
  'Start-Sleep -Milliseconds 300'
  '$lnk1 = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\Optimus Agent.lnk"'
  '$lnk2 = Join-Path ([Environment]::GetFolderPath("Desktop")) "Optimus Agent.lnk"'
  'foreach ($l in @($lnk1, $lnk2)) { if (Test-Path -LiteralPath $l) { Remove-Item -LiteralPath $l -Force } }'
  'Remove-Item -LiteralPath $root -Recurse -Force'
  'Write-Host "Uninstalled Optimus Agent from $root"'
) -join "`r`n"
Set-Content -LiteralPath $uninstallPath -Value $uninstallBody -Encoding UTF8

Write-Step "Creating Start Menu + Desktop shortcuts"
New-Shortcut -LinkPath (Join-Path $startMenu 'Optimus Agent.lnk') `
  -TargetPath $desktopExe `
  -WorkingDirectory $installRoot `
  -Description 'Optimus Agent local install'
New-Shortcut -LinkPath (Join-Path $desktop 'Optimus Agent.lnk') `
  -TargetPath $desktopExe `
  -WorkingDirectory $installRoot `
  -Description 'Optimus Agent local install'

$readmePath = Join-Path $installRoot 'README-INSTALL.txt'
$readme = @(
  'Optimus Agent - local install (not code-signed)'
  '================================================'
  ("Install root: {0}" -f $installRoot)
  ''
  'Launch:'
  '  - Start Menu > Optimus Agent'
  '  - Desktop shortcut'
  ("  - {0}" -f $desktopExe)
  ''
  'CLI:'
  ("  {0} --help" -f $cliExe)
  ''
  'Uninstall:'
  ("  powershell -ExecutionPolicy Bypass -File `"{0}`"" -f $uninstallPath)
  ''
  'Rebuild + reinstall + relaunch (from repo):'
  '  bash scripts/rebuild-install-relaunch.sh'
  '  powershell -ExecutionPolicy Bypass -File scripts\rebuild-install-relaunch.ps1'
) -join "`r`n"
Set-Content -LiteralPath $readmePath -Value $readme -Encoding UTF8

if (-not $NoRelaunch) {
  Write-Step "Relaunching Optimus desktop"
  Start-Process -FilePath $desktopExe -WorkingDirectory $installRoot
  Start-Sleep -Milliseconds 800
  $p = Get-Process -Name 'optimus-desktop' -ErrorAction SilentlyContinue | Select-Object -First 1
  if ($p) {
    Write-Host ("  running pid={0} path={1}" -f $p.Id, $p.Path) -ForegroundColor Green
  } else {
    Write-Host "  warning: process not observed yet (may still be starting)" -ForegroundColor Yellow
  }
}

Write-Host ""
Write-Host "Done. Local install at:" -ForegroundColor Green
Write-Host ("  {0}" -f $installRoot)
Write-Host ("  version {0} ({1})" -f $version, $Configuration)
