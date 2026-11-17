# Run the Library in the inkpad simulator, take screenshots, or install it.
#
#   powershell -File scripts\dev.ps1                       # open the simulator window
#   powershell -File scripts\dev.ps1 -Documents D:\books   # ...on another books folder
#   powershell -File scripts\dev.ps1 -Shots                # screenshots into screenshots\
#   powershell -File scripts\dev.ps1 -Install              # install into the simulator's home
#   powershell -File scripts\dev.ps1 -Install -Kindle E:   # install on the Kindle
#
# Books: -Documents, else $env:INKPAD_DOCUMENTS, else sample-books\ (empty
# until you put some EPUB/MOBI/PDF files there). A copy of a Kindle's
# documents\ folder works great: covers come from system\thumbnails next to it.
param([string]$Documents = "", [switch]$Shots, [switch]$Install, [string]$Kindle = "")
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
# This app lives in the inkpad workspace (inkpad\apps\library); builds go to its shared target\.
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
cargo build --release -p library
$code = $LASTEXITCODE
$ErrorActionPreference = "Stop"
Pop-Location
if ($code -ne 0) { throw "build failed" }
if ($Documents) { $env:INKPAD_DOCUMENTS = (Resolve-Path $Documents).Path }
$exe = "$Inkpad\target\release\library.exe"

if (-not $Shots) {
    Remove-Item Env:INKPAD_SNAPSHOT, Env:INKPAD_TAPS -ErrorAction SilentlyContinue
    & $exe
    return
}

# Screenshots run with their own data folder (index, covers, settings).
$out = "$root\screenshots"
$data = Join-Path ([IO.Path]::GetTempPath()) "inkpad-library-shots"
New-Item -ItemType Directory -Force $out | Out-Null
Remove-Item -Recurse -Force $data -ErrorAction SilentlyContinue
$env:INKPAD_DATA = $data
$wait = (1..60 | ForEach-Object { "wait:100" }) -join ";"
$shotList = @(
    @("index", "First start: indexing everything", $wait, ""),
    @("grid", "Cover grid, most recent first", "wait:200", '{"view":"grid"}'),
    @("grid-title", "Sorted by title", "wait:200", '{"view":"grid","sort":1}'),
    @("details", "Hold a book: details", "wait:200;hold:100,300", '{"view":"grid"}'),
    @("list", "List view", "wait:200", '{"view":"list"}'),
    @("list-author", "List, sorted by author", "wait:200", '{"view":"list","sort":2}'),
    @("folders", "Folders view", "wait:200", '{"view":"folders"}'),
    @("folder-open", "Inside a folder", "wait:200;300,300", '{"view":"folders"}'),
    @("search", "Search", "wait:200;500,70;type:lucado;enter", '{"view":"grid"}')
)
foreach ($s in $shotList) {
    $name, $desc, $taps, $prefs = $s
    New-Item -ItemType Directory -Force "$data\prefs" | Out-Null
    if ($prefs) { [IO.File]::WriteAllText("$data\prefs\library.json", $prefs) }
    $env:INKPAD_SNAPSHOT = "$out\$name.png"
    $env:INKPAD_TAPS = $taps
    $ErrorActionPreference = "Continue"
    $null = & $exe 2>&1
    $ErrorActionPreference = "Stop"
    Write-Host ("  {0,-12} {1}" -f $name, $desc)
}
Remove-Item Env:INKPAD_SNAPSHOT, Env:INKPAD_TAPS, Env:INKPAD_DATA -ErrorAction SilentlyContinue
Write-Host "Screenshots in $out"
