//! The app runtime: event loop, rendering, e-ink refresh policy.
//!
//! Each update re-renders the whole UI off-screen (cheap), compares it with
//! what the panel shows, and refreshes only the changed rectangles. Big
//! changes, or enough accumulated small ones, get a flashing full refresh
//! to clear ghosting.
//!
//! The runtime also owns the system UI every app shares: the status bar
//! and its menu, the sleep screen, and the PIN lock screen ([`crate::sysui`]).

use crate::clock::{self, LocalTime};
use crate::el::*;
use crate::keyboard::{prompt_view, KeyAct, PromptState};
use crate::layout::{Layouter, Out};
use crate::sysui;
use crate::widgets::{button, button_outline};
use inkpad_gfx::{Fonts, Frame, Painter, Rect, BLACK, WHITE};
use inkpad_hal::{Display, Event, Input, TouchPhase, Waveform};
use inkpad_store::Prefs;
use std::io;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub const STATUS_H: u32 = 38;
/// Accumulated partial-refresh area (in screens) before a cleaning flash.
const GHOST_BUDGET: f32 = 2.5;
/// A single update bigger than this fraction of the screen flashes.
const BIG_CHANGE: f32 = 0.55;
const HOLD: Duration = Duration::from_millis(550);
const SWIPE_PX: i32 = 90;
const SLOP_PX: i32 = 24;
const TOAST_FOR: Duration = Duration::from_millis(2500);
/// The stock screensaver may paint over us right after "going to sleep";
/// repaint the sleep screen once more after this delay.
const SLEEP_REPAINT: Duration = Duration::from_millis(1800);

/// What a view can read about its surroundings.
pub struct Env<'a> {
    pub width: u32,
    /// Height available to the app (below the status bar).
    pub height: u32,
    pub fonts: &'a Fonts,
    pub now: LocalTime,
    /// Battery percent and charging (refreshed every few minutes).
    pub battery: Option<(u8, bool)>,
    /// Wi-Fi switched on.
    pub wifi: Option<bool>,
}

/// Re-opens the display and input after another app had them.
pub type Reopen = Box<dyn Fn() -> io::Result<(Box<dyn Display>, Box<dyn Input>)>>;

type LaunchFn<M> = Box<dyn Fn(Result<i32, String>) -> M>;

/// Commands an app issues from `update`/`setup`.
pub struct Ctx<M> {
    cmds: Vec<Cmd<M>>,
}

type DoneFn<M> = Box<dyn Fn(Option<String>) -> M>;

enum Cmd<M> {
    Quit,
    FullRefresh,
    Toast(String),
    After(Duration, M),
    Spawn(Box<dyn FnOnce() -> M + Send>),
    Prompt(String, String, bool, DoneFn<M>),
    Launch(String, Vec<String>, LaunchFn<M>),
    Lock,
    SystemMenu,
    Exit(i32),
    RefreshStatus,
}

impl<M> Ctx<M> {
    fn new() -> Self {
        Ctx { cmds: Vec::new() }
    }
    /// Exit the app.
    pub fn quit(&mut self) {
        self.cmds.push(Cmd::Quit);
    }
    /// Force a flashing full-screen refresh after this update.
    pub fn full_refresh(&mut self) {
        self.cmds.push(Cmd::FullRefresh);
    }
    /// Show a short message at the bottom of the screen.
    pub fn toast(&mut self, text: impl Into<String>) {
        self.cmds.push(Cmd::Toast(text.into()));
    }
    /// Deliver `msg` after `d`.
    pub fn after(&mut self, d: Duration, msg: M) {
        self.cmds.push(Cmd::After(d, msg));
    }
    /// Run `f` on a background thread (network, disk); its result comes back
    /// as a message.
    pub fn spawn(&mut self, f: impl FnOnce() -> M + Send + 'static) {
        self.cmds.push(Cmd::Spawn(Box::new(f)));
    }
    /// Ask for text with the on-screen keyboard. `on_done` gets `Some(text)`
    /// or `None` if cancelled.
    pub fn prompt(&mut self, title: &str, initial: &str, on_done: impl Fn(Option<String>) -> M + 'static) {
        self.cmds.push(Cmd::Prompt(title.into(), initial.into(), false, Box::new(on_done)));
    }
    /// Like [`Ctx::prompt`] but Enter inserts new lines.
    pub fn prompt_multiline(&mut self, title: &str, initial: &str, on_done: impl Fn(Option<String>) -> M + 'static) {
        self.cmds.push(Cmd::Prompt(title.into(), initial.into(), true, Box::new(on_done)));
    }
    /// Run another inkpad app (a binary next to this one, or a path). This
    /// app's screen and touch are handed over until it exits; then
    /// `on_exit` gets its exit code and the screen is redrawn.
    pub fn launch(&mut self, program: &str, args: &[&str], on_exit: impl Fn(Result<i32, String>) -> M + 'static) {
        self.cmds.push(Cmd::Launch(program.into(), args.iter().map(|s| s.to_string()).collect(), Box::new(on_exit)));
    }
    /// Show the PIN lock screen now (does nothing if no PIN is set).
    pub fn lock(&mut self) {
        self.cmds.push(Cmd::Lock);
    }
    /// Open the system menu (refresh, lock, exit), as tapping the status bar does.
    pub fn system_menu(&mut self) {
        self.cmds.push(Cmd::SystemMenu);
    }
    /// Quit with a process exit code (the launcher script can act on it).
    pub fn exit(&mut self, code: i32) {
        self.cmds.push(Cmd::Exit(code));
    }
    /// Re-read battery and Wi-Fi state now (e.g. after switching Wi-Fi).
    pub fn refresh_status(&mut self) {
        self.cmds.push(Cmd::RefreshStatus);
    }
}

pub trait App: 'static {
    type Msg: Clone + Send + 'static;
    /// Called once before the first frame: load fonts, start timers.
    fn setup(&mut self, _fonts: &mut Fonts, _ctx: &mut Ctx<Self::Msg>) {}
    fn view(&self, env: &Env) -> El<Self::Msg>;
    fn update(&mut self, msg: Self::Msg, ctx: &mut Ctx<Self::Msg>);
    /// Show the system status bar (clock, Wi-Fi, battery).
    fn status_bar(&self) -> bool {
        true
    }
    /// The device is going to sleep.
    fn on_sleep(&mut self, _ctx: &mut Ctx<Self::Msg>) {}
    /// The device woke up (the lock screen, if any, is shown first).
    fn on_wake(&mut self, _ctx: &mut Ctx<Self::Msg>) {}
    /// A custom sleep screen; `None` uses the system one from Settings.
    fn sleep_view(&self, _env: &Env) -> Option<El<Self::Msg>> {
        None
    }
}

/// Internal messages: the app's own, plus the runtime's.
#[derive(Clone)]
enum Rt<M> {
    App(M),
    Key(KeyAct),
    PromptCancel,
    PromptDone,
    SysMenu(bool),
    SysRefresh,
    SysQuit,
    SysLock,
    PinDigit(u8),
    PinBack,
}

/// A stand-in display/input while another app owns the real ones.
struct Released(u32, u32);

impl Display for Released {
    fn size(&self) -> (u32, u32) {
        (self.0, self.1)
    }
    fn present(&mut self, _: &Frame, _: Rect, _: Waveform, _: bool) -> io::Result<()> {
        Ok(())
    }
}

impl Input for Released {
    fn poll(&mut self, _: i32) -> io::Result<Vec<Event>> {
        Ok(Vec::new())
    }
}

/// Where to find an app binary: a path as given, or a name next to the
/// running executable.
fn resolve_program(name: &str) -> PathBuf {
    let p = PathBuf::from(name);
    if p.components().count() > 1 || p.is_absolute() {
        return p;
    }
    let dir = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.to_path_buf())).unwrap_or_default();
    dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX))
}

/// Whether an app binary is installed next to the running one.
pub fn app_installed(name: &str) -> bool {
    resolve_program(name).exists()
}

struct Lock {
    entered: String,
    error: bool,
}

struct Pressed<M> {
    start: (i32, i32),
    at: Instant,
    tap: Option<Rt<M>>,
    hold: Option<Rt<M>>,
    moved: bool,
}

struct Runtime<A: App> {
    app: A,
    fonts: Fonts,
    display: Box<dyn Display>,
    input: Box<dyn Input>,
    w: u32,
    h: u32,
    frame: Frame,
    shown: Frame,
    first: bool,
    force_full: bool,
    ghost: f32,
    out: Out<Rt<A::Msg>>,
    prompt: Option<(PromptState, DoneFn<A::Msg>)>,
    toast: Option<(String, Instant)>,
    sys_menu: bool,
    timers: Vec<(Instant, A::Msg)>,
    tx: mpsc::Sender<A::Msg>,
    rx: mpsc::Receiver<A::Msg>,
    running: usize,
    pressed: Option<Pressed<A::Msg>>,
    status: (Option<(u8, bool)>, Option<bool>, Instant),
    quit: bool,
    exit_code: i32,
    sleeping: bool,
    sleep_repaint: Option<Instant>,
    lock: Option<Lock>,
    reopen: Option<Reopen>,
    launches: Vec<(String, Vec<String>, LaunchFn<A::Msg>)>,
}

/// Run `app` on the given display and input until it quits.
pub fn run_on<A: App>(app: A, display: Box<dyn Display>, input: Box<dyn Input>) -> io::Result<()> {
    run_with(app, display, input, None)
}

/// Like [`run_on`], with a way to reopen the display and input, which lets
/// the app launch other apps ([`Ctx::launch`]).
pub fn run_with<A: App>(mut app: A, display: Box<dyn Display>, input: Box<dyn Input>, reopen: Option<Reopen>) -> io::Result<()> {
    let (w, h) = display.size();
    let mut fonts = Fonts::new();
    let mut ctx = Ctx::new();
    app.setup(&mut fonts, &mut ctx);
    let (tx, rx) = mpsc::channel();
    let mut rt = Runtime {
        app,
        fonts,
        display,
        input,
        w,
        h,
        frame: Frame::new(w, h),
        shown: Frame::new(w, h),
        first: true,
        force_full: false,
        ghost: 0.0,
        out: Out::default(),
        prompt: None,
        toast: None,
        sys_menu: false,
        timers: Vec::new(),
        tx,
        rx,
        running: 0,
        pressed: None,
        status: (inkpad_hal::system::battery(), inkpad_hal::system::wifi_on(), Instant::now()),
        quit: false,
        exit_code: 0,
        sleeping: false,
        sleep_repaint: None,
        lock: None,
        reopen,
        launches: Vec::new(),
    };
    rt.apply(ctx);
    rt.render()?;
    rt.run()?;
    if rt.exit_code != 0 {
        // Give the screen and touch back cleanly before leaving.
        rt.display = Box::new(Released(0, 0));
        rt.input = Box::new(Released(0, 0));
        std::process::exit(rt.exit_code);
    }
    Ok(())
}

impl<A: App> Runtime<A> {
    fn apply(&mut self, ctx: Ctx<A::Msg>) {
        for c in ctx.cmds {
            match c {
                Cmd::Quit => self.quit = true,
                Cmd::FullRefresh => self.force_full = true,
                Cmd::Toast(t) => self.toast = Some((t, Instant::now() + TOAST_FOR)),
                Cmd::After(d, m) => self.timers.push((Instant::now() + d, m)),
                Cmd::Spawn(f) => {
                    let tx = self.tx.clone();
                    self.running += 1;
                    std::thread::spawn(move || {
                        let _ = tx.send(f());
                    });
                }
                Cmd::Prompt(title, initial, multi, done) => self.prompt = Some((PromptState::new(&title, &initial, multi), done)),
                Cmd::Launch(prog, args, done) => self.launches.push((prog, args, done)),
                Cmd::Lock => self.lock_now(),
                Cmd::SystemMenu => self.sys_menu = true,
                Cmd::Exit(code) => {
                    self.quit = true;
                    self.exit_code = code;
                }
                Cmd::RefreshStatus => self.refresh_status(),
            }
        }
    }

    fn refresh_status(&mut self) {
        self.status = (inkpad_hal::system::battery(), inkpad_hal::system::wifi_on(), Instant::now());
    }

    fn lock_now(&mut self) {
        if sysui::lock_enabled(&Prefs::system()) {
            self.lock = Some(Lock { entered: String::new(), error: false });
            self.sys_menu = false;
            self.force_full = true;
        }
    }

    /// Hand the screen to another app until it exits.
    fn launch(&mut self, prog: &str, args: &[String]) -> Result<i32, String> {
        let Some(reopen) = &self.reopen else { return Err("this app cannot launch others".into()) };
        let path = resolve_program(prog);
        if !path.exists() {
            return Err(format!("{} is not installed", path.display()));
        }
        eprintln!("launch: {} {args:?}", path.display());
        self.display = Box::new(Released(self.w, self.h));
        self.input = Box::new(Released(self.w, self.h));
        // Scripted simulator input belongs to this process, not the child.
        let mut cmd = std::process::Command::new(&path);
        cmd.args(args).env_remove("INKPAD_TAPS");
        if let Some(dir) = path.parent() {
            cmd.current_dir(dir);
        }
        let status = cmd.status();
        match reopen() {
            Ok((d, i)) => {
                self.display = d;
                self.input = i;
            }
            Err(e) => {
                eprintln!("launch: could not take the screen back: {e}");
                self.quit = true;
                return Err(e.to_string());
            }
        }
        self.first = true;
        self.pressed = None;
        self.refresh_status();
        let code = status.map_err(|e| e.to_string())?.code().unwrap_or(-1);
        eprintln!("launch: exited with {code}");
        Ok(code)
    }

    fn go_to_sleep(&mut self) {
        if self.sleeping {
            return;
        }
        let mut ctx = Ctx::new();
        self.app.on_sleep(&mut ctx);
        self.apply(ctx);
        self.sleeping = true;
        self.sys_menu = false;
        self.toast = None;
        self.pressed = None;
        self.force_full = true;
        self.sleep_repaint = Some(Instant::now() + SLEEP_REPAINT);
    }

    fn wake_up(&mut self) {
        if !self.sleeping {
            return;
        }
        self.sleeping = false;
        self.sleep_repaint = None;
        self.refresh_status();
        self.lock_now();
        let mut ctx = Ctx::new();
        self.app.on_wake(&mut ctx);
        self.apply(ctx);
        self.force_full = true;
    }

    fn dispatch(&mut self, msg: A::Msg) {
        let mut ctx = Ctx::new();
        self.app.update(msg, &mut ctx);
        self.apply(ctx);
    }

    fn handle(&mut self, m: Rt<A::Msg>) {
        match m {
            Rt::App(msg) => self.dispatch(msg),
            Rt::Key(k) => {
                let submit = self.prompt.as_mut().map(|(p, _)| p.key(&k)).unwrap_or(false);
                if submit {
                    self.handle(Rt::PromptDone);
                }
            }
            Rt::PromptCancel | Rt::PromptDone => {
                if let Some((p, done)) = self.prompt.take() {
                    let result = matches!(m, Rt::PromptDone).then_some(p.text);
                    self.dispatch(done(result));
                    self.force_full = true;
                }
            }
            Rt::SysMenu(open) => self.sys_menu = open,
            Rt::SysRefresh => {
                self.sys_menu = false;
                self.force_full = true;
            }
            Rt::SysQuit => self.quit = true,
            Rt::SysLock => {
                self.sys_menu = false;
                self.lock_now();
            }
            Rt::PinDigit(d) => {
                let prefs = Prefs::system();
                if let Some(l) = &mut self.lock {
                    l.error = false;
                    l.entered.push(char::from(b'0' + d));
                    if l.entered.len() >= sysui::pin_len(&prefs) {
                        if sysui::check_pin(&prefs, &l.entered) {
                            self.lock = None;
                            self.force_full = true;
                        } else {
                            l.entered.clear();
                            l.error = true;
                        }
                    }
                }
            }
            Rt::PinBack => {
                if let Some(l) = &mut self.lock {
                    l.entered.pop();
                }
            }
        }
    }

    fn status_view(&self) -> El<Rt<A::Msg>> {
        let now = clock::now();
        let (bat, wifi, _) = self.status;
        let mut right: Vec<El<Rt<A::Msg>>> = Vec::new();
        if let Some(on) = wifi {
            right.push(icon(if on { "wifi" } else { "wifi-off" }, 22));
        }
        if let Some((pct, charging)) = bat {
            right.push(text(format!("{pct}%")).size(17).bold());
            right.push(sysui::battery_icon(pct, charging));
        }
        let mut kids = vec![text(now.hm()).size(18).bold(), spacer()];
        kids.extend(right);
        col(vec![row(kids).gap(8).pad(20, 0).fill_h(), rule(1, 120)]).h(STATUS_H).on_tap(Rt::SysMenu(true))
    }

    fn sys_menu_view(&self) -> El<Rt<A::Msg>> {
        let mut items = vec![
            text("inkpad").size(28).bold(),
            text({
                let n = clock::now();
                format!("{} · {}", n.long_date(), n.hm())
            })
            .size(20)
            .color(60),
            button_outline("Refresh screen", Rt::SysRefresh).fill_w(),
        ];
        if sysui::lock_enabled(&Prefs::system()) {
            items.push(button_outline("Lock", Rt::SysLock).fill_w());
        }
        items.push(button_outline("Exit to Kindle", Rt::SysQuit).fill_w());
        items.push(button("Close", Rt::SysMenu(false)).fill_w());
        let card = col(items)
        .gap(16)
        .padding(28)
        .bg(WHITE)
        .border(3.0, BLACK)
        .radius(18.0);
        col(vec![empty().h(STATUS_H + 8), card, spacer()]).pad(40, 0).block_taps().on_tap(Rt::SysMenu(false))
    }

    fn toast_view(text_: &str) -> El<Rt<A::Msg>> {
        let pill = row(vec![text(text_.to_string()).size(20).bold().color(WHITE).max_lines(2)]).pad(24, 14).bg(BLACK).radius(26.0);
        col(vec![spacer(), row(vec![spacer(), pill, spacer()])]).pad_edges(Edges { top: 0, right: 20, bottom: 110, left: 20 })
    }

    fn root_view(&self) -> El<Rt<A::Msg>> {
        let now = clock::now();
        let wrap: Rc<dyn Fn(A::Msg) -> Rt<A::Msg>> = Rc::new(Rt::App);
        if self.sleeping {
            let env = Env { width: self.w, height: self.h, fonts: &self.fonts, now, battery: self.status.0, wifi: self.status.1 };
            return match self.app.sleep_view(&env) {
                Some(v) => v.map(&wrap).fill(),
                None => sysui::sleep_view(&Prefs::system(), &now, self.w, self.h).fill(),
            };
        }
        if let Some(l) = &self.lock {
            return sysui::lock_view(&Prefs::system(), &now, self.w, self.h, l.entered.len(), l.error, Rt::PinDigit, Rt::PinBack).fill().block_taps();
        }
        let app_h = if self.app.status_bar() { self.h - STATUS_H } else { self.h };
        let env = Env { width: self.w, height: app_h, fonts: &self.fonts, now, battery: self.status.0, wifi: self.status.1 };
        let app_el = self.app.view(&env).map(&wrap).fill();
        let mut root = if self.app.status_bar() { col(vec![self.status_view(), app_el]) } else { col(vec![app_el]) };
        if self.sys_menu {
            root = stack(vec![root, self.sys_menu_view()]);
        }
        if let Some((p, _)) = &self.prompt {
            root = stack(vec![root, prompt_view(p, &Rt::Key, Rt::PromptCancel, Rt::PromptDone)]);
        }
        if let Some((t, _)) = &self.toast {
            root = stack(vec![root, Self::toast_view(t)]);
        }
        root
    }

    fn render(&mut self) -> io::Result<()> {
        let root = self.root_view();
        let layouter = Layouter::new(&self.fonts);
        let mut out = Out::default();
        layouter.place(&root, Rect::new(0, 0, self.w, self.h), &mut out);
        self.frame.pixels.fill(WHITE);
        layouter.draw(&out.ops, &mut Painter::new(&mut self.frame));
        self.out = out;
        self.flush()
    }

    /// Push differences between `frame` and `shown` to the panel.
    fn flush(&mut self) -> io::Result<()> {
        let full = self.frame.full();
        if self.first || self.force_full {
            self.first = false;
            self.force_full = false;
            self.ghost = 0.0;
            self.shown.pixels.copy_from_slice(&self.frame.pixels);
            return self.display.present(&self.frame, full, Waveform::Gc16, true);
        }
        let rects = diff(&self.shown, &self.frame);
        if rects.is_empty() {
            return Ok(());
        }
        let screen = (self.w * self.h) as f32;
        let area: f32 = rects.iter().map(|r| (r.w * r.h) as f32).sum::<f32>() / screen;
        self.shown.pixels.copy_from_slice(&self.frame.pixels);
        if area > BIG_CHANGE || self.ghost + area > GHOST_BUDGET {
            self.ghost = 0.0;
            return self.display.present(&self.frame, full, Waveform::Gc16, true);
        }
        self.ghost += area;
        for r in rects {
            self.display.present(&self.frame, r, Waveform::Gl16, false)?;
        }
        Ok(())
    }

    fn hit_at(&self, x: i32, y: i32) -> Option<(Rect, Option<Rt<A::Msg>>, Option<Rt<A::Msg>>)> {
        self.out.hits.iter().rev().find(|h| h.rect.contains(x, y)).map(|h| (h.rect, h.tap.clone(), h.hold.clone()))
    }

    /// Instant visual feedback: invert the pressed area with a fast waveform.
    fn press_feedback(&mut self, r: Rect) -> io::Result<()> {
        let mut tmp = Frame { width: self.w, height: self.h, pixels: self.shown.pixels.clone() };
        Painter::new(&mut tmp).invert_rect(r);
        self.display.present(&tmp, r, Waveform::Du, false)?;
        // Remember that the panel shows the inverted area.
        Painter::new(&mut self.shown).invert_rect(r);
        Ok(())
    }

    fn on_touch(&mut self, phase: TouchPhase, x: i32, y: i32) -> io::Result<bool> {
        match phase {
            TouchPhase::Down => {
                let hit = self.hit_at(x, y);
                let (rect, tap, hold) = match hit {
                    Some((r, t, h)) => (Some(r), t, h),
                    None => (None, None, None),
                };
                let rect = rect.filter(|r| (tap.is_some() || hold.is_some()) && (r.w * r.h) < self.w * self.h / 3);
                if let Some(r) = rect {
                    self.press_feedback(r)?;
                }
                self.pressed = Some(Pressed { start: (x, y), at: Instant::now(), tap, hold, moved: false });
                Ok(false)
            }
            TouchPhase::Move => {
                if let Some(p) = &mut self.pressed {
                    if (x - p.start.0).abs() > SLOP_PX || (y - p.start.1).abs() > SLOP_PX {
                        p.moved = true;
                    }
                }
                Ok(false)
            }
            TouchPhase::Up => {
                let Some(p) = self.pressed.take() else { return Ok(false) };
                let (dx, dy) = (x - p.start.0, y - p.start.1);
                if p.moved && (dx.abs() > SWIPE_PX || dy.abs() > SWIPE_PX) {
                    let forward = if dx.abs() > dy.abs() { dx < 0 } else { dy < 0 };
                    let sw = self.out.swipes.iter().rev().find(|s| s.rect.contains(p.start.0, p.start.1));
                    let msg = sw.and_then(|s| if forward { s.next.clone() } else { s.prev.clone() });
                    if let Some(m) = msg {
                        self.handle(m);
                    }
                    return Ok(true);
                }
                if p.moved {
                    return Ok(true); // cancelled press; re-render restores it
                }
                let msg = if p.at.elapsed() >= HOLD { p.hold.or(p.tap) } else { p.tap };
                if let Some(m) = msg {
                    self.handle(m);
                }
                Ok(true)
            }
        }
    }

    fn next_wake(&self) -> i32 {
        let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
        let to_minute = Duration::from_millis(60_000 - ms % 60_000 + 20);
        let mut wake = to_minute;
        let inst = Instant::now();
        for (t, _) in &self.timers {
            wake = wake.min(t.saturating_duration_since(inst));
        }
        if let Some((_, t)) = &self.toast {
            wake = wake.min(t.saturating_duration_since(inst));
        }
        if self.running > 0 {
            wake = wake.min(Duration::from_millis(100));
        }
        if let Some(t) = self.sleep_repaint {
            wake = wake.min(t.saturating_duration_since(inst));
        }
        wake.as_millis().min(60_000) as i32
    }

    fn run(&mut self) -> io::Result<()> {
        let mut minute = clock::unix_now() / 60;
        while !self.quit {
            let events = self.input.poll(self.next_wake())?;
            let mut dirty = false;
            for ev in events {
                match ev {
                    Event::Quit => return Ok(()),
                    Event::Sleep => {
                        self.go_to_sleep();
                        dirty = true;
                    }
                    Event::Wake => {
                        self.wake_up();
                        dirty = true;
                    }
                    Event::Touch(_) if self.sleeping => {}
                    Event::Touch(t) if t.id == 0 => dirty |= self.on_touch(t.phase, t.x, t.y)?,
                    Event::Touch(_) => {}
                    Event::Char(c) if self.prompt.is_some() => {
                        self.handle(Rt::Key(KeyAct::Char(c)));
                        dirty = true;
                    }
                    Event::Backspace if self.prompt.is_some() => {
                        self.handle(Rt::Key(KeyAct::Back));
                        dirty = true;
                    }
                    Event::Enter if self.prompt.is_some() => {
                        self.handle(Rt::Key(KeyAct::Enter));
                        dirty = true;
                    }
                    _ => {}
                }
            }
            // Background results.
            while let Ok(m) = self.rx.try_recv() {
                self.running = self.running.saturating_sub(1);
                self.dispatch(m);
                dirty = true;
            }
            // Timers.
            let now = Instant::now();
            let (due, later): (Vec<_>, Vec<_>) = std::mem::take(&mut self.timers).into_iter().partition(|(t, _)| *t <= now);
            self.timers = later;
            for (_, m) in due {
                self.dispatch(m);
                dirty = true;
            }
            if self.toast.as_ref().is_some_and(|(_, t)| *t <= now) {
                self.toast = None;
                dirty = true;
            }
            if self.sleep_repaint.is_some_and(|t| t <= now) {
                self.sleep_repaint = None;
                self.force_full = true;
            }
            // Apps launched from `update`.
            for (prog, args, done) in std::mem::take(&mut self.launches) {
                let r = self.launch(&prog, &args);
                if let Err(e) = &r {
                    eprintln!("launch {prog}: {e}");
                }
                self.dispatch(done(r));
                dirty = true;
            }
            // Clock and device status.
            let m = clock::unix_now() / 60;
            if m != minute {
                minute = m;
                dirty = true;
                if self.status.2.elapsed() > Duration::from_secs(240) && !self.sleeping {
                    self.refresh_status();
                }
            }
            if self.quit {
                break;
            }
            if dirty || self.force_full {
                self.render()?;
            }
        }
        Ok(())
    }
}

/// Rectangles where `a` and `b` differ, grouped into horizontal bands.
pub fn diff(a: &Frame, b: &Frame) -> Vec<Rect> {
    let w = a.width as usize;
    let mut bands: Vec<(u32, u32, u32, u32)> = Vec::new(); // x0, y0, x1, y1 (exclusive)
    for y in 0..a.height as usize {
        let (ra, rb) = (&a.pixels[y * w..(y + 1) * w], &b.pixels[y * w..(y + 1) * w]);
        if ra == rb {
            continue;
        }
        let x0 = ra.iter().zip(rb).position(|(p, q)| p != q).unwrap() as u32;
        let x1 = w as u32 - ra.iter().zip(rb).rev().position(|(p, q)| p != q).unwrap() as u32;
        let y = y as u32;
        match bands.last_mut() {
            Some(bd) if y <= bd.3 + 8 => {
                bd.0 = bd.0.min(x0);
                bd.2 = bd.2.max(x1);
                bd.3 = y + 1;
            }
            _ => bands.push((x0, y, x1, y + 1)),
        }
    }
    // Too many pieces: merge into one.
    if bands.len() > 6 {
        let x0 = bands.iter().map(|b| b.0).min().unwrap();
        let x1 = bands.iter().map(|b| b.2).max().unwrap();
        bands = vec![(x0, bands[0].1, x1, bands.last().unwrap().3)];
    }
    bands.into_iter().map(|(x0, y0, x1, y1)| Rect::new(x0, y0, x1 - x0, y1 - y0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_finds_bands() {
        let a = Frame::new(100, 100);
        let mut b = Frame::new(100, 100);
        let mut p = Painter::new(&mut b);
        p.fill_rect(Rect::new(10, 10, 5, 5), 0);
        p.fill_rect(Rect::new(50, 12, 5, 5), 0);
        p.fill_rect(Rect::new(20, 80, 10, 2), 0);
        let d = diff(&a, &b);
        assert_eq!(d, vec![Rect::new(10, 10, 45, 7), Rect::new(20, 80, 10, 2)]);
        assert!(diff(&a, &a).is_empty());
    }
}
