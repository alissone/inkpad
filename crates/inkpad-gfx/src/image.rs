//! Grayscale images: PNG and SVG loading, scaling, dithering, drawing.

use crate::Painter;
use resvg::{tiny_skia, usvg};

/// An 8-bit grayscale image with optional alpha.
#[derive(Clone, Debug)]
pub struct GrayImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    /// Per-pixel opacity (255 = opaque). `None` = fully opaque.
    pub alpha: Option<Vec<u8>>,
}

#[inline]
fn luma(r: u8, g: u8, b: u8) -> u8 {
    ((r as u32 * 299 + g as u32 * 587 + b as u32 * 114) / 1000) as u8
}

impl GrayImage {
    pub fn new(width: u32, height: u32, gray: u8) -> Self {
        GrayImage { width, height, pixels: vec![gray; (width * height) as usize], alpha: None }
    }

    /// Load a `.png` or `.svg` file. SVGs render at their natural size.
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let is_svg = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("svg"));
        if is_svg { Self::from_svg(&data, None) } else { Self::from_png(&data) }
    }

    pub fn from_png(data: &[u8]) -> Result<Self, String> {
        let mut dec = png::Decoder::new(std::io::Cursor::new(data));
        dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = dec.read_info().map_err(|e| e.to_string())?;
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
        let (w, h) = (info.width, info.height);
        let n = (w * h) as usize;
        let buf = &buf[..info.buffer_size()];
        let (pixels, alpha): (Vec<u8>, Option<Vec<u8>>) = match info.color_type {
            png::ColorType::Grayscale => (buf.to_vec(), None),
            png::ColorType::GrayscaleAlpha => (buf.chunks_exact(2).map(|c| c[0]).collect(), Some(buf.chunks_exact(2).map(|c| c[1]).collect())),
            png::ColorType::Rgb => (buf.chunks_exact(3).map(|c| luma(c[0], c[1], c[2])).collect(), None),
            png::ColorType::Rgba => (
                buf.chunks_exact(4).map(|c| luma(c[0], c[1], c[2])).collect(),
                Some(buf.chunks_exact(4).map(|c| c[3]).collect()),
            ),
            other => return Err(format!("unsupported PNG color type {other:?}")),
        };
        if pixels.len() != n {
            return Err("PNG size mismatch".into());
        }
        Ok(GrayImage { width: w, height: h, pixels, alpha })
    }

    /// Render an SVG. `size` = target (width, height) to fit inside while
    /// keeping the aspect ratio; `None` = the SVG's own size.
    pub fn from_svg(data: &[u8], size: Option<(u32, u32)>) -> Result<Self, String> {
        let tree = usvg::Tree::from_data(data, &usvg::Options::default()).map_err(|e| e.to_string())?;
        let s = tree.size();
        let (sw, sh) = (s.width(), s.height());
        let scale = match size {
            Some((w, h)) => (w as f32 / sw).min(h as f32 / sh),
            None => 1.0,
        };
        let (w, h) = (((sw * scale).round() as u32).max(1), ((sh * scale).round() as u32).max(1));
        let mut pm = tiny_skia::Pixmap::new(w, h).ok_or("bad SVG size")?;
        resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pm.as_mut());
        let mut pixels = Vec::with_capacity((w * h) as usize);
        let mut alpha = Vec::with_capacity((w * h) as usize);
        for c in pm.data().chunks_exact(4) {
            let a = c[3];
            // Un-premultiply.
            let un = |v: u8| if a == 0 { 0 } else { ((v as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8 };
            pixels.push(luma(un(c[0]), un(c[1]), un(c[2])));
            alpha.push(a);
        }
        Ok(GrayImage { width: w, height: h, pixels, alpha: Some(alpha) })
    }

    /// Scale to fit inside `max_w`×`max_h`, keeping aspect ratio (area
    /// averaging when shrinking, bilinear when growing).
    pub fn fit(&self, max_w: u32, max_h: u32) -> Self {
        let s = (max_w as f32 / self.width as f32).min(max_h as f32 / self.height as f32);
        let (w, h) = (((self.width as f32 * s).round() as u32).max(1), ((self.height as f32 * s).round() as u32).max(1));
        self.resize(w, h)
    }

    pub fn resize(&self, w: u32, h: u32) -> Self {
        if (w, h) == (self.width, self.height) {
            return self.clone();
        }
        let sample = |src: &[u8]| -> Vec<u8> {
            let mut out = vec![0u8; (w * h) as usize];
            let (sx, sy) = (self.width as f32 / w as f32, self.height as f32 / h as f32);
            for y in 0..h {
                for x in 0..w {
                    let v = if sx > 1.0 || sy > 1.0 {
                        // Box filter over the source area.
                        let (x0, y0) = ((x as f32 * sx) as u32, (y as f32 * sy) as u32);
                        let x1 = (((x + 1) as f32 * sx) as u32).clamp(x0 + 1, self.width);
                        let y1 = (((y + 1) as f32 * sy) as u32).clamp(y0 + 1, self.height);
                        let mut sum = 0u32;
                        for yy in y0..y1 {
                            for xx in x0..x1 {
                                sum += src[(yy * self.width + xx) as usize] as u32;
                            }
                        }
                        (sum / ((x1 - x0) * (y1 - y0))) as u8
                    } else {
                        let fx = ((x as f32 + 0.5) * sx - 0.5).max(0.0);
                        let fy = ((y as f32 + 0.5) * sy - 0.5).max(0.0);
                        let (x0, y0) = (fx as u32, fy as u32);
                        let (x1, y1) = ((x0 + 1).min(self.width - 1), (y0 + 1).min(self.height - 1));
                        let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
                        let p = |xx: u32, yy: u32| src[(yy * self.width + xx) as usize] as f32;
                        let top = p(x0, y0) * (1.0 - tx) + p(x1, y0) * tx;
                        let bot = p(x0, y1) * (1.0 - tx) + p(x1, y1) * tx;
                        (top * (1.0 - ty) + bot * ty + 0.5) as u8
                    };
                    out[(y * w + x) as usize] = v;
                }
            }
            out
        };
        GrayImage { width: w, height: h, pixels: sample(&self.pixels), alpha: self.alpha.as_deref().map(sample) }
    }

    /// Floyd–Steinberg dither to the panel's 16 gray levels. Makes photos and
    /// gradients look smooth instead of banded.
    pub fn dither16(&self) -> Self {
        let (w, h) = (self.width as usize, self.height as usize);
        let mut err: Vec<f32> = self.pixels.iter().map(|&p| p as f32).collect();
        let mut out = vec![0u8; w * h];
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                let old = err[i].clamp(0.0, 255.0);
                let q = ((old / 17.0).round() * 17.0).clamp(0.0, 255.0);
                out[i] = q as u8;
                let e = old - q;
                if x + 1 < w {
                    err[i + 1] += e * 7.0 / 16.0;
                }
                if y + 1 < h {
                    if x > 0 {
                        err[i + w - 1] += e * 3.0 / 16.0;
                    }
                    err[i + w] += e * 5.0 / 16.0;
                    if x + 1 < w {
                        err[i + w + 1] += e * 1.0 / 16.0;
                    }
                }
            }
        }
        GrayImage { width: self.width, height: self.height, pixels: out, alpha: self.alpha.clone() }
    }
}

impl<'a> Painter<'a> {
    /// Draw an image with its top-left at (x, y), honoring alpha.
    pub fn draw_image(&mut self, x: i32, y: i32, img: &GrayImage) {
        let w = img.width as usize;
        for row in 0..img.height as usize {
            for col in 0..w {
                let i = row * w + col;
                let a = img.alpha.as_ref().map(|a| a[i]).unwrap_or(255);
                if a == 0 {
                    continue;
                }
                self.blend(x + col as i32, y + row as i32, img.pixels[i], a as f32 / 255.0);
            }
        }
    }

    /// Draw only an image's alpha channel, in `gray` (for icons).
    pub fn draw_tinted(&mut self, x: i32, y: i32, img: &GrayImage, gray: u8) {
        let Some(alpha) = &img.alpha else {
            return self.draw_image(x, y, img);
        };
        self.mask(x, y, img.width as usize, img.height as usize, alpha, gray, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24"><rect x="4" y="4" width="16" height="16" fill="black"/></svg>"#;

    #[test]
    fn svg_renders_with_alpha() {
        let img = GrayImage::from_svg(SVG.as_bytes(), Some((48, 48))).unwrap();
        assert_eq!((img.width, img.height), (48, 48));
        let a = img.alpha.as_ref().unwrap();
        assert_eq!(a[0], 0, "corner transparent");
        assert_eq!(a[24 * 48 + 24], 255, "center opaque");
        assert_eq!(img.pixels[24 * 48 + 24], 0, "center black");
    }

    #[test]
    fn resize_and_dither() {
        let mut img = GrayImage::new(100, 10, 0);
        for (i, p) in img.pixels.iter_mut().enumerate() {
            *p = ((i % 100) * 255 / 99) as u8;
        }
        let small = img.fit(50, 50);
        assert_eq!((small.width, small.height), (50, 5));
        let d = img.dither16();
        assert!(d.pixels.iter().all(|&p| p % 17 == 0));
    }
}
