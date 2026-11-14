# Build an inkpad app and install it where home finds it.
#
#   powershell -File scripts\install-app.ps1 ..\inkpad-library             # simulator
#   powershell -File scripts\install-app.ps1 ..\inkpad-library -Kindle E:  # Kindle over USB
#   powershell -File scripts\install-app.ps1 apps\gallery -NoBuild
#
# <Path> is the app's project folder: it holds app.json (the manifest) and
# Cargo.toml, and may hold assets/ (copied along). The app can be in this
# workspace or in its own repository.
#
# Installs to <apps dir>\<id>\ : the binary, app.json and assets\.
#   simulator: $env:INKPAD_APPS, else %LOCALAPPDATA%\inkpad\apps
#   Kindle:    <drive>\inkpad\apps
param(
    [Parameter(Mandatory = $true, Position = 0)][string]$Path,
    [string]$Kindle = "",
    [switch]$NoBuild,
    [switch]$Quiet
)
$ErrorActionPreference = "Stop"
$selfContained = "$env:USERPROFILE\.rustup\toolchains\stable-x86_64-pc-windows-gnu\lib\rustlib\x86_64-pc-windows-gnu\bin\self-contained"
$env:Path = "$env:USERPROFILE\.cargo\bin;$selfContained;$env:Path"

$proj = (Resolve-Path $Path).Path
if (-not (Test-Path "$proj\app.json")) { throw "$proj has no app.json" }
$manifest = Get-Content -Raw "$proj\app.json" | ConvertFrom-Json
$id = $manifest.id
if (-not $id) { throw "app.json has no id" }
$exec = if ($manifest.exec) { $manifest.exec } else { $id }
$triple = "armv7-unknown-linux-musleabi"

# Where cargo puts the binary (a workspace member builds into the workspace's target\).
Push-Location $proj  # cargo reads .cargo\config.toml (Kindle linker) from here
try {
    $ErrorActionPreference = "Continue"
    $meta = cargo metadata --format-version 1 --no-deps 2>$null | ConvertFrom-Json
    $ErrorActionPreference = "Stop"
    $targetDir = $meta.target_directory
    if (-not $NoBuild) {
        $args_ = @("build", "--release", "--bin", $exec)
        if ($Kindle) { $args_ += @("--target", $triple) }
        $ErrorActionPreference = "Continue"
        & cargo @args_
        $code = $LASTEXITCODE
        $ErrorActionPreference = "Stop"
        if ($code -ne 0) { throw "build failed" }
    }
} finally { Pop-Location }

if ($Kindle) {
    if (-not (Test-Path "$Kindle\documents")) { throw "No Kindle at $Kindle (is it in USB drive mode?)" }
    $bin = "$targetDir\$triple\release\$exec"
    $dest = "$Kindle\inkpad\apps\$id"
} else {
    $bin = "$targetDir\release\$exec.exe"
    $root = if ($env:INKPAD_APPS) { $env:INKPAD_APPS } else { "$env:LOCALAPPDATA\inkpad\apps" }
    $dest = "$root\$id"
}
if (-not (Test-Path $bin)) { throw "binary not found: $bin" }

New-Item -ItemType Directory -Force $dest | Out-Null
Copy-Item $bin $dest -Force
Copy-Item "$proj\app.json" "$dest\app.json" -Force
if (Test-Path "$proj\assets") { Copy-Item "$proj\assets" $dest -Recurse -Force }
if (-not $Quiet) { Write-Host "Installed $($manifest.name) ($id) to $dest" }
