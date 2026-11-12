//! Desktop simulator: shows the app in a window that behaves like the e-ink
//! panel, with the mouse acting as a finger.
//!
//! E-ink emulation: content updates (GC16/GL16/Auto) are shown with 16 gray
//! levels, DU updates with pure black/white, and flashing updates briefly
//! blank the region first, like the real panel.
//!
//! Environment:
//! - `INKPAD_SIM_SCALE=2` doubles the window size.
//! - `INKPAD_SNAPSHOT=file.png` runs headless: no window, each update is
//!   written to the PNG, and the app gets `Quit` once scripted taps run out.
//! - `INKPAD_TAPS="x,y;x,y"` scripted taps for snapshot mode.
//!
//! Keys: Esc closes the window.

use inkpad_hal::{Display, Event, Frame, Input, Rect, Touch, TouchPhase, Waveform};
use minifb::{Key, MouseButton, MouseMode, Scale, Window, WindowOptions};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::io;
use std::rc::Rc;
use std::time::{Duration, Instant};

pub const WIDTH: u32 = 600;
pub const HEIGHT: u32 = 800;

struct Shared {
    window: Option<Window>,
    /// What the simulated panel currently shows (gray).
    screen: Vec<u8>,
    argb: Vec<u32>,
    width: u32,
    height: u32,
    scale: f32,
    snapshot: Option<String>,
    taps: VecDeque<(i32, i32)>,
    mouse_down: bool,
    last_pos: (i32, i32),
}

impl Shared {
    fn show(&mut self) {
        for (d, &g) in self.argb.iter_mut().zip(&self.screen) {
            let g = g as u32;
            *d = (g << 16) | (g << 8) | g;
        }
        if let Some(w) = self.window.as_mut() {
            let _ = w.update_with_buffer(&self.argb, self.width as usize, self.height as usize);
        }
    }

    fn write_snapshot(&self) -> io::Result<()> {
        let Some(path) = &self.snapshot else { return Ok(()) };
        let file = std::fs::File::create(path)?;
        let mut enc = png::Encoder::new(io::BufWriter::new(file), self.width, self.height);
        enc.set_color(png::ColorType::Grayscale);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().map_err(io::Error::other)?;
        w.write_image_data(&self.screen).map_err(io::Error::other)?;
        Ok(())
    }
}

/// Create the simulator's display and input (they share one window).
pub fn open() -> io::Result<(SimDisplay, SimInput)> {
    let (width, height) = (WIDTH, HEIGHT);
    let snapshot = std::env::var("INKPAD_SNAPSHOT").ok().filter(|s| !s.is_empty());
    let taps = std::env::var("INKPAD_TAPS")
        .unwrap_or_default()
        .split(';')
        .filter_map(|p| {
            let (x, y) = p.split_once(',')?;
            Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
        })
        .collect();
    let scale_n = std::env::var("INKPAD_SIM_SCALE").ok().and_then(|s| s.parse::<u32>().ok()).unwrap_or(1);
    let window = if snapshot.is_some() {
        None
    } else {
        let scale = match scale_n {
            2 => Scale::X2,
            4 => Scale::X4,
            _ => Scale::X1,
        };
        let opts = WindowOptions { scale, ..WindowOptions::default() };
        let mut w = Window::new("inkpad simulator", width as usize, height as usize, opts).map_err(io::Error::other)?;
        w.set_target_fps(60);
        Some(w)
    };
    let shared = Rc::new(RefCell::new(Shared {
        window,
        screen: vec![255; (width * height) as usize],
        argb: vec![0; (width * height) as usize],
        width,
        height,
        scale: 1.0,
        snapshot,
        taps,
        mouse_down: false,
        last_pos: (0, 0),
    }));
    shared.borrow_mut().show();
    Ok((SimDisplay { s: shared.clone() }, SimInput { s: shared, pending: VecDeque::new() }))
}

pub struct SimDisplay {
    s: Rc<RefCell<Shared>>,
}

impl Display for SimDisplay {
    fn size(&self) -> (u32, u32) {
        let s = self.s.borrow();
        (s.width, s.height)
    }

    fn present(&mut self, frame: &Frame, region: Rect, wf: Waveform, flash: bool) -> io::Result<()> {
        let mut s = self.s.borrow_mut();
        let Some(r) = region.clip(s.width.min(frame.width), s.height.min(frame.height)) else { return Ok(()) };
        let width = s.width;
        let blit = |s: &mut Shared, f: &dyn Fn(u8) -> u8| {
            for y in r.y..r.y + r.h {
                for x in r.x..r.x + r.w {
                    let v = frame.pixels[(y * frame.width + x) as usize];
                    s.screen[(y * width + x) as usize] = f(v);
                }
            }
        };
        if flash && s.window.is_some() {
            blit(&mut s, &|_| 0);
            s.show();
            std::thread::sleep(Duration::from_millis(120));
        }
        match wf {
            Waveform::Du => blit(&mut s, &|v| if v < 128 { 0 } else { 255 }),
            _ => blit(&mut s, &|v| (v & 0xF0) | (v >> 4)),
        }
        s.show();
        s.write_snapshot()
    }
}

pub struct SimInput {
    s: Rc<RefCell<Shared>>,
    pending: VecDeque<Event>,
}

impl Input for SimInput {
    fn poll(&mut self, timeout_ms: i32) -> io::Result<Vec<Event>> {
        if let Some(e) = self.pending.pop_front() {
            return Ok(vec![e]);
        }
        let mut s = self.s.borrow_mut();
        // Headless: replay scripted taps, then quit.
        if s.window.is_none() {
            return Ok(match s.taps.pop_front() {
                Some((x, y)) => {
                    self.pending.push_back(Event::Touch(Touch { id: 0, phase: TouchPhase::Up, x, y }));
                    vec![Event::Touch(Touch { id: 0, phase: TouchPhase::Down, x, y })]
                }
                None => vec![Event::Quit],
            });
        }
        let start = Instant::now();
        loop {
            let sc = s.scale;
            let (open, esc, down, pos) = {
                let w = s.window.as_mut().unwrap();
                w.update();
                let pos = w.get_mouse_pos(MouseMode::Clamp).map(|(x, y)| ((x / sc) as i32, (y / sc) as i32));
                (w.is_open(), w.is_key_down(Key::Escape), w.get_mouse_down(MouseButton::Left), pos)
            };
            if !open || esc {
                return Ok(vec![Event::Quit]);
            }
            let mut out = Vec::new();
            if let Some((x, y)) = pos {
                let t = |phase| Event::Touch(Touch { id: 0, phase, x, y });
                if down && !s.mouse_down {
                    out.push(t(TouchPhase::Down));
                } else if down && (x, y) != s.last_pos {
                    out.push(t(TouchPhase::Move));
                } else if !down && s.mouse_down {
                    out.push(t(TouchPhase::Up));
                }
                s.mouse_down = down;
                s.last_pos = (x, y);
            }
            if !out.is_empty() {
                return Ok(out);
            }
            if timeout_ms >= 0 && start.elapsed() >= Duration::from_millis(timeout_ms as u64) {
                return Ok(out);
            }
            std::thread::sleep(Duration::from_millis(8));
        }
    }
}
