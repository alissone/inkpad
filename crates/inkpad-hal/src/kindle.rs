//! Kindle backend: i.MX EPDC framebuffer (`mxcfb`) and evdev multitouch.
//!
//! The ioctl numbers and struct layouts are the Linux userspace ABI of the
//! Kindle (Lab126) `mxcfb` driver and the standard fbdev/evdev interfaces.

use crate::{Display, Event, Frame, Input, Rect, Touch, TouchPhase, Waveform};
use std::ffi::CString;
use std::io;

// ---- fbdev ---------------------------------------------------------------

const FBIOGET_VSCREENINFO: u32 = 0x4600;
const FBIOGET_FSCREENINFO: u32 = 0x4602;

// ---- Kindle mxcfb --------------------------------------------------------

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct MxcfbRect {
    top: u32,
    left: u32,
    width: u32,
    height: u32,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct MxcfbAltBuffer {
    phys_addr: u32,
    width: u32,
    height: u32,
    alt_update_region: MxcfbRect,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct MxcfbUpdateData {
    update_region: MxcfbRect,
    waveform_mode: u32,
    update_mode: u32,
    update_marker: u32,
    hist_bw_waveform_mode: u32,
    hist_gray_waveform_mode: u32,
    temp: i32,
    flags: u32,
    alt_buffer_data: MxcfbAltBuffer,
}
const _: () = assert!(std::mem::size_of::<MxcfbUpdateData>() == 72);

#[repr(C)]
struct MxcfbMarkerData {
    update_marker: u32,
    collision_test: u32,
}

/// `_IOW('F', 0x2E, struct mxcfb_update_data)` (72 bytes).
const MXCFB_SEND_UPDATE: u32 = 0x4048_462E;
/// `_IOWR('F', 0x2F, struct mxcfb_update_marker_data)` — PW2-era and newer.
const MXCFB_WAIT_FOR_UPDATE_COMPLETE: u32 = 0xC008_462F;
/// `_IOW('F', 0x2F, __u32)` — older Kindles; used as a fallback.
const MXCFB_WAIT_FOR_UPDATE_COMPLETE_OLD: u32 = 0x4004_462F;

const WAVEFORM_MODE_DU: u32 = 1;
const WAVEFORM_MODE_GC16: u32 = 2;
const WAVEFORM_MODE_GL16: u32 = 5;
const WAVEFORM_MODE_AUTO: u32 = 257;
const UPDATE_MODE_PARTIAL: u32 = 0;
const UPDATE_MODE_FULL: u32 = 1;
const TEMP_USE_AMBIENT: i32 = 0x1000;

fn ioctl_ptr<T>(fd: i32, req: u32, arg: *mut T) -> io::Result<()> {
    // SAFETY: callers pass a pointer to a correctly sized repr(C) value for `req`.
    let r = unsafe { libc::ioctl(fd, req as _, arg) };
    if r < 0 { Err(io::Error::last_os_error()) } else { Ok(()) }
}

fn open(path: &str, flags: i32) -> io::Result<i32> {
    let c = CString::new(path).unwrap();
    // SAFETY: valid NUL-terminated path.
    let fd = unsafe { libc::open(c.as_ptr(), flags | libc::O_CLOEXEC) };
    if fd < 0 { Err(io::Error::new(io::Error::last_os_error().kind(), format!("{path}: {}", io::Error::last_os_error()))) } else { Ok(fd) }
}

fn rd_u32(b: &[u8], off: usize) -> u32 {
    u32::from_ne_bytes(b[off..off + 4].try_into().unwrap())
}

pub struct KindleDisplay {
    fd: i32,
    map: *mut u8,
    map_len: usize,
    /// Byte offset of the visible screen inside the mapping.
    origin: usize,
    stride: usize,
    width: u32,
    height: u32,
    /// Panel stores 0 = white (inverted); flip on copy.
    invert: bool,
    marker: u32,
    old_wait: bool,
}

impl KindleDisplay {
    pub fn open() -> io::Result<Self> {
        let fd = open("/dev/fb0", libc::O_RDWR)?;
        let mut var = [0u8; 160];
        let mut fix = [0u8; 68];
        ioctl_ptr(fd, FBIOGET_VSCREENINFO, var.as_mut_ptr())?;
        ioctl_ptr(fd, FBIOGET_FSCREENINFO, fix.as_mut_ptr())?;
        let (xres, yres) = (rd_u32(&var, 0), rd_u32(&var, 4));
        let (xoff, yoff) = (rd_u32(&var, 16), rd_u32(&var, 20));
        let bpp = rd_u32(&var, 24);
        let grayscale = rd_u32(&var, 28);
        let rotate = rd_u32(&var, 136);
        let smem_len = rd_u32(&fix, 20) as usize;
        let visual = rd_u32(&fix, 32);
        let stride = rd_u32(&fix, 44) as usize;
        let id = String::from_utf8_lossy(&fix[..16]).trim_end_matches('\0').to_string();
        eprintln!(
            "fb: id={id} {xres}x{yres} off={xoff},{yoff} bpp={bpp} grayscale={grayscale} rotate={rotate} \
             stride={stride} smem_len={smem_len} visual={visual}"
        );
        if bpp != 8 {
            return Err(io::Error::new(io::ErrorKind::Unsupported, format!("unsupported bpp {bpp}")));
        }
        // SAFETY: mapping the framebuffer device memory, length from the driver.
        let map = unsafe {
            libc::mmap(std::ptr::null_mut(), smem_len, libc::PROT_READ | libc::PROT_WRITE, libc::MAP_SHARED, fd, 0)
        };
        if map == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }
        let invert = match std::env::var("INKPAD_INVERT").as_deref() {
            Ok("1") => true,
            Ok("0") => false,
            _ => false,
        };
        Ok(KindleDisplay {
            fd,
            map: map as *mut u8,
            map_len: smem_len,
            origin: yoff as usize * stride + xoff as usize,
            stride,
            width: xres,
            height: yres,
            invert,
            marker: 0,
            old_wait: false,
        })
    }

    fn wait(&mut self, marker: u32) {
        if !self.old_wait {
            let mut d = MxcfbMarkerData { update_marker: marker, collision_test: 0 };
            match ioctl_ptr(self.fd, MXCFB_WAIT_FOR_UPDATE_COMPLETE, &mut d) {
                Ok(()) => return,
                Err(e) if e.raw_os_error() == Some(libc::ENOTTY) || e.raw_os_error() == Some(libc::EINVAL) => {
                    eprintln!("fb: new wait ioctl failed ({e}); using old one");
                    self.old_wait = true;
                }
                Err(e) => {
                    eprintln!("fb: wait failed: {e}");
                    return;
                }
            }
        }
        let mut m = marker;
        if let Err(e) = ioctl_ptr(self.fd, MXCFB_WAIT_FOR_UPDATE_COMPLETE_OLD, &mut m) {
            eprintln!("fb: old wait failed: {e}");
        }
    }
}

impl Drop for KindleDisplay {
    fn drop(&mut self) {
        // SAFETY: unmapping what we mapped; closing our fd.
        unsafe {
            libc::munmap(self.map as *mut _, self.map_len);
            libc::close(self.fd);
        }
    }
}

impl Display for KindleDisplay {
    fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn present(&mut self, frame: &Frame, region: Rect, wf: Waveform, flash: bool) -> io::Result<()> {
        let Some(r) = region.clip(self.width.min(frame.width), self.height.min(frame.height)) else {
            return Ok(());
        };
        // Copy rows into the framebuffer.
        for y in r.y..r.y + r.h {
            let src = &frame.pixels[(y * frame.width + r.x) as usize..(y * frame.width + r.x + r.w) as usize];
            let off = self.origin + y as usize * self.stride + r.x as usize;
            if off + src.len() > self.map_len {
                break;
            }
            // SAFETY: bounds checked against the mapping length above.
            let dst = unsafe { std::slice::from_raw_parts_mut(self.map.add(off), src.len()) };
            if self.invert {
                for (d, s) in dst.iter_mut().zip(src) {
                    *d = !*s;
                }
            } else {
                dst.copy_from_slice(src);
            }
        }
        self.marker = self.marker.wrapping_add(1).max(1);
        let mut upd = MxcfbUpdateData {
            update_region: MxcfbRect { top: r.y, left: r.x, width: r.w, height: r.h },
            waveform_mode: match wf {
                Waveform::Auto => WAVEFORM_MODE_AUTO,
                Waveform::Du => WAVEFORM_MODE_DU,
                Waveform::Gc16 => WAVEFORM_MODE_GC16,
                Waveform::Gl16 => WAVEFORM_MODE_GL16,
            },
            update_mode: if flash { UPDATE_MODE_FULL } else { UPDATE_MODE_PARTIAL },
            update_marker: self.marker,
            hist_bw_waveform_mode: WAVEFORM_MODE_DU,
            hist_gray_waveform_mode: WAVEFORM_MODE_GC16,
            temp: TEMP_USE_AMBIENT,
            flags: 0,
            alt_buffer_data: MxcfbAltBuffer::default(),
        };
        ioctl_ptr(self.fd, MXCFB_SEND_UPDATE, &mut upd)?;
        // Flashing updates: wait so the next draw doesn't collide with them.
        if flash {
            self.wait(self.marker);
        }
        Ok(())
    }
}

// ---- evdev touch ---------------------------------------------------------

const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const EV_ABS: u16 = 0x03;
const SYN_REPORT: u16 = 0;
const BTN_TOUCH: u16 = 0x14a;
const ABS_X: u16 = 0x00;
const ABS_Y: u16 = 0x01;
const ABS_MT_SLOT: u16 = 0x2f;
const ABS_MT_POSITION_X: u16 = 0x35;
const ABS_MT_POSITION_Y: u16 = 0x36;
const ABS_MT_TRACKING_ID: u16 = 0x39;
/// `_IOW('E', 0x90, int)`
const EVIOCGRAB: u32 = 0x4004_4590;
/// `_IOR('E', 0x40 + abs, struct input_absinfo)` (24 bytes)
const fn eviocgabs(abs: u16) -> u32 {
    0x8018_4540 + abs as u32
}
const MAX_SLOTS: usize = 10;

#[derive(Clone, Copy, Default)]
struct Slot {
    id: i32,
    x: i32,
    y: i32,
    /// What the app last saw for this slot.
    reported_id: i32,
    rx: i32,
    ry: i32,
}

/// How raw panel coordinates map to the screen. Set with `INKPAD_TOUCH`,
/// a comma list of `swap`, `mirror_x`, `mirror_y`.
#[derive(Clone, Copy, Default, Debug)]
pub struct TouchTransform {
    pub swap: bool,
    pub mirror_x: bool,
    pub mirror_y: bool,
}

pub struct KindleTouch {
    fd: i32,
    slots: [Slot; MAX_SLOTS],
    cur: usize,
    range_x: (i32, i32),
    range_y: (i32, i32),
    screen: (u32, u32),
    xf: TouchTransform,
    /// Single-touch fallback state (no MT slots seen).
    st_down: bool,
    saw_mt: bool,
}

fn abs_range(fd: i32, abs: u16) -> Option<(i32, i32)> {
    let mut info = [0i32; 6];
    ioctl_ptr(fd, eviocgabs(abs), info.as_mut_ptr()).ok()?;
    (info[2] > info[1]).then_some((info[1], info[2]))
}

impl KindleTouch {
    pub fn open(path: &str, screen: (u32, u32)) -> io::Result<Self> {
        let fd = open(path, libc::O_RDONLY | libc::O_NONBLOCK)?;
        // Grab so the stock UI doesn't also react to our touches.
        if let Err(e) = ioctl_ptr(fd, EVIOCGRAB, 1usize as *mut i32) {
            eprintln!("touch: EVIOCGRAB failed: {e}");
        }
        let range_x = abs_range(fd, ABS_MT_POSITION_X).or_else(|| abs_range(fd, ABS_X)).unwrap_or((0, screen.0 as i32 - 1));
        let range_y = abs_range(fd, ABS_MT_POSITION_Y).or_else(|| abs_range(fd, ABS_Y)).unwrap_or((0, screen.1 as i32 - 1));
        let mut xf = TouchTransform::default();
        if let Ok(v) = std::env::var("INKPAD_TOUCH") {
            for p in v.split(',') {
                match p.trim() {
                    "swap" => xf.swap = true,
                    "mirror_x" => xf.mirror_x = true,
                    "mirror_y" => xf.mirror_y = true,
                    _ => {}
                }
            }
        }
        eprintln!("touch: {path} range_x={range_x:?} range_y={range_y:?} transform={xf:?}");
        let mut slots = [Slot::default(); MAX_SLOTS];
        for s in &mut slots {
            s.id = -1;
            s.reported_id = -1;
        }
        Ok(KindleTouch { fd, slots, cur: 0, range_x, range_y, screen, xf, st_down: false, saw_mt: false })
    }

    fn map(&self, x: i32, y: i32) -> (i32, i32) {
        let (rx, ry) = if self.xf.swap { (self.range_y, self.range_x) } else { (self.range_x, self.range_y) };
        let (mut x, mut y) = if self.xf.swap { (y, x) } else { (x, y) };
        let (w, h) = (self.screen.0 as i64 - 1, self.screen.1 as i64 - 1);
        x = ((x - rx.0) as i64 * w / (rx.1 - rx.0).max(1) as i64) as i32;
        y = ((y - ry.0) as i64 * h / (ry.1 - ry.0).max(1) as i64) as i32;
        if self.xf.mirror_x {
            x = w as i32 - x;
        }
        if self.xf.mirror_y {
            y = h as i32 - y;
        }
        (x, y)
    }

    fn sync(&mut self, out: &mut Vec<Touch>) {
        for i in 0..MAX_SLOTS {
            let s = self.slots[i];
            let (x, y) = self.map(s.x, s.y);
            if s.reported_id < 0 && s.id >= 0 {
                out.push(Touch { id: i as i32, phase: TouchPhase::Down, x, y });
            } else if s.reported_id >= 0 && s.id < 0 {
                out.push(Touch { id: i as i32, phase: TouchPhase::Up, x: s.rx, y: s.ry });
            } else if s.id >= 0 && (x != s.rx || y != s.ry) {
                out.push(Touch { id: i as i32, phase: TouchPhase::Move, x, y });
            } else {
                continue;
            }
            let s = &mut self.slots[i];
            s.reported_id = s.id;
            if s.id >= 0 {
                s.rx = x;
                s.ry = y;
            }
        }
    }

    fn handle(&mut self, ty: u16, code: u16, value: i32, out: &mut Vec<Touch>) {
        if std::env::var_os("INKPAD_TRACE").is_some() {
            eprintln!("ev type={ty} code={code:#x} value={value}");
        }
        match (ty, code) {
            (EV_ABS, ABS_MT_SLOT) => {
                self.saw_mt = true;
                self.cur = (value.max(0) as usize).min(MAX_SLOTS - 1);
            }
            (EV_ABS, ABS_MT_TRACKING_ID) => {
                self.saw_mt = true;
                self.slots[self.cur].id = if value < 0 { -1 } else { value };
            }
            (EV_ABS, ABS_MT_POSITION_X) => self.slots[self.cur].x = value,
            (EV_ABS, ABS_MT_POSITION_Y) => self.slots[self.cur].y = value,
            (EV_ABS, ABS_X) if !self.saw_mt => self.slots[0].x = value,
            (EV_ABS, ABS_Y) if !self.saw_mt => self.slots[0].y = value,
            (EV_KEY, BTN_TOUCH) if !self.saw_mt => {
                self.st_down = value != 0;
                self.slots[0].id = if self.st_down { 0 } else { -1 };
            }
            (EV_SYN, SYN_REPORT) => self.sync(out),
            _ => {}
        }
    }
}

impl Drop for KindleTouch {
    fn drop(&mut self) {
        // SAFETY: releasing the grab and closing our fd.
        unsafe {
            libc::ioctl(self.fd, EVIOCGRAB as _, 0usize);
            libc::close(self.fd);
        }
    }
}

impl Input for KindleTouch {
    fn poll(&mut self, timeout_ms: i32) -> io::Result<Vec<Event>> {
        let mut out: Vec<Touch> = Vec::new();
        let mut pfd = libc::pollfd { fd: self.fd, events: libc::POLLIN, revents: 0 };
        // SAFETY: one valid pollfd.
        let n = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };
        if n < 0 {
            let e = io::Error::last_os_error();
            return if e.kind() == io::ErrorKind::Interrupted { Ok(Vec::new()) } else { Err(e) };
        }
        if n == 0 {
            return Ok(Vec::new());
        }
        // 32-bit kernel input_event: timeval (2×i32), u16 type, u16 code, i32 value.
        let mut buf = [0u8; 16 * 64];
        loop {
            // SAFETY: reading into a local buffer of the given length.
            let r = unsafe { libc::read(self.fd, buf.as_mut_ptr() as *mut _, buf.len()) };
            if r <= 0 {
                break;
            }
            for ev in buf[..r as usize].chunks_exact(16) {
                let ty = u16::from_ne_bytes([ev[8], ev[9]]);
                let code = u16::from_ne_bytes([ev[10], ev[11]]);
                let value = i32::from_ne_bytes([ev[12], ev[13], ev[14], ev[15]]);
                self.handle(ty, code, value, &mut out);
            }
        }
        Ok(out.into_iter().map(Event::Touch).collect())
    }
}
