# Build the Kindle binaries and copy everything to a Kindle mounted over USB.
# Usage (from the repo root):  powershell -File kindle\deploy.ps1 [-Drive E:] [-NoBuild]
param([string]$Drive = "E:", [switch]$NoBuild)
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$apps = @("home", "gallery", "gfxdemo", "touchtest")

if (-not (Test-Path "$Drive\documents")) { throw "No Kindle at $Drive (is it in USB drive mode?)" }

if (-not $NoBuild) {
    Push-Location $root
    $pkgs = $apps | ForEach-Object { "-p"; $_ }
    cargo build --release --target armv7-unknown-linux-musleabi @pkgs
    if ($LASTEXITCODE -ne 0) { Pop-Location; throw "build failed" }
    Pop-Location
}

# Shell scripts must have LF line endings on the Kindle.
function Copy-Lf($src, $dst) {
    $text = (Get-Content -Raw $src) -replace "`r`n", "`n"
    [IO.File]::WriteAllText($dst, $text)
}

New-Item -ItemType Directory -Force "$Drive\inkpad\assets\images", "$Drive\inkpad\assets\wallpapers", "$Drive\inkpad\assets\fonts", "$Drive\inkpad\data" | Out-Null
foreach ($a in $apps) {
    Copy-Item "$root\target\armv7-unknown-linux-musleabi\release\$a" "$Drive\inkpad\$a" -Force
}
Copy-Lf "$root\kindle\inkpad\inkpad.sh" "$Drive\inkpad\inkpad.sh"
Get-ChildItem "$root\kindle\documents\*.sh" | ForEach-Object { Copy-Lf $_.FullName "$Drive\documents\$($_.Name)" }
Copy-Item "$root\assets\images\*" "$Drive\inkpad\assets\images\" -Force
# Adds and updates wallpapers; ones you put on the Kindle yourself are kept.
Copy-Item "$root\assets\wallpapers\*" "$Drive\inkpad\assets\wallpapers\" -Force
# Local fonts (not in git, e.g. your own Bookerly copy) go along if present.
if (Test-Path "$root\assets\fonts") { Copy-Item "$root\assets\fonts\*" "$Drive\inkpad\assets\fonts\" -Force }

Get-ChildItem "$Drive\inkpad" -Recurse -File | Select-Object @{n = "File"; e = { $_.FullName.Substring(3) } }, Length | Format-Table -AutoSize
Write-Host "Deployed. Eject the Kindle, then open the Inkpad shortcuts from the library."
