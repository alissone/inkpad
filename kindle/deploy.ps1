# Build inkpad and copy it to a Kindle mounted over USB.
# Usage (from the repo root):  powershell -File kindle\deploy.ps1 [-Drive E:] [-NoBuild]
#
# Layout on the Kindle:
#   inkpad/inkpad.sh        launcher (takes and restores the screen)
#   inkpad/home             the home app
#   inkpad/apps/<id>/       installed apps (bundled samples here; your own
#                           apps via scripts\install-app.ps1 <path> -Kindle E:)
#   inkpad/assets/          shared images, wallpapers, fonts
#   inkpad/data/            settings and app data
#   documents/Inkpad.sh     library shortcut that opens home
param([string]$Drive = "E:", [switch]$NoBuild)
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$triple = "armv7-unknown-linux-musleabi"
$bundled = @("apps\gallery", "tools\touchtest", "tools\gfxdemo")
$selfContained = "$env:USERPROFILE\.rustup\toolchains\stable-x86_64-pc-windows-gnu\lib\rustlib\x86_64-pc-windows-gnu\bin\self-contained"
$env:Path = "$env:USERPROFILE\.cargo\bin;$selfContained;$env:Path"

if (-not (Test-Path "$Drive\documents")) { throw "No Kindle at $Drive (is it in USB drive mode?)" }

if (-not $NoBuild) {
    Push-Location $root
    cargo build --release --target $triple -p home -p gallery -p gfxdemo -p touchtest
    $code = $LASTEXITCODE
    Pop-Location
    if ($code -ne 0) { throw "build failed" }
}

# Shell scripts must have LF line endings on the Kindle.
function Copy-Lf($src, $dst) {
    $text = (Get-Content -Raw $src) -replace "`r`n", "`n"
    [IO.File]::WriteAllText($dst, $text)
}

New-Item -ItemType Directory -Force "$Drive\inkpad\apps", "$Drive\inkpad\assets\images", "$Drive\inkpad\assets\wallpapers", "$Drive\inkpad\assets\fonts", "$Drive\inkpad\data" | Out-Null
Copy-Item "$root\target\$triple\release\home" "$Drive\inkpad\home" -Force
foreach ($a in $bundled) { & powershell -File "$root\scripts\install-app.ps1" "$root\$a" -Kindle $Drive -NoBuild }

# Older layouts kept app binaries and shortcuts that are gone now.
foreach ($old in "gallery", "gfxdemo", "touchtest") { Remove-Item "$Drive\inkpad\$old" -ErrorAction SilentlyContinue }
foreach ($old in "InkpadGallery.sh", "InkpadDemo.sh") { Remove-Item "$Drive\documents\$old" -ErrorAction SilentlyContinue }

Copy-Lf "$root\kindle\inkpad\inkpad.sh" "$Drive\inkpad\inkpad.sh"
Get-ChildItem "$root\kindle\documents\*.sh" | ForEach-Object { Copy-Lf $_.FullName "$Drive\documents\$($_.Name)" }
Copy-Item "$root\assets\images\*" "$Drive\inkpad\assets\images\" -Force
# Adds and updates wallpapers; ones you put on the Kindle yourself are kept.
Copy-Item "$root\assets\wallpapers\*" "$Drive\inkpad\assets\wallpapers\" -Force
# Local fonts (not in git, e.g. your own Bookerly copy) go along if present.
if (Test-Path "$root\assets\fonts") { Copy-Item "$root\assets\fonts\*" "$Drive\inkpad\assets\fonts\" -Force }

Get-ChildItem "$Drive\inkpad" -Recurse -File | Where-Object { $_.FullName -notlike "*\data\*" } | Select-Object @{n = "File"; e = { $_.FullName.Substring(3) } }, Length | Format-Table -AutoSize
Write-Host "Deployed. Eject the Kindle, then open 'Inkpad' from the library."
