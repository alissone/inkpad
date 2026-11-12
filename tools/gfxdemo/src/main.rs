//! M2 demo: a hand-drawn "Tasks" screen showing the graphics and text layer.
//! Tap a row to tick it (partial refresh), tap Sync for a flashing full
//! refresh, tap the X to exit. Runs on the Kindle and in the simulator.

use inkpad::gfx::{Fonts, Painter, TextStyle, BLACK, WHITE};
use inkpad::hal::{Event, Frame, Rect, TouchPhase, Waveform};
use std::time::{Duration, Instant};

const MARGIN: u32 = 28;
const STATUS_H: u32 = 40;
const HEADER_H: u32 = 120;
const ROW_MIN_H: u32 = 84;
const FOOTER_H: u32 = 96;

struct Task {
    title: &'static str,
    meta: &'static str,
    done: bool,
}

struct App {
    fonts: Fonts,
    frame: Frame,
    tasks: Vec<Task>,
    w: u32,
    h: u32,
}

fn battery() -> String {
    std::fs::read_to_string("/sys/class/power_supply/bd7181x_bat/capacity")
        .map(|s| format!("{}%", s.trim()))
        .unwrap_or_else(|_| "simulator".into())
}

impl App {
    fn close_rect(&self) -> Rect {
        Rect::new(self.w - MARGIN - 44, STATUS_H + 22, 44, 44)
    }
    fn title_width(&self) -> u32 {
        self.w - (MARGIN + 50) - MARGIN
    }
    /// Rows size to their content: 1–2 title lines plus a meta line.
    fn row_rect(&self, i: usize) -> Rect {
        let mut y = STATUS_H + HEADER_H;
        for (k, t) in self.tasks.iter().enumerate() {
            let title = self.fonts.layout(t.title, TextStyle::new(23), Some(self.title_width()), Some(2));
            let h = (title.height + 50).max(ROW_MIN_H);
            if k == i {
                return Rect::new(0, y, self.w, h);
            }
            y += h;
        }
        Rect::new(0, y, self.w, 0)
    }
    fn sync_rect(&self) -> Rect {
        Rect::new(self.w - MARGIN - 150, self.h - FOOTER_H + 20, 150, 56)
    }
    fn add_rect(&self) -> Rect {
        Rect::new(MARGIN, self.h - FOOTER_H + 20, 250, 56)
    }

    fn draw_all(&mut self) {
        let (w, h) = (self.w, self.h);
        let (c, a, s) = (self.close_rect(), self.add_rect(), self.sync_rect());
        let mut p = Painter::new(&mut self.frame);
        p.fill_rect(Rect::new(0, 0, w, h), WHITE);
        let f = &self.fonts;

        // Status bar.
        let small = TextStyle::new(17).color(60);
        let l = f.layout("inkpad", small, None, None);
        f.draw(&mut p, &l, MARGIN as i32, 10);
        let l = f.layout(&battery(), small, None, None);
        f.draw(&mut p, &l, (w - MARGIN - l.width) as i32, 10);
        p.fill_rect(Rect::new(0, STATUS_H - 1, w, 1), 170);

        // Header.
        let open = self.tasks.iter().filter(|t| !t.done).count();
        let done = self.tasks.len() - open;
        let l = f.layout("Tasks", TextStyle::new(44).bold(), None, None);
        f.draw(&mut p, &l, MARGIN as i32, STATUS_H as i32 + 16);
        let sub = format!("{open} open · {done} done today");
        let l = f.layout(&sub, TextStyle::new(19).color(80), None, None);
        f.draw(&mut p, &l, MARGIN as i32, STATUS_H as i32 + 74);
        p.round_rect(c, 22.0, BLACK, Some(2.5));
        let (cx, cy) = (c.x as f32 + 22.0, c.y as f32 + 22.0);
        p.line(cx - 8.0, cy - 8.0, cx + 8.0, cy + 8.0, 2.5, BLACK);
        p.line(cx + 8.0, cy - 8.0, cx - 8.0, cy + 8.0, 2.5, BLACK);
        p.fill_rect(Rect::new(MARGIN, STATUS_H + HEADER_H - 2, w - 2 * MARGIN, 2), BLACK);
        drop(p);

        for i in 0..self.tasks.len() {
            self.draw_row(i);
        }

        // Footer buttons.
        let f = &self.fonts;
        let mut p = Painter::new(&mut self.frame);
        p.round_rect(a, 28.0, BLACK, None);
        let l = f.layout("+  Add a task", TextStyle::new(22).bold().color(WHITE), None, None);
        f.draw_aligned(&mut p, &l, a.x as i32, a.y as i32 + (a.h - l.height) as i32 / 2, a.w, 0.5);
        p.round_rect(s, 28.0, BLACK, Some(2.5));
        let l = f.layout("Sync", TextStyle::new(22).bold(), None, None);
        f.draw_aligned(&mut p, &l, s.x as i32, s.y as i32 + (s.h - l.height) as i32 / 2, s.w, 0.5);
    }

    fn draw_row(&mut self, i: usize) {
        let r = self.row_rect(i);
        let tw = self.title_width();
        let t = &self.tasks[i];
        let f = &self.fonts;
        let mut p = Painter::new(&mut self.frame);
        p.fill_rect(r, WHITE);
        // Checkbox.
        let (cx, cy) = (MARGIN as i32 + 16, r.y as i32 + 34);
        if t.done {
            p.circle(cx, cy, 16, BLACK, None);
            p.line(cx as f32 - 7.0, cy as f32 + 0.5, cx as f32 - 2.0, cy as f32 + 6.0, 3.0, WHITE);
            p.line(cx as f32 - 2.0, cy as f32 + 6.0, cx as f32 + 8.0, cy as f32 - 6.0, 3.0, WHITE);
        } else {
            p.circle(cx, cy, 16, BLACK, Some(2.5));
        }
        // Title (up to 2 lines) and meta.
        let tx = MARGIN as i32 + 50;
        let color = if t.done { 120 } else { BLACK };
        let title = f.layout(t.title, TextStyle::new(23).color(color), Some(tw), Some(2));
        f.draw(&mut p, &title, tx, r.y as i32 + 18);
        if t.done {
            for (k, line) in title.lines.iter().enumerate() {
                let y = r.y as f32 + 18.0 + k as f32 * title.line_height + title.ascent * 0.68;
                p.line(tx as f32, y, tx as f32 + line.width, y, 1.5, 120);
            }
        }
        let meta = f.layout(t.meta, TextStyle::new(16).color(100), Some(tw), Some(1));
        f.draw(&mut p, &meta, tx, r.y as i32 + 18 + title.height as i32 + 2);
        p.fill_rect(Rect::new(tx as u32, r.y + r.h - 1, tw, 1), 200);
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("gfxdemo error: {e}");
        std::process::exit(1);
    }
}

fn run() -> std::io::Result<()> {
    let (mut display, mut input) = inkpad::platform()?;
    let (w, h) = display.size();
    let mut app = App {
        fonts: Fonts::new(),
        frame: Frame::new(w, h),
        tasks: vec![
            Task { title: "Comprar pão, café e frutas para a semana", meta: "Today · Mercado", done: false },
            Task { title: "Reply to the landlord about the lease renewal before Friday", meta: "Fri 15 Nov", done: false },
            Task { title: "Dentist", meta: "Tue 19 Nov · 14:30 – 15:15", done: false },
            Task { title: "Read chapter 4 of “Predictably Irrational”", meta: "Books", done: true },
            Task { title: "Pay electricity bill", meta: "Done today", done: true },
        ],
        w,
        h,
    };
    let t0 = Instant::now();
    app.draw_all();
    eprintln!("first frame drawn in {} ms", t0.elapsed().as_millis());
    display.present(&app.frame, app.frame.full(), Waveform::Gc16, true)?;

    let mut last = Instant::now();
    loop {
        if cfg!(target_arch = "arm") && last.elapsed() > Duration::from_secs(120) {
            eprintln!("idle timeout");
            return Ok(());
        }
        for ev in input.poll(1000)? {
            let t = match ev {
                Event::Quit => return Ok(()),
                Event::Touch(t) => t,
            };
            last = Instant::now();
            if t.phase != TouchPhase::Down {
                continue;
            }
            if app.close_rect().contains(t.x, t.y) {
                return Ok(());
            }
            if app.sync_rect().contains(t.x, t.y) {
                app.draw_all();
                display.present(&app.frame, app.frame.full(), Waveform::Gc16, true)?;
                continue;
            }
            if let Some(i) = (0..app.tasks.len()).find(|&i| app.row_rect(i).contains(t.x, t.y)) {
                app.tasks[i].done = !app.tasks[i].done;
                // Redraw the row and the header counts, without flashing.
                app.draw_all();
                let r = app.row_rect(i);
                display.present(&app.frame, r, Waveform::Gl16, false)?;
                let header = Rect::new(0, STATUS_H, w, HEADER_H);
                display.present(&app.frame, header, Waveform::Gl16, false)?;
            }
        }
    }
}
