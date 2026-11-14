# Deploy inkpad and install your apps onto the Kindle, in one go.
# Usage (from anywhere):  powershell -File kindle\deploy-all.ps1 [-Drive E:] [-NoBuild] [-Apps inkpad-library,inkpad-tasks,inkpad-calculator,inkpad-news]
#
# Runs kindle\deploy.ps1 (home, bundled samples, assets), then installs each app
# project found next to inkpad (Projetos\inkpad-library, Projetos\inkpad-tasks...).
param(
    [string]$Drive = "E:",
    [switch]$NoBuild,
    [string[]]$Apps = @("inkpad-library", "inkpad-tasks", "inkpad-calculator", "inkpad-news")
)
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$projects = Split-Path $root -Parent

if (-not (Test-Path "$Drive\documents")) { throw "No Kindle at $Drive (is it in USB drive mode?)" }

Write-Host "== inkpad ==" -ForegroundColor Cyan
$a = @("-Drive", $Drive)
if ($NoBuild) { $a += "-NoBuild" }
& powershell -File "$PSScriptRoot\deploy.ps1" @a
if ($LASTEXITCODE -ne 0) { throw "inkpad deploy failed" }

$installed = @()
$failed = @()
foreach ($app in $Apps) {
    $dir = if (Test-Path $app) { (Resolve-Path $app).Path } else { Join-Path $projects $app }
    if (-not (Test-Path "$dir\app.json")) {
        Write-Host "== ${app}: not found at $dir, skipped ==" -ForegroundColor Yellow
        $failed += $app
        continue
    }
    Write-Host "== $app ==" -ForegroundColor Cyan
    $a = @($dir, "-Kindle", $Drive)
    if ($NoBuild) { $a += "-NoBuild" }
    & powershell -File "$root\scripts\install-app.ps1" @a
    if ($LASTEXITCODE -ne 0) { Write-Host "== ${app}: install failed ==" -ForegroundColor Red; $failed += $app }
    else { $installed += $app }
}

Write-Host ""
Write-Host "inkpad deployed; apps installed: $($installed -join ', ')" -ForegroundColor Green
if ($failed) { Write-Host "Not installed: $($failed -join ', ')" -ForegroundColor Red; exit 1 }
Write-Host "Eject the Kindle, then open 'Inkpad' from the library."
