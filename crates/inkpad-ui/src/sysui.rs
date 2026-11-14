//! System UI shared by every app: wallpaper, sleep screen, lock screen with
//! a PIN pad, plus clock and calendar widgets apps can reuse.
//!
//! Its settings live in the shared [`Prefs::system`] namespace (see [`keys`]),
//! written by the home app's Settings screen and read here.

use crate::assets;
use crate::clock::{self, LocalTime};
use crate::el::*;
use inkpad_gfx::{GrayImage, Rect, BLACK, WHITE};
use inkpad_store::Prefs;
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Keys in the system preferences.
pub mod keys {
    /// Home screen layout: `"minimal"` or `"cards"`.
    pub const HOME_STYLE: &str = "home.style";
    /// Wallpaper file name in `assets/wallpapers` (empty or missing = none).
    pub const WALLPAPER: &str = "wallpaper";
    /// How much the wallpaper is faded behind text, percent (0–90).
    pub const WALLPAPER_FADE: &str = "wallpaper.fade";
    /// Sleep screen: `"minimal"`, `"picture"`, `"calendar"` or `"agenda"` (glances).
    pub const SLEEP_MODE: &str = "sleep.mode";
    pub const LOCK_HASH: &str = "lock.hash";
    pub const LOCK_SALT: &str = "lock.salt";
    pub const LOCK_LEN: &str = "lock.len";
}

// ---- wallpaper -----------------------------------------------------------

/// Wallpaper files (PNG/SVG) found in every `assets/wallpapers` folder.
pub fn wallpapers() -> Vec<String> {
    let mut names: Vec<String> = assets::asset_dirs()
        .into_iter()
        .filter_map(|d| std::fs::read_dir(d.join("wallpapers")).ok())
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| {
            let l = n.to_ascii_lowercase();
            l.ends_with(".png") || l.ends_with(".svg")
        })
        .collect();
    names.sort();
    names.dedup();
    names
}

thread_local! {
    static WALL: RefCell<HashMap<(String, u32, u32, u32), Option<Rc<GrayImage>>>> = RefCell::new(HashMap::new());
}

/// Load wallpaper `name` cropped to `w`×`h`, faded by `fade_pct`, dithered.
pub fn load_wallpaper(name: &str, w: u32, h: u32, fade_pct: u32) -> Option<Rc<GrayImage>> {
    if name.is_empty() {
        return None;
    }
    let key = (name.to_string(), w, h, fade_pct);
    if let Some(hit) = WALL.with(|c| c.borrow().get(&key).cloned()) {
        return hit;
    }
    let img = assets::image(&format!("wallpapers/{name}"), 0, 0, false).map(|src| {
        let mut img = src.cover(w, h);
        if fade_pct > 0 {
            img = img.fade(fade_pct as f32 / 100.0);
        }
        Rc::new(img.dither16())
    });
    WALL.with(|c| c.borrow_mut().insert(key, img.clone()));
    img
}

/// The configured wallpaper, faded for use behind text.
pub fn wallpaper(prefs: &Prefs, w: u32, h: u32) -> Option<Rc<GrayImage>> {
    load_wallpaper(prefs.get_str(keys::WALLPAPER).unwrap_or(""), w, h, prefs.get_i64(keys::WALLPAPER_FADE, 55).clamp(0, 90) as u32)
}

/// Put `content` over the wallpaper (if any).
pub fn with_wallpaper<M>(wall: Option<Rc<GrayImage>>, content: El<M>) -> El<M> {
    match wall {
        Some(img) => stack(vec![image(img).fill(), content.fill()]),
        None => content,
    }
}

// ---- clock & calendar widgets ---------------------------------------------

/// The framed "16 | 55" clock.
pub fn clock_box<M>(t: &LocalTime, size: u16) -> El<M> {
    let digit = |s: String| text(s).size(size).bold().pad(size as u32 / 5, 0);
    let bar = rule(4, BLACK).h(size as u32 * 9 / 10);
    row(vec![digit(format!("{:02}", t.hour)), bar, digit(format!("{:02}", t.minute))]).border(4.0, BLACK).bg(WHITE).pad(0, 2)
}

/// Month progress: one dot per day; past days filled, today ringed.
pub fn month_dots<M>(t: &LocalTime, cols: u32, dot: u32) -> El<M> {
    let days = clock::days_in_month(t.year, t.month);
    let today = t.day;
    let rows = days.div_ceil(cols);
    let step = dot + dot / 2 + 2;
    canvas(cols * step, rows * step, move |p, r| {
        for d in 1..=days {
            let i = d - 1;
            let cx = r.x as i32 + (i % cols * step + step / 2) as i32;
            let cy = r.y as i32 + (i / cols * step + step / 2) as i32;
            let rad = dot / 2;
            if d < today {
                p.circle(cx, cy, rad, BLACK, None);
            } else if d == today {
                p.circle(cx, cy, rad + 2, BLACK, Some(2.5));
                p.circle(cx, cy, rad.saturating_sub(2), BLACK, None);
            } else {
                p.circle(cx, cy, rad, 150, Some(1.5));
            }
        }
    })
}

/// A month grid (Monday first) with today highlighted.
pub fn month_calendar<M>(t: &LocalTime, cell: u32, size: u16) -> El<M> {
    let first = clock::weekday_of(t.year, t.month, 1);
    let days = clock::days_in_month(t.year, t.month);
    let cellbox = |e: El<M>| e.w(cell).h(cell * 4 / 5);
    let mut rows = vec![row(["M", "T", "W", "T", "F", "S", "S"].iter().map(|d| cellbox(text(*d).size(size - 2).bold().color(90).center_text().pad(0, 4))).collect())];
    let mut week: Vec<El<M>> = (0..first).map(|_| cellbox(empty())).collect();
    for d in 1..=days {
        let label = text(d.to_string()).size(size).center_text().pad(0, 4);
        week.push(if d == t.day { cellbox(label.bold().color(WHITE).bg(BLACK).radius(cell as f32 / 3.0)) } else { cellbox(label) });
        if week.len() == 7 {
            rows.push(row(std::mem::take(&mut week)));
        }
    }
    if !week.is_empty() {
        rows.push(row(week));
    }
    col(rows)
}

/// Battery outline filled to `pct`.
pub fn battery_icon<M>(pct: u8, charging: bool) -> El<M> {
    canvas(30, 16, move |p, r| {
        let body = Rect::new(r.x, r.y, 27, 16);
        p.round_rect(body, 3.0, BLACK, Some(2.0));
        p.fill_rect(Rect::new(r.x + 27, r.y + 5, 3, 6), BLACK);
        let w = (21 * pct.min(100) as u32) / 100;
        p.fill_rect(Rect::new(r.x + 3, r.y + 3, w.max(1), 10), if charging { 110 } else { BLACK });
    })
}

/// Wi-Fi + battery cluster, for apps that draw their own status line.
pub fn status_icons<M>(battery: Option<(u8, bool)>, wifi: Option<bool>, size: u16) -> El<M> {
    let mut kids: Vec<El<M>> = Vec::new();
    if let Some(on) = wifi {
        kids.push(icon(if on { "wifi" } else { "wifi-off" }, size as u32 + 4));
    }
    if let Some((pct, charging)) = battery {
        kids.push(text(format!("{pct}%")).size(size).bold());
        kids.push(battery_icon(pct, charging));
    }
    row(kids).gap(8)
}

// ---- PIN -------------------------------------------------------------------

fn hash(pin: &str, salt: &str) -> String {
    let d = Sha256::digest(format!("inkpad:{salt}:{pin}").as_bytes());
    d.iter().map(|b| format!("{b:02x}")).collect()
}

/// Whether a PIN is set.
pub fn lock_enabled(prefs: &Prefs) -> bool {
    prefs.get_str(keys::LOCK_HASH).is_some_and(|h| !h.is_empty())
}

/// Length of the stored PIN (the pad submits automatically at this length).
pub fn pin_len(prefs: &Prefs) -> usize {
    prefs.get_i64(keys::LOCK_LEN, 4).clamp(4, 8) as usize
}

pub fn check_pin(prefs: &Prefs, pin: &str) -> bool {
    match (prefs.get_str(keys::LOCK_HASH), prefs.get_str(keys::LOCK_SALT)) {
        (Some(h), Some(s)) => hash(pin, s) == h,
        _ => true,
    }
}

/// Store a new PIN (salted SHA-256). `None` removes the lock.
pub fn set_pin(prefs: &mut Prefs, pin: Option<&str>) {
    match pin {
        Some(pin) => {
            let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
            let salt = hash(&format!("{nanos}:{}", std::process::id()), "salt")[..16].to_string();
            prefs.set_many([(keys::LOCK_HASH, hash(pin, &salt).into()), (keys::LOCK_SALT, salt.into()), (keys::LOCK_LEN, inkpad_store::JsonValue::from(pin.len()))]);
        }
        None => {
            prefs.remove(keys::LOCK_HASH);
            prefs.remove(keys::LOCK_SALT);
            prefs.remove(keys::LOCK_LEN);
        }
    }
}

/// Filled/empty dots showing how many digits were typed.
pub fn pin_dots<M>(entered: usize, len: usize) -> El<M> {
    let step = 34;
    canvas(step * len as u32, 24, move |p, r| {
        for i in 0..len {
            let cx = r.x as i32 + (i as u32 * step + step / 2) as i32;
            let cy = r.y as i32 + 12;
            if i < entered {
                p.circle(cx, cy, 10, BLACK, None);
            } else {
                p.circle(cx, cy, 10, BLACK, Some(2.5));
            }
        }
    })
}

/// iOS-style keypad: 1–9, 0 and delete, in round keys.
pub fn pin_pad<M: Clone>(digit: impl Fn(u8) -> M, back: M) -> El<M> {
    let k = 92;
    let key = |label: El<M>, msg: M| col(vec![spacer(), label, spacer()]).align(Align::Center).w(k).h(k).bg(WHITE).border(2.5, BLACK).radius(k as f32 / 2.0).on_tap(msg);
    let num = |d: u8| key(text(d.to_string()).size(36).center_text(), digit(d));
    let mut rows: Vec<El<M>> = (0..3).map(|r| row((1..=3).map(|c| num(r * 3 + c)).collect()).gap(28)).collect();
    let del = col(vec![spacer(), icon("delete", 34), spacer()]).align(Align::Center).w(k).h(k).on_tap(back);
    rows.push(row(vec![empty().w(k).h(k), num(0), del]).gap(28));
    col(rows).gap(18).align(Align::Center)
}

/// The lock screen: framed clock, date, PIN dots and keypad.
pub fn lock_view<M: Clone>(prefs: &Prefs, now: &LocalTime, w: u32, h: u32, entered: usize, error: bool, digit: impl Fn(u8) -> M, back: M) -> El<M> {
    let len = pin_len(prefs);
    let label = if error { "Wrong PIN, try again" } else { "Enter PIN" };
    let content = col(vec![
        empty().h(40),
        row(vec![spacer(), clock_box(now, 64), spacer()]),
        row(vec![spacer(), text(format!("{} · {} {}", now.weekday_name(), now.day, now.month_name())).size(22).bold().pad(12, 4).bg(WHITE).radius(10.0), spacer()]),
        spacer(),
        row(vec![spacer(), col(vec![text(label).size(22).bold().center_text(), row(vec![spacer(), pin_dots(entered, len), spacer()])]).gap(12).pad(18, 10).bg(WHITE).radius(16.0), spacer()]),
        empty().h(10),
        pin_pad(digit, back),
        empty().h(36),
    ])
    .gap(12);
    with_wallpaper(wallpaper(prefs, w, h), content)
}

/// The default sleep screen (see [`keys::SLEEP_MODE`]). No live clock: the
/// panel keeps this image while the device sleeps.
pub fn sleep_view<M>(prefs: &Prefs, now: &LocalTime, w: u32, h: u32) -> El<M> {
    let mode = prefs.get_str(keys::SLEEP_MODE).unwrap_or("minimal");
    let date = format!("{} · {} {}", now.weekday_name(), now.day, now.month_name());
    let pill = |s: String| row(vec![spacer(), row(vec![icon("moon", 22).color(WHITE), text(s).size(20).bold().color(WHITE)]).gap(10).pad(22, 10).bg(BLACK).radius(24.0), spacer()]);
    let wall_name = prefs.get_str(keys::WALLPAPER).unwrap_or("");
    match mode {
        "picture" if !wall_name.is_empty() => {
            let img = load_wallpaper(wall_name, w, h, 0);
            with_wallpaper(img, col(vec![spacer(), pill(format!("Sleeping · {date}")), empty().h(36)]))
        }
        "calendar" => {
            let cal = col(vec![
                text(now.month_name().to_uppercase()).size(40).bold(),
                text(format!("{}", now.year)).size(22).color(90),
                empty().h(12),
                month_calendar(now, 72, 26),
            ])
            .gap(4)
            .padding(24)
            .bg(WHITE)
            .border(3.0, BLACK)
            .radius(18.0);
            let content = col(vec![spacer(), row(vec![spacer(), cal, spacer()]), spacer(), pill("Sleeping".into()), empty().h(36)]);
            with_wallpaper(wallpaper(prefs, w, h), content)
        }
        "agenda" => {
            // Date, then what apps published (Tasks, …).
            let mut kids = vec![
                row(vec![
                    col(vec![text(now.weekday_name().to_uppercase()).size(18).bold().color(60), text(format!("{} {}", now.day, now.month_name())).size(44).bold()]).fill_w(),
                    month_dots(now, 8, 10),
                ])
                .gap(12)
                .padding(18)
                .bg(WHITE)
                .border(3.0, BLACK)
                .radius(18.0),
            ];
            let glances = crate::glance::all();
            if glances.is_empty() {
                kids.push(text("Nothing to show yet. Apps like Tasks appear here after they sync.").size(20).color(70).padding(18).bg(WHITE).radius(12.0));
            }
            let per = if glances.len() <= 1 { 14 } else { 6 };
            for g in &glances {
                kids.push(crate::glance::glance_card(g, per, 20));
            }
            kids.push(spacer());
            kids.push(pill("Sleeping".into()));
            with_wallpaper(wallpaper(prefs, w, h), col(kids).gap(16).pad(28, 30))
        }
        _ => {
            let card = col(vec![
                row(vec![spacer(), icon("moon", 56), spacer()]),
                text("Sleeping").size(52).bold().center_text(),
                text(date).size(24).center_text(),
                row(vec![spacer(), month_dots(now, 10, 14), spacer()]).pad(0, 10),
                text("Press the power button to wake").size(18).color(90).center_text(),
            ])
            .gap(14)
            .pad(30, 30)
            .bg(WHITE)
            .border(3.0, BLACK)
            .radius(22.0);
            with_wallpaper(wallpaper(prefs, w, h), col(vec![spacer(), card, spacer()]).pad(48, 0))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_roundtrip() {
        let dir = std::env::temp_dir().join(format!("inkpad-pin-{}", std::process::id()));
        let mut p = Prefs::open_at(dir.join("system.json"));
        assert!(!lock_enabled(&p));
        set_pin(&mut p, Some("2468"));
        assert!(lock_enabled(&p));
        assert_eq!(pin_len(&p), 4);
        assert!(check_pin(&p, "2468"));
        assert!(!check_pin(&p, "2469"));
        assert!(!p.get_str(keys::LOCK_HASH).unwrap().contains("2468"));
        set_pin(&mut p, None);
        assert!(!lock_enabled(&p));
        let _ = std::fs::remove_dir_all(dir);
    }
}
