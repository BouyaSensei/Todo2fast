# build-installer.ps1 — génère l'installateur Windows dans installer\Output\
# Usage (PowerShell) :  .\installer\build-installer.ps1
#
# Étapes :
#   1. build du frontend (frontend\dist) — requis, sinon l'app installée n'a pas d'UI
#   2. build release du backend (backend\target\release\todo2fast.exe)
#   3. compilation de l'installateur Inno Setup dans installer\Output\

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot   # racine du repo
Set-Location $root

Write-Host "==> [1/3] Build frontend (npm run build)" -ForegroundColor Cyan
Push-Location "$root\frontend"
try {
    npm ci
    if ($LASTEXITCODE -ne 0) { throw "npm ci a échoué" }
    npm run build
    if ($LASTEXITCODE -ne 0) { throw "npm run build a échoué" }
} finally { Pop-Location }

Write-Host "==> [2/3] Build backend release (cargo build --release)" -ForegroundColor Cyan
Push-Location "$root\backend"
try {
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build --release a échoué" }
} finally { Pop-Location }

# Inno Setup peut vivre dans Program Files (x86), Program Files, ou AppData\Local\Programs
# (winget l'installe souvent au niveau utilisateur). On cherche le bon ISCC.exe.
$iscCandidates = @(
    "C:\Program Files (x86)\Inno Setup 6\ISCC.exe",
    "C:\Program Files\Inno Setup 6\ISCC.exe",
    "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
)
$iscc = $iscCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $iscc) {
    throw "ISCC.exe (Inno Setup 6) introuvable. Installe-le avec : winget install JRSoftware.InnoSetup"
}

Write-Host "==> [3/3] Compilation de l'installateur ($iscc)" -ForegroundColor Cyan
& $iscc "$root\installer\todo2fast.iss"
if ($LASTEXITCODE -ne 0) { throw "La compilation Inno Setup a échoué" }

$setup = Get-ChildItem "$root\installer\Output\*.exe" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
Write-Host ""
Write-Host "Installer prêt : $($setup.FullName)" -ForegroundColor Green
Write-Host "Taille : {0:N1} Mo" -f ($setup.Length / 1MB)
