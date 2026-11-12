//! Ready-made widgets built from the element primitives.

use crate::el::*;
use inkpad_gfx::{BLACK, WHITE};

/// Standard touch target height (≈ 9 mm on a 167 dpi panel).
pub const TOUCH: u32 = 56;

/// A filled, rounded button.
pub fn button<M>(label: &str, msg: M) -> El<M> {
    text(label).size(22).bold().color(WHITE).center_text().pad(28, 14).bg(BLACK).radius(28.0).h(TOUCH).on_tap(msg)
}

/// An outlined, rounded button.
pub fn button_outline<M>(label: &str, msg: M) -> El<M> {
    text(label).size(22).bold().center_text().pad(28, 14).border(2.5, BLACK).radius(28.0).h(TOUCH).on_tap(msg)
}

/// A button with an icon and a label.
pub fn button_icon<M>(icon_name: &str, label: &str, filled: bool, msg: M) -> El<M> {
    let fg = if filled { WHITE } else { BLACK };
    let el = row(vec![icon(icon_name, 26).color(fg), text(label).size(22).bold().color(fg)]).gap(10).pad(24, 0).h(TOUCH).radius(28.0).on_tap(msg);
    if filled { el.bg(BLACK) } else { el.border(2.5, BLACK) }
}

/// A round icon-only button.
pub fn icon_button<M>(icon_name: &str, msg: M) -> El<M> {
    icon(icon_name, 28).w(TOUCH).h(TOUCH).on_tap(msg)
}

/// A round checkbox (filled with a check mark when on).
pub fn checkbox<M>(checked: bool) -> El<M> {
    canvas(34, 34, move |p, r| {
        let (cx, cy) = (r.x as i32 + r.w as i32 / 2, r.y as i32 + r.h as i32 / 2);
        if checked {
            p.circle(cx, cy, 16, BLACK, None);
            let (x, y) = (cx as f32, cy as f32);
            p.line(x - 7.0, y + 0.5, x - 2.0, y + 6.0, 3.0, WHITE);
            p.line(x - 2.0, y + 6.0, x + 8.0, y - 6.0, 3.0, WHITE);
        } else {
            p.circle(cx, cy, 16, BLACK, Some(2.5));
        }
    })
}

/// A two-state switch.
pub fn toggle<M>(on: bool) -> El<M> {
    canvas(64, 36, move |p, r| {
        let track = inkpad_gfx::Rect::new(r.x, r.y + (r.h - 34) / 2, 64, 34);
        if on {
            p.round_rect(track, 17.0, BLACK, None);
            p.circle(track.x as i32 + 47, track.y as i32 + 17, 12, WHITE, None);
        } else {
            p.round_rect(track, 17.0, BLACK, Some(2.5));
            p.circle(track.x as i32 + 17, track.y as i32 + 17, 11, BLACK, None);
        }
    })
}

/// Screen title with optional subtitle and trailing actions.
pub fn title_bar<M>(title: &str, subtitle: Option<&str>, actions: Vec<El<M>>) -> El<M> {
    let mut left = vec![text(title).size(40).bold().max_lines(1)];
    if let Some(s) = subtitle {
        left.push(text(s).size(19).color(80).max_lines(1));
    }
    let mut kids = vec![col(left).gap(2).fill_w()];
    kids.extend(actions);
    col(vec![row(kids).gap(4).align(Align::Center), rule(2, BLACK)]).gap(14).pad_edges(Edges { top: 18, right: 20, bottom: 0, left: 28 })
}

/// A small rounded label.
pub fn chip<M>(label: &str) -> El<M> {
    text(label).size(16).bold().pad(12, 4).border(1.5, BLACK).radius(14.0)
}

/// A horizontal progress bar, `frac` in 0..=1.
pub fn progress<M>(frac: f32, height: u32) -> El<M> {
    let frac = frac.clamp(0.0, 1.0);
    canvas(10, height, move |p, r| {
        let rad = r.h as f32 / 2.0;
        p.round_rect(r, rad, BLACK, Some(2.0));
        let w = ((r.w as f32 - 6.0) * frac) as u32;
        if w > 0 {
            p.round_rect(inkpad_gfx::Rect::new(r.x + 3, r.y + 3, w.max(r.h - 6), r.h - 6), rad - 3.0, BLACK, None);
        }
    })
    .fill_w()
}

/// A bordered card container.
pub fn card<M>(child: El<M>) -> El<M> {
    col(vec![child]).padding(20).border(2.0, BLACK).radius(16.0)
}

/// A modal dialog over `base`: dims nothing (e-ink!), but draws a heavy
/// framed card and blocks taps to what is underneath.
pub fn dialog<M>(base: El<M>, title: &str, body: &str, buttons: Vec<El<M>>) -> El<M> {
    let card = col(vec![
        text(title).size(28).bold(),
        text(body).size(21).color(40),
        row(buttons).gap(12).justify(Justify::End),
    ])
    .gap(18)
    .padding(28)
    .bg(WHITE)
    .border(3.0, BLACK)
    .radius(18.0);
    let overlay = col(vec![spacer(), card, spacer()]).pad(32, 0).block_taps();
    stack(vec![base, overlay])
}
