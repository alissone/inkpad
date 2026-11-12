//! Platform layer: an e-ink display you can push regions to, and touch input.
//!
//! Apps draw into their own 8-bit gray buffer (0 = black, 255 = white) and hand
//! dirty rectangles to [`Display::present`]. Backends: `kindle` (Linux mxcfb +
//! evdev) and, later, a desktop simulator.

use std::io;

#[cfg(target_os = "linux")]
pub mod kindle;

/// A rectangle in screen pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    pub const fn new(x: u32, y: u32, w: u32, h: u32) -> Self {
        Rect { x, y, w, h }
    }
    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x as i32
            && py >= self.y as i32
            && px < (self.x + self.w) as i32
            && py < (self.y + self.h) as i32
    }
    /// Clip to a `width`×`height` screen; `None` if nothing is left.
    pub fn clip(&self, width: u32, height: u32) -> Option<Rect> {
        let x0 = self.x.min(width);
        let y0 = self.y.min(height);
        let x1 = (self.x.saturating_add(self.w)).min(width);
        let y1 = (self.y.saturating_add(self.h)).min(height);
        (x1 > x0 && y1 > y0).then(|| Rect::new(x0, y0, x1 - x0, y1 - y0))
    }
    pub fn union(&self, o: &Rect) -> Rect {
        let x0 = self.x.min(o.x);
        let y0 = self.y.min(o.y);
        let x1 = (self.x + self.w).max(o.x + o.w);
        let y1 = (self.y + self.h).max(o.y + o.h);
        Rect::new(x0, y0, x1 - x0, y1 - y0)
    }
}

/// How the e-ink controller should repaint a region.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Waveform {
    /// Let the driver pick.
    Auto,
    /// Fast, black/white only. Tap feedback, pen strokes.
    Du,
    /// Full 16-level gray, highest quality. Content.
    Gc16,
    /// 16-level gray with less flashing; good for text on white.
    Gl16,
}

/// Grayscale frame the app draws into.
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Frame {
    pub fn new(width: u32, height: u32) -> Self {
        Frame { width, height, pixels: vec![255; (width * height) as usize] }
    }
    pub fn fill(&mut self, r: Rect, gray: u8) {
        let Some(r) = r.clip(self.width, self.height) else { return };
        for y in r.y..r.y + r.h {
            let row = (y * self.width) as usize;
            self.pixels[row + r.x as usize..row + (r.x + r.w) as usize].fill(gray);
        }
    }
    pub fn set(&mut self, x: i32, y: i32, gray: u8) {
        if x >= 0 && y >= 0 && (x as u32) < self.width && (y as u32) < self.height {
            self.pixels[(y as u32 * self.width + x as u32) as usize] = gray;
        }
    }
    pub fn full(&self) -> Rect {
        Rect::new(0, 0, self.width, self.height)
    }
}

pub trait Display {
    /// Logical (portrait) size in pixels.
    fn size(&self) -> (u32, u32);
    /// Copy `region` of `frame` to the screen and refresh it. `flash` forces a
    /// full (flashing) update that clears ghosting.
    fn present(&mut self, frame: &Frame, region: Rect, wf: Waveform, flash: bool) -> io::Result<()>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TouchPhase {
    Down,
    Move,
    Up,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Touch {
    /// Finger id (multitouch slot).
    pub id: i32,
    pub phase: TouchPhase,
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Touch(Touch),
    /// The platform wants the app to close (window closed, system request).
    Quit,
}

pub trait Input {
    /// Wait up to `timeout_ms` (-1 = forever) and return any events.
    fn poll(&mut self, timeout_ms: i32) -> io::Result<Vec<Event>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_and_union() {
        assert_eq!(Rect::new(590, 790, 20, 20).clip(600, 800), Some(Rect::new(590, 790, 10, 10)));
        assert_eq!(Rect::new(600, 0, 5, 5).clip(600, 800), None);
        assert_eq!(Rect::new(0, 0, 2, 2).union(&Rect::new(5, 5, 1, 1)), Rect::new(0, 0, 6, 6));
        assert!(Rect::new(10, 10, 5, 5).contains(14, 14));
        assert!(!Rect::new(10, 10, 5, 5).contains(15, 10));
    }

    #[test]
    fn frame_fill() {
        let mut f = Frame::new(4, 3);
        f.fill(Rect::new(1, 1, 10, 1), 0);
        assert_eq!(f.pixels, vec![255, 255, 255, 255, 255, 0, 0, 0, 255, 255, 255, 255]);
    }
}
