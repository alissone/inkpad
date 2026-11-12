//! M1 test: draw where you touch.
//!
//! Screen layout:
//! - top-right box with an X: tap to exit cleanly
//! - top-left hatched box: tap to crash on purpose (tests the launcher's restore)
//! - bottom strip: 16 gray levels, black on the left to white on the right
//!   (if it's reversed, the panel needs INKPAD_INVERT=1)
//!
//! Two fingers at once also exits. No touch for 60 s exits too.

#[cfg(target_os = "linux")]
fn main() {
    if let Err(e) = app::run() {
        eprintln!("touchtest error: {e}");
        std::process::exit(1);
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("touchtest runs on the Kindle only (build with `cargo kindle -p touchtest`).");
}

#[cfg(target_os = "linux")]
mod app {
    use inkpad_hal::kindle::{KindleDisplay, KindleTouch};
    use inkpad_hal::{Display, Event, Frame, Input, Rect, TouchPhase, Waveform};
    use std::time::{Duration, Instant};

    const BOX: u32 = 110;
    const DOT: i32 = 6;

    fn line(f: &mut Frame, x0: i32, y0: i32, x1: i32, y1: i32, thick: i32, gray: u8) {
        let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
        let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
        let (mut x, mut y, mut err) = (x0, y0, dx + dy);
        loop {
            for oy in -thick / 2..=thick / 2 {
                for ox in -thick / 2..=thick / 2 {
                    f.set(x + ox, y + oy, gray);
                }
            }
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    fn frame_rect(f: &mut Frame, r: Rect, t: u32, gray: u8) {
        f.fill(Rect::new(r.x, r.y, r.w, t), gray);
        f.fill(Rect::new(r.x, r.y + r.h - t, r.w, t), gray);
        f.fill(Rect::new(r.x, r.y, t, r.h), gray);
        f.fill(Rect::new(r.x + r.w - t, r.y, t, r.h), gray);
    }

    pub fn run() -> std::io::Result<()> {
        let mut disp = KindleDisplay::open()?;
        let (w, h) = disp.size();
        let mut touch = KindleTouch::open("/dev/input/event0", (w, h))?;
        let mut f = Frame::new(w, h);

        let exit_box = Rect::new(w - BOX - 10, 10, BOX, BOX);
        let crash_box = Rect::new(10, 10, BOX, BOX);

        // Static screen.
        let full = f.full();
        frame_rect(&mut f, full, 4, 0);
        frame_rect(&mut f, exit_box, 4, 0);
        let (ex, ey, eb) = (exit_box.x as i32, exit_box.y as i32, BOX as i32);
        line(&mut f, ex + 25, ey + 25, ex + eb - 25, ey + eb - 25, 6, 0);
        line(&mut f, ex + eb - 25, ey + 25, ex + 25, ey + eb - 25, 6, 0);
        frame_rect(&mut f, crash_box, 4, 0);
        let (cx, cy) = (crash_box.x as i32, crash_box.y as i32);
        for k in (0..2 * eb).step_by(18) {
            let (a, b) = ((k).min(eb - 1), (k - eb + 1).max(0));
            line(&mut f, cx + b, cy + a, cx + a, cy + b, 2, 128);
        }
        let strip_y = h - 90;
        let cell = (w - 40) / 16;
        for i in 0..16u32 {
            f.fill(Rect::new(20 + i * cell, strip_y, cell, 60), (i * 17) as u8);
        }
        frame_rect(&mut f, Rect::new(20, strip_y, cell * 16, 60), 2, 0);
        disp.present(&f, f.full(), Waveform::Gc16, true)?;
        eprintln!("screen {w}x{h} drawn; waiting for touches");

        let mut last = Instant::now();
        let mut down = 0usize;
        let mut prev: [Option<(i32, i32)>; 10] = [None; 10];
        loop {
            if last.elapsed() > Duration::from_secs(60) {
                eprintln!("idle timeout, exiting");
                break;
            }
            let evs = touch.poll(1000)?;
            for ev in evs {
                let t = match ev {
                    Event::Touch(t) => t,
                    Event::Quit => return Ok(()),
                    _ => continue,
                };
                last = Instant::now();
                eprintln!("touch {:?} id={} at {},{}", t.phase, t.id, t.x, t.y);
                let slot = (t.id.max(0) as usize).min(9);
                match t.phase {
                    TouchPhase::Down => {
                        down += 1;
                        if down >= 2 {
                            eprintln!("two fingers: exiting");
                            return Ok(());
                        }
                        if exit_box.contains(t.x, t.y) {
                            eprintln!("exit box tapped");
                            return Ok(());
                        }
                        if crash_box.contains(t.x, t.y) {
                            eprintln!("crash box tapped: aborting on purpose");
                            std::process::abort();
                        }
                        prev[slot] = Some((t.x, t.y));
                    }
                    TouchPhase::Up => {
                        down = down.saturating_sub(1);
                        prev[slot] = None;
                        continue;
                    }
                    TouchPhase::Move => {}
                }
                let (px, py) = prev[slot].unwrap_or((t.x, t.y));
                line(&mut f, px, py, t.x, t.y, DOT, 0);
                prev[slot] = Some((t.x, t.y));
                let x0 = px.min(t.x) - DOT;
                let y0 = py.min(t.y) - DOT;
                let x1 = px.max(t.x) + DOT;
                let y1 = py.max(t.y) + DOT;
                let r = Rect::new(x0.max(0) as u32, y0.max(0) as u32, (x1 - x0.max(0)) as u32, (y1 - y0.max(0)) as u32);
                disp.present(&f, r, Waveform::Du, false)?;
            }
        }
        Ok(())
    }
}
