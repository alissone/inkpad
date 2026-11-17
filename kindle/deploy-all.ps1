# Deploy inkpad and every app onto the Kindle, in one go.
# Usage (from anywhere):  powershell -File kindle\deploy-all.ps1 [-Drive E:] [-NoBuild] [-Apps ..\my-app,D:\other-app]
#
# 1. kindle\deploy.ps1: home, this workspace's apps (Library, News,
#    Calculator, samples) and the shared assets.
# 2. Apps that live in other repositories: the folders given with -Apps, plus
#    the ones listed in kindle\extra-apps.txt (one folder per line, relative
#    to the inkpad folder; git-ignored, so your private apps stay private).
#    A folder that isn't there is skipped with a note.
param(
    [string]$Drive = "E:",
    [switch]$NoBuild,
    [string[]]$Apps = @()
)
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent

if (-not (Test-Path "$Drive\documents")) { throw "No Kindle at $Drive (is it in USB drive mode?)" }

Write-Host "== inkpad ==" -ForegroundColor Cyan
$a = @("-Drive", $Drive)
if ($NoBuild) { $a += "-NoBuild" }
& powershell -File "$PSScriptRoot\deploy.ps1" @a
if ($LASTEXITCODE -ne 0) { throw "inkpad deploy failed" }

$list = "$PSScriptRoot\extra-apps.txt"
if (Test-Path $list) {
    $Apps += Get-Content $list | ForEach-Object { $_.Trim() } | Where-Object { $_ -and -not $_.StartsWith("#") }
}

$installed = @()
$skipped = @()
$failed = @()
foreach ($app in $Apps) {
    $dir = if ([IO.Path]::IsPathRooted($app)) { $app } else { Join-Path $root $app }
    if (-not (Test-Path "$dir\app.json")) {
        Write-Host "== ${app}: not found (no app.json in $dir), skipped ==" -ForegroundColor Yellow
        $skipped += $app
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
$extra = if ($installed) { "; also installed: $($installed -join ', ')" } else { "" }
Write-Host "inkpad and its apps deployed$extra" -ForegroundColor Green
if ($skipped) { Write-Host "Skipped (not found): $($skipped -join ', ')" -ForegroundColor Yellow }
if ($failed) { Write-Host "Failed: $($failed -join ', ')" -ForegroundColor Red; exit 1 }
Write-Host "Eject the Kindle, then open 'Inkpad' from the library."
