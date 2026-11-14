# inkpad

**A native UI framework for jailbroken e-ink Kindles, written in Rust.**

inkpad turns a Kindle into a modern black-and-white touchscreen PDA: small, fast, mostly read-only apps for the things you glance at during the day. Tasks, calendar, reminders, an AI assistant, nutrition charts. No browser, no web views. inkpad is a single static binary that draws directly to the e-ink panel and launches from the Kindle library, just like KOReader.

> **Status: early development.** The hardware layer works on a real device: drawing, e-ink refresh and multitouch. The widget toolkit and the first app are next. "inkpad" is a working name.

## Disclaimer

I am not a Rust developer, so almost 100% of the code is written by coding agents. With that being said, all feature planning, design choices, ui layout, ideas for widgets are done by me. I am also open to PRs, ideas for new widgets and applications using them!


---

## Why

E-ink devices are ideal for calm, ambient information. They're readable in sunlight, last weeks on a charge, and nothing flashes or demands attention. Old Kindles are cheap and everywhere, but their software only reads books. KOReader showed that a jailbroken Kindle can run serious native software. inkpad aims to be the *Flutter* of that world: a toolkit with real widgets, designed for e-ink from the start, so building a useful little app takes an afternoon.

### Design principles
- **E-ink first.** No animations or scrolling: content is paged. Each screen update picks the right refresh waveform: fast black-and-white for touch feedback, full grayscale for content, and an occasional full flash to clear ghosting.
- **One static binary.** It has no dependency on the Kindle's old system libraries, so it's easy to install and hard to break.
- **Small and frugal.** It targets a 1 GHz single-core CPU with 512 MB RAM, and uses Wi-Fi only on demand.
- **The stock Kindle is always safe.** The launcher always gives the screen back to the Kindle's own UI, even if an app crashes.
- **Permissive license.** Dual MIT/Apache-2.0, with no GPL code, so anyone can build on it.

---

## Supported devices

| Device | Status |
|---|---|
| Kindle basic 8th gen (KT3, 2016), firmware 5.16.x | ✅ Development device: display and touch working |
| Other i.MX6 `mxcfb` Kindles (Paperwhite 2/3, Voyage, Oasis 1, Touch 2) | 🔜 Should work with small changes; untested |
| Newer MediaTek Kindles (PW5, Scribe, …) | ❌ Different display driver; not planned yet |

The device must be **jailbroken** (see [kindlemodding.org](https://kindlemodding.org)). Hardware details for the KT3 are in [`docs/device-kt3.md`](docs/device-kt3.md).

---

## Project layout

```
inkpad/
├── crates/
│   ├── inkpad/          The crate apps depend on; picks Kindle hardware or the simulator
│   ├── inkpad-hal/      Hardware layer: e-ink framebuffer (mxcfb ioctls) and evdev multitouch
│   ├── inkpad-gfx/      Drawing (anti-aliased shapes), text (font families, wrapping, ellipsis), PNG/SVG images
│   ├── inkpad-ui/       The toolkit: elements, layout, widgets, keyboard, icons, clock, runtime,
│   │                    and the system UI (status bar, sleep screen, PIN lock screen)
│   ├── inkpad-store/    Storage like Android's SharedPreferences: JSON key/value files + text files
│   └── inkpad-sim/      Desktop simulator: e-ink-like window, mouse = finger, headless PNG snapshots
├── apps/
│   ├── home/            The launcher: minimal or cards layout, Settings, launches the other apps
│   └── gallery/         Every widget on five tabs: Home (clock), Tasks, Widgets, Fonts, Media
├── tools/
│   ├── hello/           M0: checks that the binary runs and reports device facts
│   ├── touchtest/       M1: draw-with-your-finger test for display + touch
│   └── gfxdemo/         M2: a hand-drawn Tasks screen (runs on Kindle and in the simulator)
├── assets/              Images, wallpapers (and your own fonts, git-ignored) shipped to /mnt/us/inkpad/assets
├── kindle/
│   ├── inkpad/          Files copied to /mnt/us/inkpad/ (launcher script, binaries)
│   └── documents/       Library shortcuts copied to the Kindle's documents/ folder
└── docs/                Device notes and design docs
```

`inkpad-net`: HTTPS (via curl) and Wi-Fi on demand.

### Home, lock and sleep

`home` is what the **Inkpad** shortcut opens. Each app is its own binary; when home launches one (`ctx.launch("gallery", &[], Msg::Closed)`, where `Closed` takes the exit result), it hands over the screen and touch until that app exits, then redraws. KOReader and "Kindle" leave inkpad.

Every app gets these from the runtime for free:

- **Sleep screen:** when the Kindle goes to sleep, the screen shows the sleep screen chosen in Settings (Minimal, Picture or Calendar), or the app's own `sleep_view`.
- **PIN lock:** if a PIN is set, the lock screen appears on wake, whichever app is open. The PIN is stored salted and hashed. It keeps casual hands out; it is not encryption.
- **Settings storage:** `Prefs::open("myapp")` gives an app its own key/value file under `inkpad/data/prefs/`; `Prefs::system()` holds the shared settings. Writes are atomic.

```rust
let mut prefs = Prefs::open("tasks");
let n = prefs.get_i64("launches", 0);
prefs.set("launches", n + 1); // saved immediately
```

In the simulator, **F2** is the power button. Simulator data lives in `data/` (git-ignored).

---

### Apps live in their own repositories

inkpad is the system: the framework, home, Settings and the system UI. Apps such as [Library](../inkpad-library) and the upcoming Tasks app are separate projects that depend on the `inkpad` crate, the way phone apps depend on an SDK. Each app project has:

- `Cargo.toml` with `inkpad = { path = "../inkpad/crates/inkpad" }` (a git dependency once inkpad is published);
- `.cargo/config.toml`, copied from inkpad, for the Kindle linker;
- `app.json`, a manifest such as `{ "id": "library", "name": "Library", "exec": "library", "icon": "library-big" }`.

`scripts\install-app.ps1 <app folder> [-Kindle E:]` builds the app and copies it to `apps/<id>/`, together with its manifest and `assets/`. That folder is `%LOCALAPPDATA%\inkpadpps` in the simulator and `/mnt/us/inkpad/apps` on the Kindle. Home lists whatever is installed there.

Apps also get two system services:

- **Networking** (`inkpad::net`): HTTPS requests through the system's `curl`, so no TLS library has to be cross-compiled. On the Kindle a current CA bundle (`assets/certs/cacert.pem`) is passed to curl. `net::ensure_online` switches Wi-Fi on only while it's needed, and there's a clock check.
- **Glances** (`ui::glance`): an app publishes a short summary (for example Tasks: "6 open" and the next tasks). Home shows it on the minimal and cards layouts, and the **Agenda** sleep screen lists it.

An app can hand the device to KOReader with `handoff::open_in_koreader(ctx, Some(file), Some("my-app"))`. inkpad's launcher runs KOReader, and when KOReader closes it comes back to home and reopens that app. `handoff::open_in_kindle_reader` opens Amazon books in the stock reader; that one is experimental.

## Writing an app

An app is a state struct. `view` turns the state into an element tree, and `update` changes the state when a message arrives. The runtime does the rest:
- it re-renders off-screen;
- it finds which pixels changed;
- it refreshes only those parts of the panel, with the right waveform;
- it flashes the whole screen now and then to clear ghosting.

```rust
use inkpad::prelude::*;

#[derive(Clone)]
enum Msg { Toggle(usize), Add, Added(Option<String>) }

struct Todo { items: Vec<(String, bool)> }

impl App for Todo {
    type Msg = Msg;

    fn view(&self, _env: &Env) -> El<Msg> {
        let rows = self.items.iter().enumerate().map(|(i, (t, done))| {
            row(vec![checkbox(*done), text(t.clone()).fill_w()]).gap(16).pad(28, 14).on_tap(Msg::Toggle(i))
        }).collect();
        col(vec![title_bar("Todo", None, vec![icon_button("plus", Msg::Add)]), col(rows)])
    }

    fn update(&mut self, msg: Msg, ctx: &mut Ctx<Msg>) {
        match msg {
            Msg::Toggle(i) => self.items[i].1 ^= true,
            Msg::Add => ctx.prompt("New task", "", Msg::Added),       // on-screen keyboard
            Msg::Added(Some(t)) => { self.items.push((t, false)); ctx.toast("Added"); }
            Msg::Added(None) => {}
        }
    }
}

fn main() { inkpad::run(Todo { items: vec![] }).unwrap() }
```

| Building blocks | |
|---|---|
| Layout | `col`, `row`, `stack`, `paged` (swipe or ‹ › to change page), `spacer`, `divider`; `.gap`, `.pad`, `.fill_w`, `.w(px)`, `.align`, `.justify` |
| Content | `text` (`.size`, `.bold`, `.italic`, `.family`, `.max_lines`), `icon(name, px)`, `image(...)`, `canvas(w, h, draw)` for charts |
| Widgets | `button`, `button_outline`, `button_icon`, `icon_button`, `checkbox`, `toggle`, `title_bar`, `card`, `chip`, `progress`, `dialog` |
| Interaction | `.on_tap(msg)`, `.on_hold(msg)` (long press), with instant inverted press feedback |
| `Ctx` commands | `prompt` (keyboard), `toast`, `after(duration, msg)`, `spawn(background job)`, `full_refresh`, `quit` |
| Built in | status bar with clock, Wi-Fi and battery. Tap it for the system menu (refresh screen, exit to Kindle) |

### Fonts, icons, images
- **Fonts:** Atkinson Hyperlegible (SIL OFL) is bundled. Load any other TTF/OTF family at runtime with `assets::load_font(fonts, "Bookerly", "Bookerly")` in `App::setup`. It searches:
  - `/mnt/us/inkpad/assets/fonts`
  - `/mnt/us/fonts`
  - the Kindle system fonts
  - `assets/fonts` on a PC

  Characters a font lacks fall back to the bundled one. **Never commit proprietary fonts:** `assets/fonts/` is git-ignored.
- **Icons:** 39 [Lucide](https://lucide.dev) icons (ISC license) are bundled and drawn from SVG at any size. See `inkpad_ui::icons::NAMES`.
- **Images:** load PNG and SVG with `assets::image("images/x.png", max_w, max_h, dither)`. Dithering to the panel's 16 gray levels makes photos look smooth.

---

## Building

You only need **Rust** ([rustup](https://rustup.rs)). The Kindle binary links with the bundled `rust-lld` against Rust's self-contained musl libc, so no cross-compiler is required.

```sh
rustup target add armv7-unknown-linux-musleabi

cargo kindle -p touchtest      # alias: build --release --target armv7-unknown-linux-musleabi
cargo test                     # host-side unit tests
```

Binaries end up in `target/armv7-unknown-linux-musleabi/release/`.

### Simulator

```sh
cargo run --release -p gallery                 # opens a 600×800 window; mouse = finger, Esc quits
INKPAD_SIM_SCALE=2 cargo run --release -p gallery
INKPAD_SNAPSHOT=out.png INKPAD_TAPS="182,760;hold:300,250" cargo run --release -p gallery   # headless
```

In the window, typing on your physical keyboard fills text prompts. In headless mode, `INKPAD_TAPS` is a `;`-separated script with these steps:

| Step | Does |
|---|---|
| `x,y` | tap |
| `hold:x,y` | long press |
| `swipe:x0,y0,x1,y1` | swipe |
| `type:text` | type text |
| `enter` | press Enter |
| `back` | backspace |
| `wait:ms` | pause |

The last frame is saved to the PNG.

**Screenshots of everything:** `scripts\screenshots.ps1` builds the sample apps, captures every screen (each tab, the keyboard layers, dialog, toast, system menu, paging, the M2 demo) into `screenshots\`, and opens a contact sheet (`index.html`).

```powershell
powershell -File scripts\screenshots.ps1                # all shots
powershell -File scripts\screenshots.ps1 -Only widgets* # matching shots only
powershell -File scripts\screenshots.ps1 -List          # shot names
powershell -File scripts\screenshots.ps1 -Live gallery  # just open the simulator window (-Scale 2 to enlarge)
```

To add a shot, add a line to the `$shots` table in the script.

The simulator imitates the panel: content refreshes show 16 gray levels, fast (DU) refreshes show pure black and white, and full refreshes flash.

> **Windows note:** if the MSVC linker fails (for example, the Windows SDK isn't installed), use the GNU host toolchain: `rustup default stable-x86_64-pc-windows-gnu`. Also put that toolchain's bundled `dlltool` on PATH: `%USERPROFILE%\.rustup\toolchains\stable-x86_64-pc-windows-gnu\lib\rustlib\x86_64-pc-windows-gnu\bin\self-contained`.

---

## Installing on the Kindle

Connect the Kindle by USB. It mounts as a drive, which is `/mnt/us` on the device. The quick way, on Windows, is to run `powershell -File kindle\deploy.ps1` (it builds everything and copies it to `E:`). `powershell -File kindle\deploy-all.ps1` does that and also installs the app projects kept next to inkpad (`inkpad-library`, `inkpad-tasks`; change the list with `-Apps`). To do it by hand:

1. Copy `kindle/inkpad/inkpad.sh` and the built binaries into an `inkpad/` folder at the drive's root.
2. Copy the shortcuts from `kindle/documents/` into the drive's `documents/` folder.
3. Eject the Kindle. The shortcuts appear in the library: **Inkpad touch test**, **Inkpad hello test**.

> Shell scripts **must use LF line endings**. A script saved with Windows CRLF line endings fails silently on the Kindle.

### How launching works
Each library shortcut runs `inkpad/inkpad.sh <binary>`, which:
1. saves the current screen;
2. disables the stock status bar (pillow);
3. pauses the window manager (`awesome`);
4. pauses `volumd`, so USB storage can't unmount the disk mid-run;
5. runs the app;
6. **always** undoes all of the above, using a shell `trap`, even if the app crashes.

The output goes to `inkpad/inkpad.log`.

### If something goes wrong
Hold the power button for about 15 seconds to restart the Kindle. Nothing is lost, and inkpad never modifies the system partition. To uninstall, delete the `inkpad/` folder and the `Inkpad*.sh` shortcuts.

---

## Testing the hardware layer (touchtest)

Open **Inkpad touch test**:
- Draw with your finger. Lines should follow it exactly.
- **Top-right X box:** exits cleanly.
- **Top-left striped box:** crashes on purpose, to prove the launcher still restores the Kindle UI.
- **Bottom strip:** 16 gray levels, black on the left to white on the right.
- Two fingers at once also exits, and so does 60 s without a touch.

Environment overrides for other devices (set them in `inkpad.sh`):

| Variable | Effect |
|---|---|
| `INKPAD_INVERT=1` | Panel stores 0 as white; invert on copy |
| `INKPAD_TOUCH=swap,mirror_x,mirror_y` | Fix rotated or mirrored touch coordinates |
| `INKPAD_TRACE=1` | Log every raw input event |

---

## Roadmap

- [x] **M0, toolchain:** a static Rust binary runs on the Kindle (kernel 3.10, soft-float ARMv7)
- [x] **M1, hardware layer:** framebuffer and e-ink refresh ioctls, multitouch, a safe launcher
- [x] **M2, graphics + simulator:** canvas, font rendering and text layout, and a desktop window that emulates the e-ink screen, so most development happens on a PC
- [x] **M3, UI toolkit:** a declarative widget tree (`view(state) → widgets`, `update(msg)`), layout, a refresh scheduler, and widgets: Text, Button, Checkbox row, TitleBar, PagedList, Dialog, Toast, on-screen Keyboard
- [x] **M4, Tasks app** ([inkpad-tasks](../inkpad-tasks)): a to-do list synced with a Supabase backend: tick tasks done, add new ones with the keyboard, and keep an offline cache
- [x] **Home screen:** launcher (minimal and cards layouts), Settings, wallpapers, sleep screen, PIN lock, preferences storage
- [x] **Apps as separate projects:** app manifests, `install-app.ps1`, KOReader handoff; apps: [Library](../inkpad-library), [Tasks](../inkpad-tasks)
- [ ] **Later:**
  - home widgets: calendar agenda, weather, next tasks;
  - a Claude assistant app;
  - a calendar from an iCal URL;
  - reminders and nutrition charts;
  - an SDK for third-party apps, and docs.

---

## Contributing

The project is early and the APIs will change. Issues and ideas are welcome, especially from people with other Kindle models who can run `hello` and `touchtest` and share their logs.

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT), at your option. Unless you explicitly state otherwise, any contribution you submit for inclusion is dual-licensed as above, without additional terms.

Bundled third-party assets: Atkinson Hyperlegible font (SIL Open Font License 1.1, `crates/inkpad-gfx/fonts/OFL.txt`), Lucide icons (ISC, `crates/inkpad-ui/icons/LICENSE`).

inkpad is not affiliated with Amazon. Kindle is a trademark of Amazon.com, Inc.
