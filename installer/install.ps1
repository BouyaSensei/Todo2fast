# Todo2fast Windows Installer
# Usage: .\install.ps1 [-InstallDir "C:\Todo2fast"] [-Port 8080] [-RegisterService]

param(
    [string]$InstallDir = "$env:ProgramFiles\Todo2fast",
    [int]$Port = 8080,
    [switch]$RegisterService,
    [switch]$Uninstall
)

$ErrorActionPreference = "Stop"
$AppName = "Todo2fast"
$ExeName = "todo2fast.exe"

Write-Host "╔══════════════════════════════════════╗" -ForegroundColor Cyan
Write-Host "║       Todo2fast Installer          ║" -ForegroundColor Cyan
Write-Host "╚══════════════════════════════════════╝" -ForegroundColor Cyan
Write-Host ""

# ─── Uninstall ────────────────────────────────────────────────────────────────
if ($Uninstall) {
    Write-Host "[UNINSTALL] Removing $AppName..." -ForegroundColor Yellow

    # Stop service if registered
    $svc = Get-Service -Name "$AppName" -ErrorAction SilentlyContinue
    if ($svc) {
        Write-Host "  Stopping service..."
        Stop-Service -Name "$AppName" -Force
        Write-Host "  Removing service..."
        & sc.exe delete "$AppName" | Out-Null
    }

    # Remove shortcuts
    $startMenu = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\$AppName.lnk"
    if (Test-Path $startMenu) { Remove-Item $startMenu -Force }
    $desktopLnk = Join-Path $env:USERPROFILE "Desktop\$AppName.lnk"
    if (Test-Path $desktopLnk) { Remove-Item $desktopLnk -Force }

    # Remove install dir
    if (Test-Path $InstallDir) {
        Remove-Item $InstallDir -Recurse -Force
        Write-Host "  Removed $InstallDir"
    }

    Write-Host ""
    Write-Host "[DONE] $AppName uninstalled." -ForegroundColor Green
    exit 0
}

# ─── Install ──────────────────────────────────────────────────────────────────

# Find the exe (next to this script, or in backend/target/release/)
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$candidates = @(
    (Join-Path $scriptDir $ExeName),
    (Join-Path $scriptDir "..\backend\target\release\$ExeName"),
    (Join-Path $PSScriptRoot $ExeName)
)
$exePath = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1

if (-not $exePath) {
    Write-Host "[ERROR] Cannot find $ExeName. Run 'cargo build --release' first." -ForegroundColor Red
    exit 1
}
$exePath = (Resolve-Path $exePath).Path
Write-Host "[1/4] Found binary: $exePath"

# Create install directory
Write-Host "[2/4] Installing to: $InstallDir"
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Copy-Item $exePath (Join-Path $InstallDir $ExeName) -Force

# Create config file
$configFile = Join-Path $InstallDir "config.json"
if (-not (Test-Path $configFile)) {
    @{
        addr = "0.0.0.0:$Port"
        db_path = (Join-Path $InstallDir "todo2fast.sqlite")
    } | ConvertTo-Json | Set-Content $configFile -Encoding UTF8
}

# Create Start Menu + Desktop shortcuts
Write-Host "[3/4] Creating shortcuts..."
$WshShell = New-Object -ComObject WScript.Shell

$startMenuDir = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs"
if (-not (Test-Path $startMenuDir)) { New-Item -ItemType Directory -Force -Path $startMenuDir | Out-Null }
$progDir = Join-Path $startMenuDir $AppName
New-Item -ItemType Directory -Force -Path $progDir | Out-Null

$lnk = $WshShell.CreateShortcut((Join-Path $progDir "$AppName.lnk"))
$lnk.TargetPath = Join-Path $InstallDir $ExeName
$lnk.WorkingDirectory = $InstallDir
$lnk.Description = "Todo2fast - Collaborative task manager"
$lnk.Save()

$desktopLnk = $WshShell.CreateShortcut((Join-Path $env:USERPROFILE "Desktop\$AppName.lnk"))
$desktopLnk.TargetPath = Join-Path $InstallDir $ExeName
$desktopLnk.WorkingDirectory = $InstallDir
$desktopLnk.Description = "Todo2fast - Collaborative task manager"
$desktopLnk.Save()

# Optional: register as Windows service (requires admin)
if ($RegisterService) {
    Write-Host "[4/4] Registering Windows service..."
    $isElevated = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    if (-not $isElevated) {
        Write-Host "  [WARN] Not running as admin. Skipping service registration." -ForegroundColor Yellow
        Write-Host "  Run as Administrator to enable: .\install.ps1 -RegisterService"
    } else {
        # Use NSSM-style approach via sc.exe (binary must support --service flag)
        # For now, create a scheduled task that runs at startup
        $taskName = "$AppName Server"
        Unregister-ScheduledTask -TaskName $taskName -Confirm:$false -ErrorAction SilentlyContinue
        Register-ScheduledTask -TaskName $taskName `
            -Action (New-ScheduledTaskAction -Execute (Join-Path $InstallDir $ExeName) -WorkingDirectory $InstallDir) `
            -Trigger (New-ScheduledTaskTrigger -AtStartup) `
            -Settings (New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries) `
            -Description "Todo2fast API server"
        Write-Host "  Scheduled task '$taskName' created (runs at startup)."
    }
} else {
    Write-Host "[4/4] Skipping service registration (use -RegisterService to enable)."
}

Write-Host ""
Write-Host "╔══════════════════════════════════════╗" -ForegroundColor Green
Write-Host "║       Installation complete!       ║" -ForegroundColor Green
Write-Host "╚══════════════════════════════════════╝" -ForegroundColor Green
Write-Host ""
Write-Host "  Binary:     $(Join-Path $InstallDir $ExeName)"
Write-Host "  Config:     $configFile"
Write-Host "  Port:       $Port"
Write-Host "  Start Menu: Yes"
Write-Host "  Desktop:    Yes"
Write-Host ""
Write-Host "  To start:   & '$(Join-Path $InstallDir $ExeName)'"
Write-Host "  To stop:    taskkill /IM $ExeName /F"
Write-Host "  Uninstall:  .\install.ps1 -Uninstall"
Write-Host ""
