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

function Stop-OptimusProcesses([string]$InstallRoot) {
  Write-Step "Stopping running Optimus processes"
  $owned = @(
    [IO.Path]::GetFullPath((Join-Path $InstallRoot 'optimus-desktop.exe')),
    [IO.Path]::GetFullPath((Join-Path $InstallRoot 'optimus.exe')),
    [IO.Path]::GetFullPath((Join-Path $InstallRoot 'optimus-cli.exe'))
  )
  foreach ($name in @('optimus-desktop', 'optimus-cli')) {
    Get-Process -Name $name -ErrorAction SilentlyContinue | ForEach-Object {
      $path = $_.Path
      if ($path -and ($owned -contains [IO.Path]::GetFullPath($path))) {
        Write-Host ("  kill pid={0} {1}" -f $_.Id, $_.ProcessName)
        Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue
      }
    }
  }
  Start-Sleep -Milliseconds 400
}

$script:InstallMarkerName = '.optimus-agent-install'
$script:InstallMarkerPrefix = 'optimus-agent-user-install-v1:'

function Assert-NoReparsePoint([string]$Path, [string]$Label) {
  if (Test-Path -LiteralPath $Path) {
    $item = Get-Item -LiteralPath $Path -Force
    if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
      throw "Refusing reparse-point $Label`: $Path"
    }
  }
}

function Assert-NoReparseComponents([string]$Path, [string]$Label) {
  $full = [IO.Path]::GetFullPath($Path)
  $root = [IO.Path]::GetPathRoot($full)
  $current = $root
  Assert-NoReparsePoint -Path $current -Label $Label
  $relative = $full.Substring($root.Length)
  foreach ($component in ($relative -split '[\\/]')) {
    if (-not $component) { continue }
    $current = Join-Path $current $component
    Assert-NoReparsePoint -Path $current -Label $Label
  }
}

function Ensure-Dir([string]$Path) {
  Assert-NoReparseComponents -Path $Path -Label 'directory path'
  if (-not (Test-Path -LiteralPath $Path)) {
    New-Item -ItemType Directory -Path $Path -Force | Out-Null
  }
  Assert-NoReparseComponents -Path $Path -Label 'directory path'
}

if (-not ('OptimusNativeFile' -as [type])) {
  Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;

public static class OptimusNativeFile {
    [StructLayout(LayoutKind.Sequential)]
    private struct FileTime {
        public uint Low;
        public uint High;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct ByHandleFileInformation {
        public uint FileAttributes;
        public FileTime CreationTime;
        public FileTime LastAccessTime;
        public FileTime LastWriteTime;
        public uint VolumeSerialNumber;
        public uint FileSizeHigh;
        public uint FileSizeLow;
        public uint NumberOfLinks;
        public uint FileIndexHigh;
        public uint FileIndexLow;
    }

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool GetFileInformationByHandle(
        SafeFileHandle handle,
        out ByHandleFileInformation information);

    public static uint GetLinkCount(SafeFileHandle handle) {
        ByHandleFileInformation information;
        if (!GetFileInformationByHandle(handle, out information)) {
            throw new Win32Exception(Marshal.GetLastWin32Error());
        }
        return information.NumberOfLinks;
    }
}
'@
}

function Assert-SingleLink([string]$Path, [string]$Label) {
  Assert-NoReparseComponents -Path $Path -Label $Label
  $stream = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
  try {
    $links = [OptimusNativeFile]::GetLinkCount($stream.SafeFileHandle)
    if ($links -ne 1) {
      throw "Refusing hard-linked $Label ($links links): $Path"
    }
  } finally {
    $stream.Dispose()
  }
}

function New-RandomTempPath([string]$Destination, [string]$Suffix = '.tmp') {
  $dir = Split-Path -Parent $Destination
  $name = Split-Path -Leaf $Destination
  return (Join-Path $dir ('.{0}.{1}{2}' -f $name, [Guid]::NewGuid().ToString('N'), $Suffix))
}

function Publish-OwnedFile([string]$Temporary, [string]$Destination, [string]$Label) {
  Assert-SingleLink -Path $Temporary -Label "temporary $Label"
  Assert-NoReparseComponents -Path $Destination -Label $Label
  if (Test-Path -LiteralPath $Destination) {
    Remove-Item -LiteralPath $Destination -Force
  }
  [IO.File]::Move($Temporary, $Destination)
}

function Assert-InstallRootOwnership([string]$InstallRoot) {
  Assert-NoReparseComponents -Path $InstallRoot -Label 'install root'
  $marker = Join-Path $InstallRoot $script:InstallMarkerName
  Assert-NoReparsePoint -Path $marker -Label 'ownership marker'
  if (-not (Test-Path -LiteralPath $InstallRoot)) { return }
  if (-not (Test-Path -LiteralPath $marker -PathType Leaf)) {
    $entries = @(Get-ChildItem -LiteralPath $InstallRoot -Force)
    if ($entries.Count -gt 0) {
      throw "Refusing non-empty install root without Optimus ownership marker: $InstallRoot"
    }
    return
  }
  $value = (Get-Content -LiteralPath $marker -Raw).Trim()
  if (-not $value.StartsWith($script:InstallMarkerPrefix, [StringComparison]::Ordinal)) {
    throw "Refusing install root with invalid Optimus ownership marker: $InstallRoot"
  }
}

function Assert-ShortcutOwned([string]$LinkPath, [string]$ExpectedTarget) {
  Assert-NoReparseComponents -Path $LinkPath -Label 'shortcut'
  if (-not (Test-Path -LiteralPath $LinkPath)) { return }
  $shell = New-Object -ComObject WScript.Shell
  $shortcut = $shell.CreateShortcut($LinkPath)
  $actual = [IO.Path]::GetFullPath([Environment]::ExpandEnvironmentVariables($shortcut.TargetPath))
  $expected = [IO.Path]::GetFullPath($ExpectedTarget)
  if (-not [String]::Equals($actual, $expected, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to replace foreign shortcut: $LinkPath targets $actual"
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
  Assert-ShortcutOwned -LinkPath $LinkPath -ExpectedTarget $TargetPath
  $tmp = New-RandomTempPath -Destination $LinkPath -Suffix '.lnk'
  $shell = New-Object -ComObject WScript.Shell
  try {
    $shortcut = $shell.CreateShortcut($tmp)
    $shortcut.TargetPath = $TargetPath
    if ($Arguments) { $shortcut.Arguments = $Arguments }
    if ($WorkingDirectory) { $shortcut.WorkingDirectory = $WorkingDirectory }
    $shortcut.Description = $Description
    if (Test-Path -LiteralPath $TargetPath) { $shortcut.IconLocation = ($TargetPath + ',0') }
    $shortcut.Save()
    Assert-ShortcutOwned -LinkPath $tmp -ExpectedTarget $TargetPath
    Assert-ShortcutOwned -LinkPath $LinkPath -ExpectedTarget $TargetPath
    Publish-OwnedFile -Temporary $tmp -Destination $LinkPath -Label 'shortcut'
  } finally {
    if (Test-Path -LiteralPath $tmp) { Remove-Item -LiteralPath $tmp -Force }
  }
  Write-Host ("  shortcut: {0}" -f $LinkPath)
}

function Install-Binary {
  param(
    [string]$Src,
    [string]$InstallRoot,
    [string]$Name,
    [Parameter(Mandatory = $true)][string]$ExpectedSha256
  )
  $dest = Join-Path $InstallRoot $Name
  $tmp = New-RandomTempPath -Destination $dest
  Assert-NoReparseComponents -Path $dest -Label 'installed binary'
  $sourceStream = $null
  $targetStream = $null
  try {
    $sourceStream = [IO.File]::Open($Src, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    $targetStream = [IO.File]::Open($tmp, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    $sourceStream.CopyTo($targetStream)
    $targetStream.Flush($true)
    $sourceStream.Dispose()
    $sourceStream = $null
    $targetStream.Dispose()
    $targetStream = $null
    $stagedSha256 = (Get-FileHash -LiteralPath $tmp -Algorithm SHA256).Hash.ToLowerInvariant()
    if (-not [String]::Equals($stagedSha256, $ExpectedSha256, [StringComparison]::Ordinal)) {
      throw "Staged bytes for $Name do not match the validated artifact"
    }
    Publish-OwnedFile -Temporary $tmp -Destination $dest -Label 'installed binary'
  } finally {
    if ($sourceStream) { $sourceStream.Dispose() }
    if ($targetStream) { $targetStream.Dispose() }
    if (Test-Path -LiteralPath $tmp) { Remove-Item -LiteralPath $tmp -Force }
  }
  $item = Get-Item -LiteralPath $dest
  Write-Host ("  {0}  {1:N0} bytes  {2}" -f $Name, $item.Length, $item.LastWriteTime.ToString('s'))
}

function Set-OwnedContent {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)]$Value
  )
  $tmp = New-RandomTempPath -Destination $Path
  Assert-NoReparseComponents -Path $Path -Label 'installed metadata'
  $stream = $null
  try {
    $text = (@($Value) -join [Environment]::NewLine) + [Environment]::NewLine
    $bytes = [Text.Encoding]::UTF8.GetBytes($text)
    $stream = [IO.File]::Open($tmp, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    $stream.Write($bytes, 0, $bytes.Length)
    $stream.Flush($true)
    $stream.Dispose()
    $stream = $null
    Publish-OwnedFile -Temporary $tmp -Destination $Path -Label 'installed metadata'
  } finally {
    if ($stream) { $stream.Dispose() }
    if (Test-Path -LiteralPath $tmp) { Remove-Item -LiteralPath $tmp -Force }
  }
}

# --- paths ---
$root = Get-RepoRoot
$installRoot = Join-Path $env:LOCALAPPDATA 'Programs\OptimusAgent'
$startMenu = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs'
$desktop = [Environment]::GetFolderPath('Desktop')

if (-not $env:CARGO_TARGET_DIR) {
  $env:CARGO_TARGET_DIR = (Join-Path $env:LOCALAPPDATA 'OptimusAgent\cargo-target')
}

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
Assert-NoReparseComponents -Path $installRoot -Label 'install root'
Assert-NoReparseComponents -Path $startMenu -Label 'Start Menu path'
Assert-NoReparseComponents -Path $desktop -Label 'Desktop path'
Assert-NoReparseComponents -Path $env:CARGO_TARGET_DIR -Label 'Cargo target path'
Assert-InstallRootOwnership -InstallRoot $installRoot

$python = Get-Command python -ErrorAction SilentlyContinue
if (-not $python) {
  throw "Python is required for the Optimus/Hermes release-version gate."
}
Write-Step "Checking Optimus/Hermes version policy"
Push-Location $root
try {
  & $python.Source scripts/optimus_version.py release-check
  if ($LASTEXITCODE -ne 0) { throw "Optimus/Hermes release-version check failed" }
  $parityJson = (& $python.Source scripts/optimus_version.py status --json | Out-String)
  if ($LASTEXITCODE -ne 0) { throw "Could not read Optimus/Hermes version metadata" }
  $parityStatus = $parityJson | ConvertFrom-Json
} finally {
  Pop-Location
}

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

$builtVersionLine = (& $builtDesktop --version | Out-String).Trim()
if ($LASTEXITCODE -ne 0 -or -not $builtVersionLine) {
  throw "Could not read built Desktop version"
}
$builtVersion = ($builtVersionLine -split '\s+')[-1]
if ($builtVersion -ne $parityStatus.product_version) {
  throw ("Built Desktop version {0} does not match policy version {1}" -f $builtVersion, $parityStatus.product_version)
}
$builtCliVersionLine = (& $builtCli --version | Out-String).Trim()
if ($LASTEXITCODE -ne 0 -or -not $builtCliVersionLine) {
  throw "Could not read built CLI version"
}
$builtCliVersion = ($builtCliVersionLine -split '\s+')[-1]
if ($builtCliVersion -ne $parityStatus.product_version) {
  throw ("Built CLI version {0} does not match policy version {1}" -f $builtCliVersion, $parityStatus.product_version)
}
$builtDesktopSha256 = (Get-FileHash -LiteralPath $builtDesktop -Algorithm SHA256).Hash.ToLowerInvariant()
$builtCliSha256 = (Get-FileHash -LiteralPath $builtCli -Algorithm SHA256).Hash.ToLowerInvariant()

# The build may take minutes. Re-run every policy, binary, and path gate
# immediately before stopping or replacing the stable application.
Write-Step "Rechecking Optimus/Hermes version policy and selected binaries"
Push-Location $root
try {
  & $python.Source scripts/optimus_version.py release-check
  if ($LASTEXITCODE -ne 0) { throw "Optimus/Hermes release-version recheck failed" }
  $parityJson = (& $python.Source scripts/optimus_version.py status --json | Out-String)
  if ($LASTEXITCODE -ne 0) { throw "Could not reread Optimus/Hermes version metadata" }
  $parityStatus = $parityJson | ConvertFrom-Json
} finally {
  Pop-Location
}
$builtVersionLine = (& $builtDesktop --version | Out-String).Trim()
$builtVersion = ($builtVersionLine -split '\s+')[-1]
if ($LASTEXITCODE -ne 0 -or -not $builtVersionLine -or $builtVersion -ne $parityStatus.product_version) {
  throw "Built Desktop no longer matches the release policy"
}
$builtCliVersionLine = (& $builtCli --version | Out-String).Trim()
$builtCliVersion = ($builtCliVersionLine -split '\s+')[-1]
if ($LASTEXITCODE -ne 0 -or -not $builtCliVersionLine -or $builtCliVersion -ne $parityStatus.product_version) {
  throw "Built CLI no longer matches the release policy"
}
$currentDesktopSha256 = (Get-FileHash -LiteralPath $builtDesktop -Algorithm SHA256).Hash.ToLowerInvariant()
if (-not [String]::Equals($currentDesktopSha256, $builtDesktopSha256, [StringComparison]::Ordinal)) {
  throw "Built Desktop changed after version validation"
}
$currentCliSha256 = (Get-FileHash -LiteralPath $builtCli -Algorithm SHA256).Hash.ToLowerInvariant()
if (-not [String]::Equals($currentCliSha256, $builtCliSha256, [StringComparison]::Ordinal)) {
  throw "Built CLI changed after version validation"
}
Assert-NoReparseComponents -Path $installRoot -Label 'install root'
Assert-NoReparseComponents -Path $startMenu -Label 'Start Menu path'
Assert-NoReparseComponents -Path $desktop -Label 'Desktop path'
Assert-NoReparseComponents -Path $env:CARGO_TARGET_DIR -Label 'Cargo target path'
Assert-InstallRootOwnership -InstallRoot $installRoot
Stop-OptimusProcesses -InstallRoot $installRoot

Write-Step ("Installing binaries to {0}" -f $installRoot)
Ensure-Dir $installRoot
Assert-InstallRootOwnership -InstallRoot $installRoot
$installMarker = Join-Path $installRoot $script:InstallMarkerName
Assert-NoReparsePoint -Path $installMarker -Label 'ownership marker'
$ownerSid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
Set-OwnedContent -Path $installMarker -Value ($script:InstallMarkerPrefix + $ownerSid)
Install-Binary -Src $builtDesktop -InstallRoot $installRoot -Name 'optimus-desktop.exe' -ExpectedSha256 $builtDesktopSha256
Install-Binary -Src $builtCli -InstallRoot $installRoot -Name 'optimus.exe' -ExpectedSha256 $builtCliSha256
# Convenience alias name used in docs
Install-Binary -Src (Join-Path $installRoot 'optimus.exe') -InstallRoot $installRoot -Name 'optimus-cli.exe' -ExpectedSha256 $builtCliSha256

$version = $builtVersion

$stamp = Get-Date -Format o
$versionLines = @(
  ("Optimus Agent {0}" -f $version),
  ("profile={0}" -f $Configuration),
  ("installed={0}" -f $stamp),
  ("source={0}" -f $root),
  ("hermes_target={0}" -f $parityStatus.hermes_target_version),
  ("hermes_parity={0}" -f $(if ($parityStatus.hermes_parity_version) { $parityStatus.hermes_parity_version } else { 'unverified' })),
  ("hermes_parity_status={0}" -f $parityStatus.claim_status),
  ("hermes_feature_contracts={0}" -f $parityStatus.features.total),
  'signed=false'
)
$versionPath = Join-Path $installRoot 'VERSION.txt'
Set-OwnedContent -Path $versionPath -Value $versionLines

$desktopExe = Join-Path $installRoot 'optimus-desktop.exe'
$cliExe = Join-Path $installRoot 'optimus-cli.exe'
$metaPath = Join-Path $installRoot 'install-meta.json'
$metaMap = @{
  name          = 'Optimus Agent'
  version       = $version
  hermes_target_version = $parityStatus.hermes_target_version
  hermes_parity_version = $parityStatus.hermes_parity_version
  hermes_parity_status = $parityStatus.claim_status
  hermes_feature_contracts = $parityStatus.features.total
  configuration = $Configuration
  installed_at  = $stamp
  install_root  = $installRoot
  source_repo   = $root
  cargo_target  = $env:CARGO_TARGET_DIR
  signed        = $false
  desktop_exe   = $desktopExe
  cli_exe       = $cliExe
}
Set-OwnedContent -Path $metaPath -Value ($metaMap | ConvertTo-Json)

$uninstallPath = Join-Path $installRoot 'uninstall.ps1'
$expectedRoot = $installRoot.Replace("'", "''")
$uninstallBody = @(
  '#Requires -Version 5.1'
  '$ErrorActionPreference = "Stop"'
  '$root = Split-Path -Parent $MyInvocation.MyCommand.Path'
  ("`$expectedRoot = '{0}'" -f $expectedRoot)
  'if (-not [String]::Equals([IO.Path]::GetFullPath($root), [IO.Path]::GetFullPath($expectedRoot), [StringComparison]::OrdinalIgnoreCase)) { throw "Refusing uninstaller outside expectedRoot" }'
  '$fullRoot = [IO.Path]::GetFullPath($root)'
  '$pathRoot = [IO.Path]::GetPathRoot($fullRoot)'
  '$current = $pathRoot'
  'foreach ($component in ($fullRoot.Substring($pathRoot.Length) -split ''[\\/]'')) { if (-not $component) { continue }; $current = Join-Path $current $component; if (Test-Path -LiteralPath $current) { $item = Get-Item -LiteralPath $current -Force; if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Refusing reparse-point install path component: $current" } } }'
  '$marker = Join-Path $root ".optimus-agent-install"'
  'if (-not (Test-Path -LiteralPath $marker -PathType Leaf)) { throw "Optimus ownership marker is missing" }'
  '$markerValue = (Get-Content -LiteralPath $marker -Raw).Trim()'
  'if (-not $markerValue.StartsWith("optimus-agent-user-install-v1:", [StringComparison]::Ordinal)) { throw "Invalid Optimus ownership marker" }'
  '$ownedExe = [IO.Path]::GetFullPath((Join-Path $root "optimus-desktop.exe"))'
  'Get-Process optimus-desktop -ErrorAction SilentlyContinue | Where-Object { $_.Path -and ([IO.Path]::GetFullPath($_.Path) -eq $ownedExe) } | Stop-Process -Force -ErrorAction SilentlyContinue'
  'Start-Sleep -Milliseconds 300'
  '$lnk1 = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\Optimus Agent.lnk"'
  '$lnk2 = Join-Path ([Environment]::GetFolderPath("Desktop")) "Optimus Agent.lnk"'
  '$shortcutShell = New-Object -ComObject WScript.Shell'
  'foreach ($l in @($lnk1, $lnk2)) { if (Test-Path -LiteralPath $l) { $target = $shortcutShell.CreateShortcut($l).TargetPath; if ([String]::Equals([IO.Path]::GetFullPath($target), $ownedExe, [StringComparison]::OrdinalIgnoreCase)) { Remove-Item -LiteralPath $l -Force } else { Write-Warning "Leaving foreign shortcut: $l" } } }'
  'Remove-Item -LiteralPath $root -Recurse -Force'
  'Write-Host "Uninstalled Optimus Agent from $root"'
) -join "`r`n"
Set-OwnedContent -Path $uninstallPath -Value $uninstallBody

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
Set-OwnedContent -Path $readmePath -Value $readme

if (-not $NoRelaunch) {
  Write-Step "Relaunching Optimus desktop"
  Start-Process -FilePath $desktopExe -WorkingDirectory $installRoot
  Start-Sleep -Milliseconds 800
  $process = Get-Process -Name 'optimus-desktop' -ErrorAction SilentlyContinue | Select-Object -First 1
  if ($process) {
    Write-Host ("  running pid={0} path={1}" -f $process.Id, $process.Path) -ForegroundColor Green
  } else {
    Write-Host "  warning: process not observed yet (may still be starting)" -ForegroundColor Yellow
  }
}

Write-Host ""
Write-Host "Done. Local install at:" -ForegroundColor Green
Write-Host ("  {0}" -f $installRoot)
Write-Host ("  version {0} ({1})" -f $version, $Configuration)
