//! Settings: tabs on the left, options on the right (Boox-style). Everything
//! is stored in the shared system preferences, so other apps and the
//! runtime (lock and sleep screens) see changes immediately.

use crate::{Home, Msg, PinSetup, Screen, Tab, DEV_APPS};
use inkpad::prelude::*;
use inkpad::ui::{sysui, sysui::keys};

const TABS: [(Tab, &str, &str); 6] = [
    (Tab::Display, "monitor", "Display"),
    (Tab::Wallpaper, "wallpaper", "Wallpaper"),
    (Tab::Lock, "lock", "Lock screen"),
    (Tab::Sleep, "moon", "Sleep screen"),
    (Tab::Network, "wifi", "Network"),
    (Tab::About, "info", "About"),
];

const HOME_STYLES: [(&str, &str); 2] = [("minimal", "Minimal"), ("cards", "Cards")];
const SLEEP_MODES: [(&str, &str); 4] = [("minimal", "Minimal"), ("picture", "Picture"), ("calendar", "Month"), ("agenda", "Agenda")];
const FADES: [(i64, &str); 4] = [(0, "Off"), (35, "Light"), (55, "Medium"), (75, "Strong")];

fn index_of<T: PartialEq + Copy>(list: &[(T, &str)], v: T) -> usize {
    list.iter().position(|(k, _)| *k == v).unwrap_or(0)
}

fn heading(s: &str) -> El<Msg> {
    text(s).size(26).bold()
}

fn note(s: &str) -> El<Msg> {
    text(s).size(16).color(80)
}

impl Home {
    pub fn settings_view(&self, env: &Env) -> El<Msg> {
        let top = row(vec![icon_button("chevron-left", Msg::Back), text("Settings").size(32).bold()]).gap(4).pad(10, 6);
        let tabs: Vec<El<Msg>> = TABS
            .iter()
            .map(|&(t, ic, label)| {
                let sel = t == self.tab;
                let fg = if sel { WHITE } else { BLACK };
                let item = row(vec![icon(ic, 24).color(fg), text(label).size(18).bold().color(fg).max_lines(1)]).gap(10).pad(12, 14).on_tap(Msg::Tab(t));
                if sel { item.bg(BLACK).radius(12.0) } else { item }
            })
            .collect();
        let rail = col(tabs).gap(6).pad(8, 12).w(186);
        let pane = match self.tab {
            Tab::Display => self.display_tab(),
            Tab::Wallpaper => self.wallpaper_tab(),
            Tab::Lock => self.lock_tab(),
            Tab::Sleep => self.sleep_tab(),
            Tab::Network => self.network_tab(env),
            Tab::About => self.about_tab(env),
        };
        col(vec![
            top,
            rule(2, BLACK),
            row(vec![rail, rule(1, 170), pane.gap(18).pad(20, 16).fill_w()]).align(Align::Stretch).fill_h(),
        ])
    }

    fn display_tab(&self) -> El<Msg> {
        let style = index_of(&HOME_STYLES, self.prefs.get_str(keys::HOME_STYLE).unwrap_or("minimal"));
        let dev = self.prefs.get_bool(DEV_APPS, false);
        col(vec![
            heading("Display"),
            text("Home layout").size(21).bold(),
            segmented(&HOME_STYLES.map(|s| s.1), style, Msg::HomeStyle),
            note(if style == 0 { "Framed clock, the month at a glance and a plain list of apps." } else { "Status bar, big clock, calendar and device cards, app grid." }),
            divider(),
            setting_row("Developer apps", Some("Show Touch test and Graphics demo"), toggle(dev)).on_tap(Msg::DevApps),
            divider(),
            button_icon("refresh-cw", "Refresh screen", false, Msg::RefreshScreen),
            note("Clears e-ink ghosting with one full flash."),
        ])
    }

    fn wallpaper_tab(&self) -> El<Msg> {
        let current = self.prefs.get_str(keys::WALLPAPER).unwrap_or("");
        let mut items = vec![heading("Wallpaper")];
        let option = |i: usize, name: &str, label: &str| {
            let thumb = match sysui::load_wallpaper(name, 54, 72, 0) {
                Some(img) => image(img).w(54).h(72).border(1.5, BLACK),
                None => empty().w(54).h(72).border(1.5, BLACK),
            };
            row(vec![thumb, text(label).size(19).bold().max_lines(1).fill_w(), checkbox(name == current)]).gap(14).pad(0, 2).on_tap(Msg::Wallpaper(i))
        };
        items.push(option(0, "", "None"));
        for (i, w) in self.walls.iter().enumerate() {
            let label = w.rsplit_once('.').map(|p| p.0).unwrap_or(w);
            items.push(option(i + 1, w, label));
        }
        let fade = index_of(&FADES, self.prefs.get_i64(keys::WALLPAPER_FADE, 55));
        items.push(text("Fade behind text").size(19).bold());
        items.push(segmented(&FADES.map(|f| f.1), fade, Msg::Fade));
        items.push(note("Add PNGs to inkpad/assets/wallpapers."));
        col(items)
    }

    fn lock_tab(&self) -> El<Msg> {
        if let Some(s) = &self.pin_setup {
            let title = match (&s.first, s.mismatch) {
                (_, true) => "PINs didn't match. Choose a PIN",
                (None, _) => "Choose a 4-digit PIN",
                (Some(_), _) => "Enter it again",
            };
            return col(vec![
                text(title).size(21).bold().center_text(),
                row(vec![spacer(), sysui::pin_dots(s.entered.len(), 4), spacer()]),
                sysui::pin_pad(Msg::PinDigit, Msg::PinBack),
                row(vec![spacer(), button_outline("Cancel", Msg::PinCancel), spacer()]),
            ])
            .gap(14);
        }
        let on = sysui::lock_enabled(&self.prefs);
        let mut items = vec![
            heading("Lock screen"),
            setting_row("PIN lock", Some(if on { "On: asked at start and on wake" } else { "Off" }), toggle(on)).on_tap(if on { Msg::RemovePin } else { Msg::SetPin }),
        ];
        if on {
            items.push(row(vec![button_outline("Change PIN", Msg::SetPin), button_outline("Lock now", Msg::LockNow)]).gap(10));
        }
        items.push(divider());
        items.push(note("Forgot the PIN? Hold the power button for about 40 seconds: the Kindle restarts into its normal interface."));
        items.push(note("The PIN keeps casual hands out. It does not encrypt anything: files are still readable over USB."));
        col(items)
    }

    fn sleep_tab(&self) -> El<Msg> {
        let mode = index_of(&SLEEP_MODES, self.prefs.get_str(keys::SLEEP_MODE).unwrap_or("minimal"));
        let has_wall = !self.prefs.get_str(keys::WALLPAPER).unwrap_or("").is_empty();
        let hint = match mode {
            1 if !has_wall => "Picture needs a wallpaper; shows Minimal until you pick one.",
            1 => "Your wallpaper, full strength, with a small date label.",
            2 => "This month's calendar, framed.",
            3 => "The date, plus what your apps publish: your open tasks, for example.",
            _ => "Moon, date and the month's progress dots.",
        };
        col(vec![
            heading("Sleep screen"),
            text("Shown while the Kindle sleeps").size(19).bold(),
            segmented(&SLEEP_MODES.map(|s| s.1), mode, Msg::SleepMode),
            note(hint),
            button_icon("moon", "Preview", false, Msg::PreviewSleep(true)),
            note("Tap the preview to close it. In the simulator, F2 is the power button."),
        ])
    }

    fn network_tab(&self, env: &Env) -> El<Msg> {
        let on = env.wifi.unwrap_or(false);
        col(vec![
            heading("Network"),
            setting_row("Wi-Fi", Some(if on { "On" } else { "Off: saves battery" }), toggle(on)).on_tap(Msg::Wifi),
            note("Networks are chosen in the Kindle's own settings. Apps that sync turn Wi-Fi on only while they need it."),
        ])
    }

    fn about_tab(&self, env: &Env) -> El<Msg> {
        let kv = |k: &str, v: String| row(vec![text(k).size(17).color(80).w(100), text(v).size(17).bold().fill_w()]).gap(10);
        let mut items = vec![heading("About"), kv("inkpad", env!("CARGO_PKG_VERSION").to_string())];
        items.push(kv("Device", if cfg!(target_arch = "arm") { "Kindle".into() } else { "Simulator".into() }));
        items.push(kv("Screen", format!("{} × {}", env.width, env.height + STATUS_H)));
        if let Some((pct, ch)) = env.battery {
            items.push(kv("Battery", format!("{pct}%{}", if ch { ", charging" } else { "" })));
        }
        if let Some((free, total)) = inkpad::hal::system::storage() {
            items.push(kv("Storage", format!("{:.1} of {:.1} GB free", free as f64 / 1e9, total as f64 / 1e9)));
        }
        items.push(kv("Data", inkpad::ui::store::data_dir().display().to_string()));
        items.push(divider());
        items.push(note("Open source, MIT or Apache-2.0. Icons: Lucide (ISC)."));
        col(items)
    }

    pub fn settings_update(&mut self, msg: Msg, ctx: &mut Ctx<Msg>) {
        match msg {
            Msg::Back => {
                self.screen = Screen::Home;
                self.pin_setup = None;
                self.page = 0;
                ctx.full_refresh();
            }
            Msg::Tab(t) => {
                self.tab = t;
                self.pin_setup = None;
            }
            Msg::HomeStyle(i) => self.prefs.set(keys::HOME_STYLE, HOME_STYLES[i].0),
            Msg::DevApps => {
                let v = !self.prefs.get_bool(DEV_APPS, false);
                self.prefs.set(DEV_APPS, v);
                self.reload();
            }
            Msg::Wallpaper(i) => {
                let name = if i == 0 { String::new() } else { self.walls.get(i - 1).cloned().unwrap_or_default() };
                self.prefs.set(keys::WALLPAPER, name);
            }
            Msg::Fade(i) => self.prefs.set(keys::WALLPAPER_FADE, FADES[i].0),
            Msg::SleepMode(i) => self.prefs.set(keys::SLEEP_MODE, SLEEP_MODES[i].0),
            Msg::PreviewSleep(on) => {
                self.preview_sleep = on;
                ctx.full_refresh();
            }
            Msg::SetPin => self.pin_setup = Some(PinSetup { first: None, entered: String::new(), mismatch: false }),
            Msg::PinCancel => self.pin_setup = None,
            Msg::RemovePin => {
                sysui::set_pin(&mut self.prefs, None);
                ctx.toast("PIN removed");
            }
            Msg::LockNow => ctx.lock(),
            Msg::PinBack => {
                if let Some(s) = &mut self.pin_setup {
                    s.entered.pop();
                }
            }
            Msg::PinDigit(d) => {
                let Some(s) = &mut self.pin_setup else { return };
                s.entered.push(char::from(b'0' + d));
                if s.entered.len() < 4 {
                    return;
                }
                let pin = std::mem::take(&mut s.entered);
                match s.first.take() {
                    None => {
                        s.first = Some(pin);
                        s.mismatch = false;
                    }
                    Some(first) if first == pin => {
                        sysui::set_pin(&mut self.prefs, Some(&pin));
                        self.pin_setup = None;
                        ctx.toast("PIN set");
                    }
                    Some(_) => s.mismatch = true,
                }
            }
            Msg::Wifi => {
                let on = inkpad::hal::system::wifi_on().unwrap_or(false);
                if let Err(e) = inkpad::hal::system::set_wifi(!on) {
                    ctx.toast(format!("Wi-Fi: {e}"));
                }
                ctx.refresh_status();
            }
            Msg::RefreshScreen => ctx.full_refresh(),
            Msg::Open(_) | Msg::Launched(..) | Msg::StatusTap | Msg::AppPage(_) | Msg::OpenApp(_) => {}
        }
    }
}
