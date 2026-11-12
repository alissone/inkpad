//! Text: font families, glyph cache, word-wrapping layout with ellipsis.
//!
//! Family 0 is the bundled Atkinson Hyperlegible. More families can be added
//! at runtime from font files (e.g. the Kindle's system fonts or
//! `/mnt/us/fonts`); characters a family lacks fall back to family 0.

use crate::Painter;
use fontdue::{Font, FontSettings, Metrics};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

static REGULAR: &[u8] = include_bytes!("../fonts/AtkinsonHyperlegible-Regular.ttf");
static BOLD: &[u8] = include_bytes!("../fonts/AtkinsonHyperlegible-Bold.ttf");

pub type FamilyId = u8;
/// The bundled sans-serif family.
pub const DEFAULT_FAMILY: FamilyId = 0;

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
    pub italic: bool,
    pub family: FamilyId,
    /// Gray level, 0 = black.
    pub color: u8,
    /// Line height as a multiple of the font's natural line height.
    pub line_spacing: f32,
}

impl TextStyle {
    pub const fn new(size: u16) -> Self {
        TextStyle { size, weight: Weight::Regular, italic: false, family: DEFAULT_FAMILY, color: 0, line_spacing: 1.0 }
    }
    pub const fn bold(mut self) -> Self {
        self.weight = Weight::Bold;
        self
    }
    pub const fn italic(mut self) -> Self {
        self.italic = true;
        self
    }
    pub const fn family(mut self, f: FamilyId) -> Self {
        self.family = f;
        self
    }
    pub const fn color(mut self, gray: u8) -> Self {
        self.color = gray;
        self
    }
    pub const fn size(mut self, px: u16) -> Self {
        self.size = px;
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

struct Family {
    name: String,
    regular: Font,
    bold: Option<Font>,
    italic: Option<Font>,
    bold_italic: Option<Font>,
}

impl Family {
    fn pick(&self, weight: Weight, italic: bool) -> &Font {
        match (weight, italic) {
            (Weight::Bold, true) => self.bold_italic.as_ref().or(self.bold.as_ref()).unwrap_or(&self.regular),
            (Weight::Bold, false) => self.bold.as_ref().unwrap_or(&self.regular),
            (Weight::Regular, true) => self.italic.as_ref().unwrap_or(&self.regular),
            (Weight::Regular, false) => &self.regular,
        }
    }
}

/// Font families plus a glyph cache. Create once and share.
pub struct Fonts {
    families: Vec<Family>,
    cache: RefCell<HashMap<(FamilyId, Weight, bool, char, u16), Cached>>,
    /// Coverage gamma for text; below 1.0 makes text darker/heavier, which
    /// helps on e-ink.
    pub gamma: f32,
}

const ELLIPSIS: char = '…';

fn parse(bytes: Vec<u8>) -> Result<Font, String> {
    Font::from_bytes(bytes, FontSettings::default()).map_err(|e| e.to_string())
}

impl Fonts {
    pub fn new() -> Self {
        let regular = parse(REGULAR.to_vec()).expect("bundled font");
        let bold = parse(BOLD.to_vec()).expect("bundled font");
        Fonts {
            families: vec![Family { name: "Atkinson Hyperlegible".into(), regular, bold: Some(bold), italic: None, bold_italic: None }],
            cache: RefCell::new(HashMap::new()),
            gamma: 0.8,
        }
    }

    /// Add a family from font file bytes (TTF/OTF). Returns its id.
    pub fn add_family(
        &mut self,
        name: &str,
        regular: Vec<u8>,
        bold: Option<Vec<u8>>,
        italic: Option<Vec<u8>>,
        bold_italic: Option<Vec<u8>>,
    ) -> Result<FamilyId, String> {
        if self.families.len() >= FamilyId::MAX as usize {
            return Err("too many font families".into());
        }
        let opt = |b: Option<Vec<u8>>| b.map(parse).transpose();
        self.families.push(Family {
            name: name.to_string(),
            regular: parse(regular)?,
            bold: opt(bold)?,
            italic: opt(italic)?,
            bold_italic: opt(bold_italic)?,
        });
        Ok((self.families.len() - 1) as FamilyId)
    }

    /// Find `<stem>-Regular.ttf` (or `<stem>.ttf`), `-Bold`, `-Italic` and
    /// `-BoldItalic` (`.ttf` or `.otf`; `-` or `_`) in the first directory of
    /// `dirs` that has the regular face, and add them as family `name`.
    pub fn load_family(&mut self, name: &str, stem: &str, dirs: &[PathBuf]) -> Option<FamilyId> {
        if let Some(id) = self.family(name) {
            return Some(id);
        }
        let find = |dir: &Path, styles: &[&str]| -> Option<Vec<u8>> {
            for style in styles {
                for sep in ["-", "_", " "] {
                    for ext in ["ttf", "otf", "TTF", "OTF"] {
                        let file = if style.is_empty() { format!("{stem}.{ext}") } else { format!("{stem}{sep}{style}.{ext}") };
                        if let Ok(b) = std::fs::read(dir.join(file)) {
                            return Some(b);
                        }
                    }
                }
            }
            None
        };
        for dir in dirs {
            let Some(regular) = find(dir, &["Regular", ""]) else { continue };
            let bold = find(dir, &["Bold"]);
            let italic = find(dir, &["Italic"]);
            let bold_italic = find(dir, &["BoldItalic", "Bold-Italic", "Bold Italic"]);
            match self.add_family(name, regular, bold, italic, bold_italic) {
                Ok(id) => return Some(id),
                Err(e) => eprintln!("font {stem} in {}: {e}", dir.display()),
            }
        }
        None
    }

    /// Look up a family id by name.
    pub fn family(&self, name: &str) -> Option<FamilyId> {
        self.families.iter().position(|f| f.name.eq_ignore_ascii_case(name)).map(|i| i as FamilyId)
    }

    pub fn family_names(&self) -> Vec<&str> {
        self.families.iter().map(|f| f.name.as_str()).collect()
    }

    /// The font used for `ch` in `style`, falling back to the default family
    /// when the chosen one lacks the glyph. Returns the family actually used.
    fn font_for(&self, style: &TextStyle, ch: char) -> (FamilyId, &Font) {
        let fam = self.families.get(style.family as usize).unwrap_or(&self.families[0]);
        let f = fam.pick(style.weight, style.italic);
        if style.family == DEFAULT_FAMILY || ch.is_whitespace() || f.lookup_glyph_index(ch) != 0 {
            return (style.family.min((self.families.len() - 1) as u8), f);
        }
        (DEFAULT_FAMILY, self.families[0].pick(style.weight, style.italic))
    }

    /// Width of a single line of text (no wrapping).
    pub fn measure_line(&self, text: &str, style: &TextStyle) -> f32 {
        self.place(text, style).width
    }

    fn line_metrics(&self, style: &TextStyle) -> (f32, f32) {
        let px = style.size as f32;
        let fam = self.families.get(style.family as usize).unwrap_or(&self.families[0]);
        let m = fam.pick(style.weight, style.italic).horizontal_line_metrics(px);
        let (ascent, natural) = m.map(|m| (m.ascent, m.new_line_size)).unwrap_or((px * 0.8, px * 1.2));
        (ascent.ceil(), (natural * style.line_spacing).ceil())
    }

    fn place(&self, text: &str, style: &TextStyle) -> Line {
        let px = style.size as f32;
        let mut glyphs = Vec::with_capacity(text.len());
        let mut x = 0.0;
        let mut prev: Option<(char, FamilyId)> = None;
        for ch in text.chars() {
            let (fid, font) = self.font_for(style, ch);
            if let Some((p, pf)) = prev {
                if pf == fid {
                    x += font.horizontal_kern(p, ch, px).unwrap_or(0.0);
                }
            }
            glyphs.push(PlacedGlyph { ch, x });
            x += font.metrics(ch, px).advance_width;
            prev = Some((ch, fid));
        }
        Line { glyphs, width: x }
    }

    /// Lay out `text`. With `max_width`, words wrap (long words break
    /// anywhere). With `max_lines`, the last line ends in "…" when cut.
    pub fn layout(&self, text: &str, style: TextStyle, max_width: Option<u32>, max_lines: Option<usize>) -> TextLayout {
        let (ascent, line_height) = self.line_metrics(&style);
        let limit = max_width.map(|w| w as f32).unwrap_or(f32::INFINITY);
        let max_lines = max_lines.unwrap_or(usize::MAX).max(1);

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

        let truncated = lines.len() > max_lines;
        if truncated {
            lines.truncate(max_lines);
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
                let (fid, font) = self.font_for(&style, g.ch);
                let key = (fid, style.weight, style.italic, g.ch, style.size);
                let c = cache.entry(key).or_insert_with(|| {
                    let (metrics, bitmap) = font.rasterize(g.ch, style.size as f32);
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

    #[test]
    fn add_family_and_lookup() {
        let mut f = Fonts::new();
        let id = f.add_family("Copy", BOLD.to_vec(), None, None, None).unwrap();
        assert_eq!(id, 1);
        assert_eq!(f.family("copy"), Some(1));
        // Missing faces fall back to the family's regular face.
        let w1 = f.measure_line("Hello", &TextStyle::new(20).family(id));
        let w2 = f.measure_line("Hello", &TextStyle::new(20).bold());
        assert!((w1 - w2).abs() < 0.01);
        assert!(f.load_family("Nope", "DoesNotExist", &[PathBuf::from(".")]).is_none());
    }
}
