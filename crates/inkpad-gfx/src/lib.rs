//! Drawing on an 8-bit grayscale [`Frame`] (0 = black, 255 = white):
//! rectangles, rounded rectangles, circles and lines with anti-aliased edges,
//! plus text (see [`text`]).

pub mod text;

pub use inkpad_hal::{Frame, Rect};
pub use text::{Fonts, TextLayout, TextStyle, Weight};

pub const BLACK: u8 = 0;
pub const WHITE: u8 = 255;

/// Draws into a frame, restricted to a clip rectangle.
pub struct Painter<'a> {
    frame: &'a mut Frame,
    clip: (i32, i32, i32, i32), // x0, y0, x1, y1 (exclusive)
}

impl<'a> Painter<'a> {
    pub fn new(frame: &'a mut Frame) -> Self {
        let clip = (0, 0, frame.width as i32, frame.height as i32);
        Painter { frame, clip }
    }

    /// Restrict drawing to `r` (intersected with the current clip).
    pub fn set_clip(&mut self, r: Rect) {
        let full = (0, 0, self.frame.width as i32, self.frame.height as i32);
        self.clip = (
            (r.x as i32).max(full.0),
            (r.y as i32).max(full.1),
            ((r.x + r.w) as i32).min(full.2),
            ((r.y + r.h) as i32).min(full.3),
        );
    }

    pub fn reset_clip(&mut self) {
        self.clip = (0, 0, self.frame.width as i32, self.frame.height as i32);
    }

    pub fn size(&self) -> (u32, u32) {
        (self.frame.width, self.frame.height)
    }

    #[inline]
    fn in_clip(&self, x: i32, y: i32) -> bool {
        x >= self.clip.0 && y >= self.clip.1 && x < self.clip.2 && y < self.clip.3
    }

    /// Set one pixel.
    #[inline]
    pub fn put(&mut self, x: i32, y: i32, gray: u8) {
        if self.in_clip(x, y) {
            self.frame.pixels[(y as u32 * self.frame.width + x as u32) as usize] = gray;
        }
    }

    /// Blend `gray` over a pixel with coverage `a` (0..=1).
    #[inline]
    pub fn blend(&mut self, x: i32, y: i32, gray: u8, a: f32) {
        if a <= 0.0 || !self.in_clip(x, y) {
            return;
        }
        let i = (y as u32 * self.frame.width + x as u32) as usize;
        if a >= 1.0 {
            self.frame.pixels[i] = gray;
        } else {
            let d = self.frame.pixels[i] as f32;
            self.frame.pixels[i] = (d + (gray as f32 - d) * a + 0.5) as u8;
        }
    }

    fn clip_rect(&self, r: Rect) -> Option<(i32, i32, i32, i32)> {
        let x0 = (r.x as i32).max(self.clip.0);
        let y0 = (r.y as i32).max(self.clip.1);
        let x1 = (r.x as i32 + r.w as i32).min(self.clip.2);
        let y1 = (r.y as i32 + r.h as i32).min(self.clip.3);
        (x1 > x0 && y1 > y0).then_some((x0, y0, x1, y1))
    }

    pub fn fill_rect(&mut self, r: Rect, gray: u8) {
        let Some((x0, y0, x1, y1)) = self.clip_rect(r) else { return };
        let w = self.frame.width as i32;
        for y in y0..y1 {
            let row = (y * w) as usize;
            self.frame.pixels[row + x0 as usize..row + x1 as usize].fill(gray);
        }
    }

    pub fn stroke_rect(&mut self, r: Rect, t: u32, gray: u8) {
        let t = t.min(r.w / 2 + 1).min(r.h / 2 + 1);
        self.fill_rect(Rect::new(r.x, r.y, r.w, t), gray);
        self.fill_rect(Rect::new(r.x, r.y + r.h.saturating_sub(t), r.w, t), gray);
        self.fill_rect(Rect::new(r.x, r.y, t, r.h), gray);
        self.fill_rect(Rect::new(r.x + r.w.saturating_sub(t), r.y, t, r.h), gray);
    }

    /// Invert a region (pressed-state feedback).
    pub fn invert_rect(&mut self, r: Rect) {
        let Some((x0, y0, x1, y1)) = self.clip_rect(r) else { return };
        let w = self.frame.width as i32;
        for y in y0..y1 {
            let row = (y * w) as usize;
            for p in &mut self.frame.pixels[row + x0 as usize..row + x1 as usize] {
                *p = !*p;
            }
        }
    }

    /// Rounded rectangle, filled (`stroke = None`) or outlined with width `stroke`.
    /// Edges are anti-aliased using a signed-distance function.
    pub fn round_rect(&mut self, r: Rect, radius: f32, gray: u8, stroke: Option<f32>) {
        let Some((x0, y0, x1, y1)) = self.clip_rect(r) else { return };
        let hw = r.w as f32 / 2.0;
        let hh = r.h as f32 / 2.0;
        let radius = radius.min(hw).min(hh).max(0.0);
        let cx = r.x as f32 + hw;
        let cy = r.y as f32 + hh;
        // Pixels well inside the straight parts are fully covered; only
        // compute the SDF near the edges.
        let inner = stroke.map(|s| s + 1.0).unwrap_or(f32::INFINITY);
        for y in y0..y1 {
            for x in x0..x1 {
                let px = x as f32 + 0.5 - cx;
                let py = y as f32 + 0.5 - cy;
                let qx = px.abs() - (hw - radius);
                let qy = py.abs() - (hh - radius);
                let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
                let d = outside + qx.max(qy).min(0.0) - radius;
                if d < -inner {
                    continue; // inside the hole of an outline
                }
                let mut a = (0.5 - d).clamp(0.0, 1.0);
                if let Some(s) = stroke {
                    a -= (0.5 - (d + s)).clamp(0.0, 1.0);
                }
                self.blend(x, y, gray, a);
            }
        }
    }

    pub fn circle(&mut self, cx: i32, cy: i32, radius: u32, gray: u8, stroke: Option<f32>) {
        let r = Rect::new(
            (cx - radius as i32).max(0) as u32,
            (cy - radius as i32).max(0) as u32,
            radius * 2,
            radius * 2,
        );
        self.round_rect(r, radius as f32, gray, stroke);
    }

    /// A straight line of thickness `t` with anti-aliased edges.
    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, t: f32, gray: u8) {
        let minx = (x0.min(x1) - t).floor() as i32;
        let maxx = (x0.max(x1) + t).ceil() as i32;
        let miny = (y0.min(y1) - t).floor() as i32;
        let maxy = (y0.max(y1) + t).ceil() as i32;
        let (dx, dy) = (x1 - x0, y1 - y0);
        let len2 = (dx * dx + dy * dy).max(1e-6);
        let half = t / 2.0;
        for y in miny.max(self.clip.1)..maxy.min(self.clip.3) {
            for x in minx.max(self.clip.0)..maxx.min(self.clip.2) {
                let (px, py) = (x as f32 + 0.5 - x0, y as f32 + 0.5 - y0);
                let h = ((px * dx + py * dy) / len2).clamp(0.0, 1.0);
                let (ex, ey) = (px - dx * h, py - dy * h);
                let d = (ex * ex + ey * ey).sqrt() - half;
                self.blend(x, y, gray, (0.5 - d).clamp(0.0, 1.0));
            }
        }
    }

    /// Draw a 1-byte-per-pixel coverage mask (e.g. a glyph) in `gray`.
    pub fn mask(&mut self, x: i32, y: i32, w: usize, h: usize, cov: &[u8], gray: u8, gamma: f32) {
        for row in 0..h {
            for col in 0..w {
                let c = cov[row * w + col];
                if c == 0 {
                    continue;
                }
                let a = (c as f32 / 255.0).powf(gamma);
                self.blend(x + col as i32, y + row as i32, gray, a);
            }
        }
    }

    /// Copy a grayscale image (row-major, `w`×`h`).
    pub fn image(&mut self, x: i32, y: i32, w: usize, h: usize, px: &[u8]) {
        for row in 0..h {
            for col in 0..w {
                self.put(x + col as i32, y + row as i32, px[row * w + col]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_respects_clip() {
        let mut f = Frame::new(10, 10);
        let mut p = Painter::new(&mut f);
        p.set_clip(Rect::new(2, 2, 3, 3));
        p.fill_rect(Rect::new(0, 0, 10, 10), BLACK);
        let black = f.pixels.iter().filter(|&&v| v == 0).count();
        assert_eq!(black, 9);
    }

    #[test]
    fn round_rect_corners_are_soft() {
        let mut f = Frame::new(40, 40);
        Painter::new(&mut f).round_rect(Rect::new(0, 0, 40, 40), 10.0, BLACK, None);
        assert_eq!(f.pixels[0], 255, "corner pixel stays white");
        assert_eq!(f.pixels[20 * 40 + 20], 0, "center is black");
        let edge = f.pixels[(0 * 40 + 3) as usize];
        assert!(edge > 0, "pixel near the curve is not solid black: {edge}");
    }

    #[test]
    fn outline_has_hole() {
        let mut f = Frame::new(40, 40);
        Painter::new(&mut f).round_rect(Rect::new(0, 0, 40, 40), 4.0, BLACK, Some(2.0));
        assert_eq!(f.pixels[20 * 40 + 20], 255);
        assert_eq!(f.pixels[20 * 40], 0);
    }

    #[test]
    fn invert() {
        let mut f = Frame::new(2, 1);
        Painter::new(&mut f).invert_rect(Rect::new(0, 0, 1, 1));
        assert_eq!(f.pixels, vec![0, 255]);
    }
}
