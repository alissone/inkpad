//! inkpad home: the launcher you land on. Two layouts (Settings → Display):
//!
//! - **Minimal**: a framed clock, the month at a glance and a plain list of
//!   apps over an optional wallpaper (Niagara-style).
//! - **Cards**: status bar, big clock, calendar and device cards, and an
//!   icon grid (Android/Boox-style).
//!
//! Apps are separate binaries; launching one hands it the screen until it
//! exits, then home redraws. Settings, the PIN lock and the sleep screen are
//! configured here and stored in the shared system preferences.

mod apps;
mod settings;

use apps::{Action, AppEntry};
use inkpad::prelude::*;
use inkpad::ui::{clock, sysui, sysui::keys};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Screen {
    Home,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Display,
    Wallpaper,
    Lock,
    Sleep,
    Network,
    About,
}

#[derive(Clone, Debug)]
pub enum Msg {
    Open(usize),
    Launched(&'static str, Result<i32, String>),
    StatusTap,
    AppPage(usize),
    // Settings
    Back,
    Tab(Tab),
    HomeStyle(usize),
    DevApps,
    Wallpaper(usize),
    Fade(usize),
    SleepMode(usize),
    PreviewSleep(bool),
    SetPin,
    RemovePin,
    LockNow,
    PinDigit(u8),
    PinBack,
    PinCancel,
    Wifi,
    RefreshScreen,
}

/// Choosing a new PIN: first entry, then confirmation.
pub struct PinSetup {
    first: Option<String>,
    entered: String,
    mismatch: bool,
}

pub struct Home {
    screen: Screen,
    tab: Tab,
    prefs: Prefs,
    apps: Vec<AppEntry>,
    page: usize,
    walls: Vec<String>,
    pin_setup: Option<PinSetup>,
    preview_sleep: bool,
}

const DEV_APPS: &str = "home.dev_apps";

impl Home {
    fn new() -> Self {
        let prefs = Prefs::system();
        let apps = apps::list(prefs.get_bool(DEV_APPS, false));
        Home { screen: Screen::Home, tab: Tab::Display, prefs, apps, page: 0, walls: sysui::wallpapers(), pin_setup: None, preview_sleep: false }
    }

    fn minimal(&self) -> bool {
        self.prefs.get_str(keys::HOME_STYLE).unwrap_or("minimal") != "cards"
    }

    fn reload(&mut self) {
        self.prefs.refresh();
        self.apps = apps::list(self.prefs.get_bool(DEV_APPS, false));
        self.walls = sysui::wallpapers();
    }

    // ---- minimal layout ----

    fn minimal_view(&self, env: &Env) -> El<Msg> {
        let now = env.now;
        let wall = sysui::wallpaper(&self.prefs, env.width, env.height);
        // On a wallpaper, text sits on small white plates to stay readable.
        let plate = |e: El<Msg>, px: u32, py: u32| if wall.is_some() { e.pad(px, py).bg(WHITE).radius(12.0) } else { e };
        let status = row(vec![spacer(), plate(sysui::status_icons(env.battery, env.wifi, 16), 10, 4)]).pad(18, 10).on_tap(Msg::StatusTap);
        let dim = clock::days_in_month(now.year, now.month);
        let month = row(vec![
            col(vec![
                text(now.month_name().to_uppercase()).size(30).bold(),
                row(vec![canvas(10, 10, |p, r| p.circle(r.x as i32 + 5, r.y as i32 + 5, 4, BLACK, None)), text(format!("{} / {}", now.day, dim)).size(18).bold()]).gap(8),
            ])
            .gap(2)
            .align(Align::End),
            rule(3, BLACK).h(64),
            col(vec![sysui::month_dots(&now, 11, 9), text(now.weekday_name().to_uppercase()).size(14).bold().color(60)]).gap(4),
        ])
        .gap(18);
        let list: Vec<El<Msg>> = self
            .apps
            .iter()
            .enumerate()
            .map(|(i, a)| {
                let bullet = canvas(12, 12, |p, r| p.circle(r.x as i32 + 6, r.y as i32 + 6, 5, BLACK, None));
                let item = plate(row(vec![bullet, text(a.name).size(30).max_lines(1)]).gap(20), 14, 2);
                row(vec![item, spacer()]).pad(38, 6).on_tap(Msg::Open(i))
            })
            .collect();
        let content = col(vec![
            status,
            empty().h(30),
            row(vec![spacer(), sysui::clock_box(&now, 104), spacer()]),
            empty().h(18),
            row(vec![spacer(), plate(month, 16, 10), spacer()]),
            spacer(),
            col(list),
            empty().h(30),
        ]);
        sysui::with_wallpaper(wall, content)
    }

    // ---- cards layout ----

    fn cards_view(&self, env: &Env) -> El<Msg> {
        let now = env.now;
        let header = col(vec![text(now.hm()).size(92).bold(), text(now.long_date()).size(22).color(60)]).pad(28, 4);
        let cal = col(vec![
            row(vec![text(now.month_name()).size(20).bold().fill_w(), text(now.year.to_string()).size(18).color(90)]),
            sysui::month_calendar(&now, 32, 15),
        ])
        .gap(8)
        .padding(14)
        .border(2.0, BLACK)
        .radius(16.0)
        .fill_w();
        let line = |ic: &str, s: String| row(vec![icon(ic, 22), text(s).size(17).max_lines(1)]).gap(10);
        let mut dev = vec![text("Device").size(20).bold()];
        if let Some((pct, charging)) = env.battery {
            dev.push(row(vec![sysui::battery_icon(pct, charging), text(format!("{pct}%{}", if charging { " · charging" } else { "" })).size(17)]).gap(10));
        }
        if let Some(on) = env.wifi {
            dev.push(line(if on { "wifi" } else { "wifi-off" }, if on { "Wi-Fi on".into() } else { "Wi-Fi off".into() }));
        }
        if let Some((free, _)) = inkpad::hal::system::storage() {
            dev.push(line("hard-drive", format!("{:.1} GB free", free as f64 / 1e9)));
        }
        let lock = sysui::lock_enabled(&self.prefs);
        dev.push(line(if lock { "lock" } else { "lock-open" }, if lock { "PIN on".into() } else { "No PIN".into() }));
        let device = col(dev).gap(12).padding(14).border(2.0, BLACK).radius(16.0).fill_w().on_tap(Msg::StatusTap);
        let widgets = row(vec![cal, device]).gap(14).align(Align::Stretch).pad(24, 0);

        let tiles: Vec<El<Msg>> = self
            .apps
            .iter()
            .enumerate()
            .map(|(i, a)| {
                let tile = col(vec![spacer(), icon(a.icon, 38), spacer()]).align(Align::Center).w(76).h(76).border(2.5, BLACK).radius(20.0);
                col(vec![row(vec![spacer(), tile, spacer()]), text(a.name).size(15).bold().center_text().max_lines(1)]).gap(6).fill_w().on_tap(Msg::Open(i))
            })
            .collect();
        let mut rows = Vec::new();
        let mut it = tiles.into_iter().peekable();
        while it.peek().is_some() {
            let mut r: Vec<El<Msg>> = it.by_ref().take(4).collect();
            while r.len() < 4 {
                r.push(empty().fill_w());
            }
            rows.push(row(r).gap(8).pad(16, 10));
        }
        col(vec![
            header,
            widgets,
            empty().h(14),
            text("Apps").size(18).bold().color(60).pad(28, 0),
            paged(rows, self.page, Msg::AppPage),
        ])
        .gap(10)
    }

    fn open(&mut self, i: usize, ctx: &mut Ctx<Msg>) {
        let Some(a) = self.apps.get(i).copied() else { return };
        match a.action {
            Action::Launch(bin) => ctx.launch(bin, &[], move |r| Msg::Launched(a.name, r)),
            Action::Settings => {
                self.screen = Screen::Settings;
                ctx.full_refresh();
            }
            Action::KOReader => {
                if cfg!(target_arch = "arm") {
                    ctx.exit(apps::EXIT_KOREADER);
                } else {
                    ctx.toast("KOReader only runs on the Kindle");
                }
            }
            Action::Kindle => ctx.quit(),
        }
    }
}

impl App for Home {
    type Msg = Msg;

    fn setup(&mut self, _fonts: &mut Fonts, ctx: &mut Ctx<Msg>) {
        ctx.lock();
    }

    fn status_bar(&self) -> bool {
        !self.preview_sleep && (self.screen == Screen::Settings || !self.minimal())
    }

    fn view(&self, env: &Env) -> El<Msg> {
        if self.preview_sleep {
            return sysui::sleep_view(&self.prefs, &env.now, env.width, env.height).on_tap(Msg::PreviewSleep(false));
        }
        match self.screen {
            Screen::Settings => self.settings_view(env),
            Screen::Home if self.minimal() => self.minimal_view(env),
            Screen::Home => self.cards_view(env),
        }
    }

    fn update(&mut self, msg: Msg, ctx: &mut Ctx<Msg>) {
        match msg {
            Msg::Open(i) => self.open(i, ctx),
            Msg::Launched(name, r) => {
                self.reload();
                match r {
                    Ok(0) => {}
                    Ok(code) => ctx.toast(format!("{name} closed (code {code})")),
                    Err(e) => ctx.toast(format!("Couldn't open {name}: {e}")),
                }
            }
            Msg::StatusTap => ctx.system_menu(),
            Msg::AppPage(p) => self.page = p,
            other => self.settings_update(other, ctx),
        }
    }

    fn on_wake(&mut self, _ctx: &mut Ctx<Msg>) {
        self.preview_sleep = false;
        self.reload();
    }
}

fn main() {
    if let Err(e) = inkpad::run(Home::new()) {
        eprintln!("home: {e}");
        std::process::exit(1);
    }
}
