# Take simulator screenshots of every sample app and screen, or open the simulator.
#
#   powershell -File scripts\screenshots.ps1               # build, shoot all, open contact sheet
#   powershell -File scripts\screenshots.ps1 -Only tasks*  # just the matching shots
#   powershell -File scripts\screenshots.ps1 -NoBuild -NoOpen
#   powershell -File scripts\screenshots.ps1 -Live home    # open the simulator window (mouse = finger, F2 = power)
#   powershell -File scripts\screenshots.ps1 -List         # list shot names
#
# Shots run headless: the simulator replays the INKPAD_TAPS script, writes the
# final screen to <Out>\<name>.png and exits. Coordinates are in 600x800 panel pixels.
param(
    [string]$Only = "*",
    [string]$Out = "",
    [string]$Live = "",
    [int]$Scale = 1,
    [switch]$NoBuild,
    [switch]$NoOpen,
    [switch]$List
)
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
if (-not $Out) { $Out = Join-Path $root "screenshots" }

# GNU toolchain: cargo + bundled dlltool (minifb needs it) on PATH.
$selfContained = "$env:USERPROFILE\.rustup\toolchains\stable-x86_64-pc-windows-gnu\lib\rustlib\x86_64-pc-windows-gnu\bin\self-contained"
$env:Path = "$env:USERPROFILE\.cargo\bin;$selfContained;$env:Path"

# Shared tap positions.
$tab = @{ home = "60,765"; tasks = "180,765"; widgets = "300,765"; fonts = "420,765"; media = "540,765" }
$status = "300,18"        # status bar -> system menu
$plus = "550,70"          # title-bar "+" on Tasks
$kbNum = "60,770"         # keyboard "?123"
$kbAcc = "150,770"        # keyboard accents
$kbShift = "46,700"       # keyboard shift

# Non-ASCII text built from code points: PowerShell 5 reads this file as ANSI.
$agrave = "$([char]0xE0)s"                              # "às"
$acao = "A$([char]0xE7)$([char]0xE3)o"                 # "Ação"

# Home app positions.
$hSettings = "60,682"     # "Settings" in the minimal app list
$hTab = @{ display = "90,146"; wallpaper = "90,202"; lock = "90,262"; sleep = "90,320"; network = "90,378"; about = "90,436" }
$pinOn = "500,200"        # Lock tab: PIN switch
$pin1234 = "272,258;392,258;512,258;272,366"   # on the Settings PIN pad
$lock1234 = "178,376;298,376;418,376;178,486"  # on the lock screen pad
$setPin = "$hSettings;$($hTab.lock);$pinOn;$pin1234;$pin1234"
$wall = '{"wallpaper":"ridges.png"}'

# name, app, description, tap script, [system prefs JSON to start with]
# Every shot runs with its own empty data folder, so your settings are untouched.
$shots = @(
    @("home-minimal", "home", "Home: minimal layout", ""),
    @("home-minimal-wall", "home", "Minimal over a wallpaper", "", $wall),
    @("home-minimal-contours", "home", "Minimal, contours wallpaper", "", '{"wallpaper":"contours.png"}'),
    @("home-cards", "home", "Home: cards layout", "", '{"home.style":"cards"}'),
    @("home-system-menu", "home", "Status tapped: system menu", "560,24"),
    @("settings-display", "home", "Settings: Display", "$hSettings"),
    @("settings-wallpaper", "home", "Settings: Wallpaper", "$hSettings;$($hTab.wallpaper)", $wall),
    @("settings-lock", "home", "Settings: Lock screen", "$hSettings;$($hTab.lock)"),
    @("settings-pin-setup", "home", "Choosing a PIN", "$hSettings;$($hTab.lock);$pinOn;272,258;392,258"),
    @("settings-pin-on", "home", "PIN set", "$setPin"),
    @("settings-sleep", "home", "Settings: Sleep screen", "$hSettings;$($hTab.sleep)"),
    @("settings-network", "home", "Settings: Network", "$hSettings;$($hTab.network)"),
    @("settings-about", "home", "Settings: About", "$hSettings;$($hTab.about)"),
    @("sleep-minimal", "home", "Sleep screen: minimal", "sleep"),
    @("sleep-picture", "home", "Sleep screen: picture", "sleep", '{"wallpaper":"ridges.png","sleep.mode":"picture"}'),
    @("sleep-calendar", "home", "Sleep screen: calendar", "sleep", '{"wallpaper":"dunes.png","sleep.mode":"calendar"}'),
    @("lock-screen", "home", "Woken up with a PIN set", "$setPin;sleep;wake"),
    @("lock-wrong-pin", "home", "Wrong PIN", "$setPin;sleep;wake;178,376;178,376;178,376;178,376"),
    @("lock-wallpaper", "home", "Lock screen over a wallpaper", "$hSettings;$($hTab.lock);$pinOn;$pin1234;$pin1234;sleep;wake;178,376", '{"wallpaper":"contours.png"}'),
    @("home-launch-gallery", "home", "Opened Gallery and came back", "60,566"),

    @("home", "gallery", "Home tab: big clock, cards", ""),
    @("system-menu", "gallery", "Status bar tapped: system menu", "$status"),
    @("tasks", "gallery", "Tasks tab, page 1", "$($tab.tasks)"),
    @("tasks-page2", "gallery", "Tasks swiped to page 2", "$($tab.tasks);swipe:500,400,100,400"),
    @("tasks-toggled", "gallery", "First two tasks ticked", "$($tab.tasks);300,150;wait:100;300,240"),
    @("tasks-hold-toast", "gallery", "Long-press a task: toast", "$($tab.tasks);hold:300,240"),
    @("keyboard", "gallery", "Add-task prompt, empty", "$($tab.tasks);$plus"),
    @("keyboard-typing", "gallery", "Typed text, shift layer on", "$($tab.tasks);$plus;type:Comprar leite;$kbShift"),
    @("keyboard-numbers", "gallery", "Numbers layer", "$($tab.tasks);$plus;type:Ligar $agrave 14;$kbNum"),
    @("keyboard-accents", "gallery", "Accents layer", "$($tab.tasks);$plus;type:$acao;$kbAcc"),
    @("task-added", "gallery", "Task added + toast", "$($tab.tasks);$plus;type:Water the basil;enter"),
    @("widgets", "gallery", "Widgets tab", "$($tab.widgets)"),
    @("widgets-syncing", "gallery", "Background job running", "$($tab.widgets);120,240"),
    @("widgets-switches", "gallery", "Both switches flipped", "$($tab.widgets);300,364;wait:100;300,446"),
    @("widgets-dialog", "gallery", "Modal dialog", "$($tab.widgets);250,194"),
    @("fonts", "gallery", "Fonts tab (Bookerly if in assets/fonts)", "$($tab.fonts)"),
    @("media", "gallery", "Dithered photo + icons", "$($tab.media)"),
    @("media-icons2", "gallery", "Icon grid page 2", "$($tab.media);swipe:500,560,100,560"),
    @("gfxdemo", "gfxdemo", "M2 raw graphics demo", ""),
    @("gfxdemo-tap", "gfxdemo", "M2 demo after a tap", "300,220")
)

if ($List) { $shots | ForEach-Object { "{0,-18} {1,-9} {2}" -f $_[0], $_[1], $_[2] }; return }

$apps = if ($Live) { @($Live) } else { $shots | Where-Object { $_[0] -like $Only } | ForEach-Object { $_[1] } | Select-Object -Unique }
if (-not $apps) { throw "No shot matches '$Only' (use -List)" }

if (-not $NoBuild) {
    Push-Location $root
    $pkgs = $apps | ForEach-Object { "-p"; $_ }
    cargo build --release @pkgs
    $code = $LASTEXITCODE
    Pop-Location
    if ($code -ne 0) { throw "build failed" }
}

if ($Live) {
    Remove-Item Env:INKPAD_SNAPSHOT, Env:INKPAD_TAPS -ErrorAction SilentlyContinue
    $env:INKPAD_SIM_SCALE = "$Scale"
    Push-Location $root  # assets/ is found relative to the working directory
    & "$root\target\release\$Live.exe"
    Pop-Location
    return
}

New-Item -ItemType Directory -Force $Out | Out-Null
Push-Location $root
$done = @()
$data = Join-Path ([IO.Path]::GetTempPath()) "inkpad-shots-data"
foreach ($s in $shots) {
    $name, $app, $desc, $taps, $prefs = $s
    if ($name -notlike $Only) { continue }
    $png = Join-Path $Out "$name.png"
    Remove-Item $png -ErrorAction SilentlyContinue
    Remove-Item $data -Recurse -Force -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force "$data\prefs" | Out-Null
    if ($prefs) { [IO.File]::WriteAllText("$data\prefs\system.json", $prefs) }
    $env:INKPAD_DATA = $data
    $env:INKPAD_SNAPSHOT = $png
    $env:INKPAD_TAPS = $taps
    $ErrorActionPreference = "Continue"  # apps log to stderr; that's not a failure
    $null = & "$root\target\release\$app.exe" 2>&1
    $ErrorActionPreference = "Stop"
    if (Test-Path $png) { Write-Host ("  ok  {0,-18} {1}" -f $name, $desc); $done += , $s }
    else { Write-Host ("  FAIL {0,-18} (no image)" -f $name) -ForegroundColor Red }
}
Remove-Item Env:INKPAD_SNAPSHOT, Env:INKPAD_TAPS, Env:INKPAD_DATA -ErrorAction SilentlyContinue
Remove-Item $data -Recurse -Force -ErrorAction SilentlyContinue
Pop-Location

# Contact sheet.
$cards = ($done | ForEach-Object {
        "<figure><img src='$($_[0]).png' alt='$($_[0])'><figcaption><b>$($_[0])</b><br>$($_[2])</figcaption></figure>"
    }) -join "`n"
$html = @"
<!doctype html><meta charset="utf-8"><title>inkpad screenshots</title>
<style>
body{font:14px system-ui,sans-serif;background:#ddd;margin:24px}
h1{font-weight:600}
main{display:grid;grid-template-columns:repeat(auto-fill,minmax(300px,1fr));gap:20px}
figure{margin:0;background:#fff;padding:10px;border-radius:8px;box-shadow:0 1px 3px #0003}
img{width:100%;image-rendering:auto;border:1px solid #bbb}
figcaption{margin-top:6px;color:#333}
</style>
<h1>inkpad simulator &middot; $(Get-Date -Format "yyyy-MM-dd HH:mm")</h1>
<main>
$cards
</main>
"@
$index = Join-Path $Out "index.html"
[IO.File]::WriteAllText($index, $html)
Write-Host "$($done.Count) screenshots in $Out"
if (-not $NoOpen) { Start-Process $index }
