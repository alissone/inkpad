# Run News in the inkpad simulator, or install it.
#
#   powershell -File scripts\dev.ps1                      # simulator
#   powershell -File scripts\dev.ps1 -Install             # into the simulator's home screen
#   powershell -File scripts\dev.ps1 -Install -Kindle E:  # onto the Kindle
param([switch]$Install, [string]$Kindle = "")
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
# This app lives in the inkpad workspace (inkpad\apps\news); builds go to its shared target\.
$Inkpad = Split-Path (Split-Path $root -Parent) -Parent
$selfContained = "$env:USERPROFILE\.rustup\toolchains\stable-x86_64-pc-windows-gnu\lib\rustlib\x86_64-pc-windows-gnu\bin\self-contained"
$env:Path = "$env:USERPROFILE\.cargo\bin;$selfContained;$env:Path"

if ($Install) {
    $a = @("$root")
    if ($Kindle) { $a += @("-Kindle", $Kindle) }
    & powershell -File "$Inkpad\scripts\install-app.ps1" @a
    return
}

Push-Location $root
$ErrorActionPreference = "Continue"
cargo build --release -p news
$code = $LASTEXITCODE
$ErrorActionPreference = "Stop"
Pop-Location
if ($code -ne 0) { throw "build failed" }

Remove-Item Env:INKPAD_SNAPSHOT, Env:INKPAD_TAPS -ErrorAction SilentlyContinue
& "$Inkpad\target\release\news.exe"
