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
//! - `INKPAD_TAPS` scripted input for snapshot mode, `;`-separated steps:
//!   `x,y` tap · `hold:x,y` long press · `swipe:x0,y0,x1,y1` ·
//!   `type:text` · `enter` · `back` (backspace) · `wait:ms`.
//!
//! Keys: typing goes to text fields; Esc closes the window.

use inkpad_hal::{Display, Event, Frame, Input, Rect, Touch, TouchPhase, Waveform};
use minifb::{InputCallback, Key, MouseButton, MouseMode, Scale, Window, WindowOptions};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::io;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// One scripted step: events to deliver, and a pause before the next one.
type Step = (Vec<Event>, u64);

fn touch(phase: TouchPhase, x: i32, y: i32) -> Event {
    Event::Touch(Touch { id: 0, phase, x, y })
}

fn parse_script(s: &str) -> VecDeque<Step> {
    let nums = |s: &str| -> Vec<i32> { s.split(',').filter_map(|n| n.trim().parse().ok()).collect() };
    let mut out = VecDeque::new();
    for step in s.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        let (kind, arg) = match step.split_once(':') {
            Some(p) => p,
            None if step.contains(',') => ("tap", step),
            None => (step, ""),
        };
        match kind {
            "tap" => {
                if let [x, y] = nums(arg)[..] {
                    out.push_back((vec![touch(TouchPhase::Down, x, y)], 0));
                    out.push_back((vec![touch(TouchPhase::Up, x, y)], 0));
                }
            }
            "hold" => {
                if let [x, y] = nums(arg)[..] {
                    out.push_back((vec![touch(TouchPhase::Down, x, y)], 800));
                    out.push_back((vec![touch(TouchPhase::Up, x, y)], 0));
                }
            }
            "swipe" => {
                if let [x0, y0, x1, y1] = nums(arg)[..] {
                    out.push_back((vec![touch(TouchPhase::Down, x0, y0)], 0));
                    out.push_back((vec![touch(TouchPhase::Move, (x0 + x1) / 2, (y0 + y1) / 2)], 0));
                    out.push_back((vec![touch(TouchPhase::Move, x1, y1)], 0));
                    out.push_back((vec![touch(TouchPhase::Up, x1, y1)], 0));
                }
            }
            "type" => out.push_back((arg.chars().map(Event::Char).collect(), 0)),
            "enter" => out.push_back((vec![Event::Enter], 0)),
            "back" => out.push_back((vec![Event::Backspace], 0)),
            "wait" => out.push_back((vec![], arg.trim().parse().unwrap_or(0))),
            _ => eprintln!("sim: unknown script step {step:?}"),
        }
    }
    out
}

struct Typed(Arc<Mutex<VecDeque<Event>>>);

impl InputCallback for Typed {
    fn add_char(&mut self, c: u32) {
        let ev = match c {
            8 => Event::Backspace,
            13 | 10 => Event::Enter,
            c if c < 32 || c == 127 => return,
            c => match char::from_u32(c) {
                Some(ch) => Event::Char(ch),
                None => return,
            },
        };
        self.0.lock().unwrap().push_back(ev);
    }
}

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
    script: VecDeque<Step>,
    typed: Arc<Mutex<VecDeque<Event>>>,
    pause_next: u64,
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
    let script = parse_script(&std::env::var("INKPAD_TAPS").unwrap_or_default());
    let typed = Arc::new(Mutex::new(VecDeque::new()));
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
        w.set_input_callback(Box::new(Typed(typed.clone())));
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
        script,
        typed,
        pause_next: 0,
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
        // Headless: replay the script, then quit.
        if s.window.is_none() {
            // Pause *after* a step (e.g. between a hold's down and up).
            std::thread::sleep(Duration::from_millis(std::mem::take(&mut s.pause_next)));
            return Ok(match s.script.pop_front() {
                Some((events, pause)) => {
                    s.pause_next = pause;
                    events
                }
                None => vec![Event::Quit],
            });
        }
        let typed: Vec<Event> = s.typed.lock().unwrap().drain(..).collect();
        if !typed.is_empty() {
            return Ok(typed);
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
