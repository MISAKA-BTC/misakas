<#
  MISAKA local node launcher for Windows.

  This script runs the shared Linux/macOS helper inside WSL2 Ubuntu, so Windows
  users can start/check/stop the local MISAKA node from PowerShell or .cmd files.
#>
[CmdletBinding()]
param(
  [ValidateSet(
    "auto-node",
    "status",
    "auto-validator",
    "stop-all",
    "logs",
    "prepare",
    "start-node",
    "stop-node",
    "restart-node",
    "doctor",
    "collect-support-log",
    "wait-sync",
    "keygen",
    "miner-start",
    "miner-stop",
    "balance",
    "bond",
    "validator-start",
    "validator-stop",
    "clean",
    "web",
    "help"
  )]
  [string]$Command = "auto-node",

  [string]$Distro = "",

  [string]$Amount = "10MSK",

  [switch]$InstallUbuntu,

  [switch]$ListDistros,

  [switch]$Pause
)

$ErrorActionPreference = "Stop"

function Write-Section {
  param([string]$Message)
  Write-Host ""
  Write-Host "== $Message ==" -ForegroundColor Cyan
}

function Write-Note {
  param([string]$Message)
  Write-Host $Message -ForegroundColor DarkGray
}

function Stop-WithMessage {
  param([string]$Message)
  Write-Host ""
  Write-Host "ERROR: $Message" -ForegroundColor Red
  exit 1
}

function ConvertTo-CleanText {
  param($Value)

  if ($null -eq $Value) {
    return ""
  }

  return ([string]$Value -replace "`0", "").Trim()
}

function Join-CleanOutput {
  param($Value)

  if ($null -eq $Value) {
    return ""
  }

  $lines = @($Value | ForEach-Object { ConvertTo-CleanText $_ })
  return ($lines -join "`n").Trim()
}

function ConvertTo-WslMountPath {
  param([string]$WindowsPath)

  $fullPath = (Resolve-Path -LiteralPath $WindowsPath).Path
  if ($fullPath -notmatch "^([A-Za-z]):\\(.*)$") {
    return ""
  }

  $drive = $Matches[1].ToLowerInvariant()
  $rest = $Matches[2] -replace "\\", "/"
  return "/mnt/$drive/$rest"
}

function ConvertTo-BashSingleQuoted {
  param([string]$Value)

  $singleQuote = [string][char]39
  $backslash = [string][char]92
  $replacement = $singleQuote + $backslash + $singleQuote + $singleQuote
  return $singleQuote + $Value.Replace($singleQuote, $replacement) + $singleQuote
}

function New-BashScriptWithEnv {
  param(
    [hashtable]$Vars,
    [string]$Body
  )

  $assignments = @()
  $names = @()
  foreach ($key in $Vars.Keys) {
    $names += $key
    $assignments += "$key=$(ConvertTo-BashSingleQuoted ([string]$Vars[$key]))"
  }

  return (($assignments + @("export $($names -join ' ')")) -join "; ") + "`n" + $Body
}

function ConvertTo-BashScriptBase64 {
  param([string]$ScriptText)

  if ($null -eq $ScriptText) {
    $ScriptText = ""
  }

  $normalized = ([string]$ScriptText) -replace "`r`n", "`n"
  $normalized = $normalized -replace "`r", "`n"
  $utf8NoBom = New-Object System.Text.UTF8Encoding -ArgumentList $false
  return [Convert]::ToBase64String($utf8NoBom.GetBytes($normalized))
}

function New-WslBashRunnerCommand {
  param([string]$ScriptText)

  $encodedScript = ConvertTo-BashScriptBase64 -ScriptText $ScriptText
  return "set -o pipefail; printf '%s' '$encodedScript' | base64 -d | bash"
}

function Get-WslDistros {
  $raw = @(& wsl.exe -l -q 2>$null)
  if ($LASTEXITCODE -ne 0) {
    return @()
  }
  return @(
    $raw |
      ForEach-Object { ConvertTo-CleanText $_ } |
      Where-Object { $_ -ne "" -and $_ -notmatch "^docker-desktop" }
  )
}

function Select-WslDistro {
  param(
    [string]$Requested,
    [string[]]$Distros
  )

  if ($Requested -ne "") {
    if ($Distros -contains $Requested) {
      return $Requested
    }
    Stop-WithMessage "WSL distro '$Requested' was not found. Use -ListDistros to see installed distros."
  }

  $ubuntu = @($Distros | Where-Object { $_ -match "^Ubuntu" })
  if ($ubuntu.Count -gt 0) {
    return $ubuntu[0]
  }

  if ($Distros.Count -gt 0) {
    return $Distros[0]
  }

  return ""
}

function Test-MisakaShareRoot {
  param([string]$Path)

  return (
    (Test-Path -LiteralPath (Join-Path $Path "windows\start-misaka-local-node-wsl.ps1")) -and
    (Test-Path -LiteralPath (Join-Path $Path "scripts\misaka-desktop-node.sh")) -and
    (Test-Path -LiteralPath (Join-Path $Path "scripts\misaka-desktop-web.sh")) -and
    (Test-Path -LiteralPath (Join-Path $Path "ui\setup.html"))
  )
}

function Resolve-MisakaShareRoot {
  param([string]$ScriptDir)

  $parent = (Resolve-Path -LiteralPath (Join-Path $ScriptDir "..")).Path
  if (Test-MisakaShareRoot -Path $parent) {
    return $parent
  }

  $nested = @(
    Get-ChildItem -LiteralPath $parent -Directory -ErrorAction SilentlyContinue |
      Where-Object { Test-MisakaShareRoot -Path $_.FullName }
  )

  if ($nested.Count -eq 1) {
    Write-Note "Detected nested share folder: $($nested[0].FullName)"
    return $nested[0].FullName
  }

  if ($nested.Count -gt 1) {
    Write-Host ""
    Write-Host "Candidate folders:"
    $nested | ForEach-Object { Write-Host "  $($_.FullName)" }
    Stop-WithMessage "Multiple MISAKA share folders were found. Run the .cmd file from the windows folder inside the share folder you want to use."
  }

  Write-Host ""
  Write-Host "Expected files were not found under:"
  Write-Host "  $parent"
  Write-Host ""
  Write-Host "Expected folder layout:"
  Write-Host "  <misaka folder>\windows\start-node-wsl.cmd"
  Write-Host "  <misaka folder>\scripts\misaka-desktop-node.sh"
  Write-Host "  <misaka folder>\ui\setup.html"
  Write-Host ""
  Stop-WithMessage "The zip was not fully extracted, or only the windows folder was copied. Extract the latest zip and keep windows, scripts, ui, mac, and docs together."
}

function Invoke-Wsl {
  param(
    [string]$DistroName,
    [string[]]$Arguments
  )
  $oldErrorActionPreference = $ErrorActionPreference
  $ErrorActionPreference = "Continue"
  try {
    & wsl.exe -d $DistroName -- @Arguments
    $exitCode = $LASTEXITCODE
  } finally {
    $ErrorActionPreference = $oldErrorActionPreference
  }
  if ($exitCode -ne 0) {
    exit $exitCode
  }
}

function Invoke-WslBashScript {
  param(
    [string]$DistroName,
    [string]$ScriptText
  )

  $oldErrorActionPreference = $ErrorActionPreference
  $ErrorActionPreference = "Continue"
  try {
    $runner = New-WslBashRunnerCommand -ScriptText $ScriptText
    & wsl.exe -d $DistroName -- bash -lc $runner
    $exitCode = $LASTEXITCODE
  } finally {
    $ErrorActionPreference = $oldErrorActionPreference
  }
  if ($exitCode -ne 0) {
    exit $exitCode
  }
}

function Test-WslShareRoot {
  param(
    [string]$DistroName,
    [string]$WslPath
  )

  $oldErrorActionPreference = $ErrorActionPreference
  $ErrorActionPreference = "Continue"
  try {
    $checkScript = New-BashScriptWithEnv -Vars @{ MISAKA_SHARE_DIR = $WslPath } -Body 'test -f "$MISAKA_SHARE_DIR/scripts/misaka-desktop-node.sh" && test -f "$MISAKA_SHARE_DIR/scripts/misaka-desktop-web.sh" && test -f "$MISAKA_SHARE_DIR/ui/setup.html"'
    $runner = New-WslBashRunnerCommand -ScriptText $checkScript
    & wsl.exe -d $DistroName -- bash -lc $runner | Out-Null
    $exitCode = $LASTEXITCODE
  } finally {
    $ErrorActionPreference = $oldErrorActionPreference
  }

  return $exitCode -eq 0
}

function Show-WslShareDiagnostics {
  param(
    [string]$DistroName,
    [string]$WslPath
  )

  Write-Host ""
  Write-Host "WSL path checked:"
  Write-Host "  $WslPath"
  Write-Host ""
  Write-Host "WSL folder listing:"

  $oldErrorActionPreference = $ErrorActionPreference
  $ErrorActionPreference = "Continue"
  try {
    $diagScript = New-BashScriptWithEnv -Vars @{ MISAKA_SHARE_DIR = $WslPath } -Body 'pwd; echo "MISAKA_SHARE_DIR=$MISAKA_SHARE_DIR"; ls -la "$MISAKA_SHARE_DIR" 2>&1; echo; ls -la "$MISAKA_SHARE_DIR/scripts" "$MISAKA_SHARE_DIR/ui" 2>&1'
    $runner = New-WslBashRunnerCommand -ScriptText $diagScript
    & wsl.exe -d $DistroName -- bash -lc $runner
  } finally {
    $ErrorActionPreference = $oldErrorActionPreference
  }
}

try {
  Write-Section "MISAKA local node for Windows WSL2"

  if (-not (Get-Command wsl.exe -ErrorAction SilentlyContinue)) {
    Stop-WithMessage "WSL is not installed. Open PowerShell as Administrator and run: wsl --install -d Ubuntu"
  }

  if ($InstallUbuntu) {
    Write-Section "Install Ubuntu on WSL2"
    Write-Host "Running: wsl --install -d Ubuntu"
    $oldErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
      & wsl.exe --install -d Ubuntu
      $installExitCode = $LASTEXITCODE
    } finally {
      $ErrorActionPreference = $oldErrorActionPreference
    }
    if ($installExitCode -ne 0) {
      Stop-WithMessage "Ubuntu installation failed. Open PowerShell as Administrator and run: wsl --install -d Ubuntu"
    }
    Write-Host ""
    Write-Host "After installation finishes, restart Windows if requested, open Ubuntu once, then run this script again."
    return
  }

  $distros = Get-WslDistros

  if ($ListDistros) {
    Write-Section "Installed WSL distros"
    if ($distros.Count -eq 0) {
      Write-Host "No WSL distro found."
      Write-Host "Install Ubuntu:"
      Write-Host "  wsl --install -d Ubuntu"
    } else {
      $distros | ForEach-Object { Write-Host "  $_" }
    }
    return
  }

  if ($distros.Count -eq 0) {
    Stop-WithMessage "No WSL distro found. Install Ubuntu first: powershell -ExecutionPolicy Bypass -File .\windows\start-misaka-local-node-wsl.ps1 -InstallUbuntu"
  }

  $selectedDistro = Select-WslDistro -Requested $Distro -Distros $distros
  if ($selectedDistro -eq "") {
    Stop-WithMessage "No usable WSL distro found. Install Ubuntu first: wsl --install -d Ubuntu"
  }

  Write-Host "WSL distro: $selectedDistro"

  $scriptDir = if ($PSScriptRoot) {
    $PSScriptRoot
  } else {
    Split-Path -Parent $MyInvocation.MyCommand.Path
  }
  $shareDir = Resolve-MisakaShareRoot -ScriptDir $scriptDir
  Write-Host "Share dir:  $shareDir"
  Write-Note "WSL bridge: base64-v3"

  # wslpath accepts forward-slash Windows paths more reliably. Without this,
  # paths like C:\misakatest can arrive in WSL as C:misakatest.
  $shareDirForWslpath = $shareDir -replace "\\", "/"
  $oldErrorActionPreference = $ErrorActionPreference
  $ErrorActionPreference = "Continue"
  try {
    $wslPathOutput = @(& wsl.exe -d $selectedDistro -- wslpath -u -a "$shareDirForWslpath" 2>&1)
    $wslPathExit = $LASTEXITCODE
  } finally {
    $ErrorActionPreference = $oldErrorActionPreference
  }
  $wslShareDir = Join-CleanOutput $wslPathOutput
  if ($wslPathExit -ne 0 -or $wslShareDir -eq "") {
    if ($wslShareDir -ne "") {
      Write-Host "wslpath output:"
      Write-Host $wslShareDir
    }
    Stop-WithMessage "Could not convert Windows path to a WSL path. Move this folder under a normal local folder such as C:\Users\<you>\Downloads, open Ubuntu once, then try again."
  }
  if (-not (Test-WslShareRoot -DistroName $selectedDistro -WslPath $wslShareDir)) {
    $fallbackWslShareDir = ConvertTo-WslMountPath -WindowsPath $shareDir
    if ($fallbackWslShareDir -ne "" -and $fallbackWslShareDir -ne $wslShareDir) {
      Write-Note "wslpath result did not expose required files; trying fallback path: $fallbackWslShareDir"
      if (Test-WslShareRoot -DistroName $selectedDistro -WslPath $fallbackWslShareDir) {
        $wslShareDir = $fallbackWslShareDir
      } else {
        Show-WslShareDiagnostics -DistroName $selectedDistro -WslPath $fallbackWslShareDir
        Stop-WithMessage "WSL can open the folder, but required files are missing there. Extract the full zip and keep windows, scripts, ui, mac, and docs together. Do not copy only the windows folder."
      }
    } else {
      Show-WslShareDiagnostics -DistroName $selectedDistro -WslPath $wslShareDir
      Stop-WithMessage "WSL can open the folder, but required files are missing there. Extract the full zip and keep windows, scripts, ui, mac, and docs together. Do not copy only the windows folder."
    }
  }
  if (-not (Test-WslShareRoot -DistroName $selectedDistro -WslPath $wslShareDir)) {
    Show-WslShareDiagnostics -DistroName $selectedDistro -WslPath $wslShareDir
    Stop-WithMessage "WSL can open the folder, but required files are missing there. Extract the full zip and keep windows, scripts, ui, mac, and docs together. Do not copy only the windows folder."
  }

  $scriptArgs = @($Command)
  if ($Command -eq "bond") {
    $scriptArgs += $Amount
  }

  if ($Command -eq "web") {
    $bashCommand = @'
set -e
cd "$MISAKA_SHARE_DIR"
chmod +x scripts/misaka-desktop-node.sh scripts/misaka-desktop-web.sh
scripts/misaka-desktop-web.sh
'@
    Write-Section "Run"
    Write-Host "Command: scripts/misaka-desktop-web.sh"
    Write-Note "Keep this PowerShell window open while using the Web UI."
    $bashCommand = New-BashScriptWithEnv -Vars @{ MISAKA_SHARE_DIR = $wslShareDir } -Body $bashCommand
    Invoke-WslBashScript -DistroName $selectedDistro -ScriptText $bashCommand
    return
  }

  $bashCommand = @'
set -e
cd "$MISAKA_SHARE_DIR"
chmod +x scripts/misaka-desktop-node.sh
if [ "$MISAKA_DESKTOP_COMMAND" = "bond" ]; then
  scripts/misaka-desktop-node.sh bond "$MISAKA_DESKTOP_AMOUNT"
else
  scripts/misaka-desktop-node.sh "$MISAKA_DESKTOP_COMMAND"
fi
'@

  Write-Section "Run"
  Write-Host "Command: scripts/misaka-desktop-node.sh $($scriptArgs -join ' ')"
  Write-Note "First build can take a long time."

  $bashCommand = New-BashScriptWithEnv -Vars @{
    MISAKA_SHARE_DIR = $wslShareDir
    MISAKA_DESKTOP_COMMAND = $Command
    MISAKA_DESKTOP_AMOUNT = $Amount
  } -Body $bashCommand
  Invoke-WslBashScript -DistroName $selectedDistro -ScriptText $bashCommand
} finally {
  if ($Pause) {
    Write-Host ""
    Read-Host "Press Enter to close"
  }
}
