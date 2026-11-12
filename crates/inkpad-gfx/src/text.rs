//! Text: font loading, glyph cache, word-wrapping layout with ellipsis.

use crate::Painter;
use fontdue::{Font, FontSettings, Metrics};
use std::cell::RefCell;
use std::collections::HashMap;

static REGULAR: &[u8] = include_bytes!("../fonts/AtkinsonHyperlegible-Regular.ttf");
static BOLD: &[u8] = include_bytes!("../fonts/AtkinsonHyperlegible-Bold.ttf");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Weight {
    #[default]
    Regular,
    Bold,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    /// Font size in pixels.
    pub size: u16,
    pub weight: Weight,
    /// Gray level, 0 = black.
    pub color: u8,
    /// Line height as a multiple of the font's natural line height.
    pub line_spacing: f32,
}

impl TextStyle {
    pub const fn new(size: u16) -> Self {
        TextStyle { size, weight: Weight::Regular, color: 0, line_spacing: 1.0 }
    }
    pub const fn bold(mut self) -> Self {
        self.weight = Weight::Bold;
        self
    }
    pub const fn color(mut self, gray: u8) -> Self {
        self.color = gray;
        self
    }
    pub const fn spacing(mut self, s: f32) -> Self {
        self.line_spacing = s;
        self
    }
}

impl Default for TextStyle {
    fn default() -> Self {
        TextStyle::new(20)
    }
}

#[derive(Clone, Debug)]
pub struct PlacedGlyph {
    pub ch: char,
    pub x: f32,
}

#[derive(Clone, Debug)]
pub struct Line {
    pub glyphs: Vec<PlacedGlyph>,
    pub width: f32,
}

/// The result of laying out a string. Draw it with [`Fonts::draw`].
#[derive(Clone, Debug)]
pub struct TextLayout {
    pub style: TextStyle,
    pub lines: Vec<Line>,
    /// Distance from the top of a line to its baseline.
    pub ascent: f32,
    pub line_height: f32,
    pub width: u32,
    pub height: u32,
    /// True if text was cut short (max_lines) and ends with "…".
    pub truncated: bool,
}

struct Cached {
    metrics: Metrics,
    bitmap: Vec<u8>,
}

/// The font set plus a glyph cache. Create once and share.
pub struct Fonts {
    regular: Font,
    bold: Font,
    cache: RefCell<HashMap<(Weight, char, u16), Cached>>,
    /// Coverage gamma for text; below 1.0 makes text darker/heavier, which
    /// helps on e-ink.
    pub gamma: f32,
}

const ELLIPSIS: char = '…';

impl Fonts {
    pub fn new() -> Self {
        let load = |b: &'static [u8]| Font::from_bytes(b, FontSettings::default()).expect("bundled font");
        Fonts { regular: load(REGULAR), bold: load(BOLD), cache: RefCell::new(HashMap::new()), gamma: 0.8 }
    }

    fn font(&self, w: Weight) -> &Font {
        match w {
            Weight::Regular => &self.regular,
            Weight::Bold => &self.bold,
        }
    }

    fn advance(&self, style: &TextStyle, prev: Option<char>, ch: char) -> f32 {
        let f = self.font(style.weight);
        let px = style.size as f32;
        let kern = prev.and_then(|p| f.horizontal_kern(p, ch, px)).unwrap_or(0.0);
        f.metrics(ch, px).advance_width + kern
    }

    /// Width of a single line of text (no wrapping).
    pub fn measure_line(&self, text: &str, style: &TextStyle) -> f32 {
        let mut prev = None;
        let mut w = 0.0;
        for ch in text.chars() {
            w += self.advance(style, prev, ch);
            prev = Some(ch);
        }
        w
    }

    fn line_metrics(&self, style: &TextStyle) -> (f32, f32) {
        let px = style.size as f32;
        let m = self.font(style.weight).horizontal_line_metrics(px);
        let (ascent, natural) = m.map(|m| (m.ascent, m.new_line_size)).unwrap_or((px * 0.8, px * 1.2));
        (ascent.ceil(), (natural * style.line_spacing).ceil())
    }

    fn place(&self, text: &str, style: &TextStyle) -> Line {
        let mut glyphs = Vec::with_capacity(text.len());
        let font = self.font(style.weight);
        let px = style.size as f32;
        let mut x = 0.0;
        let mut prev = None;
        for ch in text.chars() {
            if let Some(p) = prev {
                x += font.horizontal_kern(p, ch, px).unwrap_or(0.0);
            }
            glyphs.push(PlacedGlyph { ch, x });
            x += font.metrics(ch, px).advance_width;
            prev = Some(ch);
        }
        Line { glyphs, width: x }
    }

    /// Lay out `text`. With `max_width`, words wrap (long words break
    /// anywhere). With `max_lines`, the last line ends in "…" when cut.
    pub fn layout(&self, text: &str, style: TextStyle, max_width: Option<u32>, max_lines: Option<usize>) -> TextLayout {
        let (ascent, line_height) = self.line_metrics(&style);
        let limit = max_width.map(|w| w as f32).unwrap_or(f32::INFINITY);
        let max_lines = max_lines.unwrap_or(usize::MAX).max(1);

        // Break into line strings first.
        let mut lines: Vec<String> = Vec::new();
        'outer: for para in text.split('\n') {
            let mut cur = String::new();
            for word in split_keep_spaces(para) {
                let candidate = format!("{cur}{word}");
                if self.measure_line(candidate.trim_end(), &style) <= limit || cur.is_empty() && word.trim().is_empty() {
                    cur = candidate;
                    continue;
                }
                if !cur.is_empty() {
                    lines.push(cur.trim_end().to_string());
                    if lines.len() > max_lines {
                        break 'outer;
                    }
                }
                cur = String::new();
                let word = word.trim_start();
                // A word wider than the line: break it by characters.
                if self.measure_line(word.trim_end(), &style) > limit {
                    for ch in word.chars() {
                        let c2 = format!("{cur}{ch}");
                        if !cur.is_empty() && self.measure_line(c2.trim_end(), &style) > limit {
                            lines.push(std::mem::take(&mut cur));
                            if lines.len() > max_lines {
                                break 'outer;
                            }
                            cur.push(ch);
                        } else {
                            cur = c2;
                        }
                    }
                } else {
                    cur = word.to_string();
                }
            }
            lines.push(cur.trim_end().to_string());
            if lines.len() > max_lines {
                break;
            }
        }

        let mut truncated = false;
        if lines.len() > max_lines {
            lines.truncate(max_lines);
            truncated = true;
        }
        if truncated {
            let last = lines.last_mut().unwrap();
            let ell_w = self.measure_line(&ELLIPSIS.to_string(), &style);
            while !last.is_empty() && self.measure_line(last, &style) + ell_w > limit {
                last.pop();
            }
            let trimmed = last.trim_end().to_string();
            *last = format!("{trimmed}{ELLIPSIS}");
        }

        let lines: Vec<Line> = lines.iter().map(|s| self.place(s, &style)).collect();
        let width = lines.iter().map(|l| l.width).fold(0.0, f32::max).ceil() as u32;
        let height = (line_height * lines.len() as f32).ceil() as u32;
        TextLayout { style, lines, ascent, line_height, width, height, truncated }
    }

    /// Lay out a single line, cutting it with "…" to fit `max_width`.
    pub fn layout_line(&self, text: &str, style: TextStyle, max_width: u32) -> TextLayout {
        let flat: String = text.chars().map(|c| if c == '\n' { ' ' } else { c }).collect();
        self.layout(&flat, style, Some(max_width), Some(1))
    }

    /// Draw a layout with its top-left corner at (x, y). `align` shifts each
    /// line within `box_width`: 0.0 = left, 0.5 = center, 1.0 = right.
    pub fn draw_aligned(&self, p: &mut Painter, layout: &TextLayout, x: i32, y: i32, box_width: u32, align: f32) {
        let style = layout.style;
        let mut cache = self.cache.borrow_mut();
        for (i, line) in layout.lines.iter().enumerate() {
            let baseline = y as f32 + layout.ascent + i as f32 * layout.line_height;
            let shift = ((box_width as f32 - line.width) * align).max(0.0);
            for g in &line.glyphs {
                if g.ch.is_whitespace() {
                    continue;
                }
                let key = (style.weight, g.ch, style.size);
                let c = cache.entry(key).or_insert_with(|| {
                    let (metrics, bitmap) = self.font(style.weight).rasterize(g.ch, style.size as f32);
                    Cached { metrics, bitmap }
                });
                let gx = (x as f32 + shift + g.x).round() as i32 + c.metrics.xmin;
                let gy = baseline.round() as i32 - c.metrics.height as i32 - c.metrics.ymin;
                p.mask(gx, gy, c.metrics.width, c.metrics.height, &c.bitmap, style.color, self.gamma);
            }
        }
    }

    pub fn draw(&self, p: &mut Painter, layout: &TextLayout, x: i32, y: i32) {
        self.draw_aligned(p, layout, x, y, layout.width, 0.0);
    }
}

impl Default for Fonts {
    fn default() -> Self {
        Fonts::new()
    }
}

/// Split into words, each keeping its trailing spaces ("a b" -> ["a ", "b"]).
fn split_keep_spaces(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut in_space = false;
    for (i, ch) in s.char_indices() {
        if ch == ' ' {
            in_space = true;
        } else if in_space {
            out.push(&s[start..i]);
            start = i;
            in_space = false;
        }
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_words() {
        assert_eq!(split_keep_spaces("a bb  c"), vec!["a ", "bb  ", "c"]);
        assert_eq!(split_keep_spaces(""), Vec::<&str>::new());
    }

    #[test]
    fn wraps_within_width() {
        let f = Fonts::new();
        let style = TextStyle::new(20);
        let l = f.layout("the quick brown fox jumps over the lazy dog", style, Some(120), None);
        assert!(l.lines.len() > 1);
        for line in &l.lines {
            assert!(line.width <= 120.5, "line too wide: {}", line.width);
        }
        let joined: Vec<String> = l.lines.iter().map(|ln| ln.glyphs.iter().map(|g| g.ch).collect()).collect();
        assert_eq!(joined.join(" "), "the quick brown fox jumps over the lazy dog");
    }

    #[test]
    fn ellipsis_when_truncated() {
        let f = Fonts::new();
        let l = f.layout("one two three four five six seven eight", TextStyle::new(20), Some(100), Some(2));
        assert_eq!(l.lines.len(), 2);
        assert!(l.truncated);
        let last: String = l.lines[1].glyphs.iter().map(|g| g.ch).collect();
        assert!(last.ends_with('…'), "{last}");
        assert!(l.lines[1].width <= 100.5);
    }

    #[test]
    fn breaks_long_words_and_keeps_newlines() {
        let f = Fonts::new();
        let l = f.layout("Supercalifragilistic\nok", TextStyle::new(24), Some(80), None);
        assert!(l.lines.len() >= 3);
        let last: String = l.lines.last().unwrap().glyphs.iter().map(|g| g.ch).collect();
        assert_eq!(last, "ok");
    }

    #[test]
    fn accents_render() {
        let f = Fonts::new();
        let mut frame = crate::Frame::new(200, 40);
        let l = f.layout("Ação já", TextStyle::new(24), None, None);
        f.draw(&mut Painter::new(&mut frame), &l, 0, 0);
        assert!(frame.pixels.iter().any(|&p| p < 128));
    }
}
