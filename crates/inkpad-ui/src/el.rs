//! The element tree an app's `view` returns, and the builders to make it.
//!
//! ```ignore
//! col(vec![
//!     text("Tasks").size(40).bold(),
//!     row(vec![icon("check", 28), text("Done").fill_w()]).gap(12).on_tap(Msg::Toggle),
//! ])
//! .padding(24)
//! ```

use inkpad_gfx::{FamilyId, GrayImage, Painter, Rect, TextStyle, Weight};
use std::rc::Rc;

/// How big an element wants to be along one axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Size {
    /// As big as its content.
    Auto,
    /// Exactly this many pixels.
    Px(u32),
    /// Share leftover space with other `Fill` siblings, by weight.
    Fill(u16),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Row,
    Col,
}

/// Cross-axis alignment of children, and text alignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
    /// Children take the full cross size.
    Stretch,
}

/// Main-axis distribution of children.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Justify {
    Start,
    Center,
    End,
    SpaceBetween,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Edges {
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
    pub left: u32,
}

impl Edges {
    pub const fn all(v: u32) -> Self {
        Edges { top: v, right: v, bottom: v, left: v }
    }
    pub const fn xy(x: u32, y: u32) -> Self {
        Edges { top: y, right: x, bottom: y, left: x }
    }
    pub fn h(&self) -> u32 {
        self.left + self.right
    }
    pub fn v(&self) -> u32 {
        self.top + self.bottom
    }
}

/// Custom drawing callback: paints inside the given rectangle.
pub type DrawFn = Rc<dyn Fn(&mut Painter, Rect)>;

pub enum Kind<M> {
    Empty,
    Text { text: String, style: TextStyle, max_lines: Option<usize>, align: Align },
    Image { img: Rc<GrayImage>, tint: Option<u8> },
    Rule { thickness: u32, gray: u8 },
    Flex { axis: Axis, children: Vec<El<M>>, gap: u32, cross: Align, justify: Justify },
    /// Children drawn on top of each other; later ones are on top and get
    /// taps first.
    Stack { children: Vec<El<M>> },
    /// A vertical list split into pages that fit the available height.
    Paged { children: Vec<El<M>>, page: usize, on_page: Rc<dyn Fn(usize) -> M>, gap: u32 },
    Canvas { draw: DrawFn, w: u32, h: u32 },
}

pub struct El<M> {
    pub kind: Kind<M>,
    pub width: Size,
    pub height: Size,
    pub padding: Edges,
    pub bg: Option<u8>,
    pub border: Option<(f32, u8)>,
    pub radius: f32,
    pub on_tap: Option<M>,
    pub on_hold: Option<M>,
    /// Swallow taps even without a handler (dialog backdrops).
    pub block: bool,
}

impl<M> El<M> {
    pub fn new(kind: Kind<M>) -> Self {
        El {
            kind,
            width: Size::Auto,
            height: Size::Auto,
            padding: Edges::default(),
            bg: None,
            border: None,
            radius: 0.0,
            on_tap: None,
            on_hold: None,
            block: false,
        }
    }

    // ---- box modifiers ----
    pub fn width(mut self, s: Size) -> Self {
        self.width = s;
        self
    }
    pub fn height(mut self, s: Size) -> Self {
        self.height = s;
        self
    }
    /// Fixed width in pixels.
    pub fn w(self, px: u32) -> Self {
        self.width(Size::Px(px))
    }
    /// Fixed height in pixels.
    pub fn h(self, px: u32) -> Self {
        self.height(Size::Px(px))
    }
    pub fn fill_w(self) -> Self {
        self.width(Size::Fill(1))
    }
    pub fn fill_h(self) -> Self {
        self.height(Size::Fill(1))
    }
    pub fn fill(self) -> Self {
        self.fill_w().fill_h()
    }
    pub fn padding(mut self, v: u32) -> Self {
        self.padding = Edges::all(v);
        self
    }
    pub fn pad(mut self, x: u32, y: u32) -> Self {
        self.padding = Edges::xy(x, y);
        self
    }
    pub fn pad_edges(mut self, e: Edges) -> Self {
        self.padding = e;
        self
    }
    pub fn bg(mut self, gray: u8) -> Self {
        self.bg = Some(gray);
        self
    }
    pub fn border(mut self, width: f32, gray: u8) -> Self {
        self.border = Some((width, gray));
        self
    }
    pub fn radius(mut self, r: f32) -> Self {
        self.radius = r;
        self
    }
    pub fn on_tap(mut self, msg: M) -> Self {
        self.on_tap = Some(msg);
        self
    }
    pub fn on_hold(mut self, msg: M) -> Self {
        self.on_hold = Some(msg);
        self
    }
    pub fn block_taps(mut self) -> Self {
        self.block = true;
        self
    }

    // ---- text modifiers (no-ops on other kinds) ----
    fn style_mut(&mut self) -> Option<&mut TextStyle> {
        match &mut self.kind {
            Kind::Text { style, .. } => Some(style),
            _ => None,
        }
    }
    /// Font size in pixels.
    pub fn size(mut self, px: u16) -> Self {
        if let Some(s) = self.style_mut() {
            s.size = px;
        }
        self
    }
    pub fn bold(mut self) -> Self {
        if let Some(s) = self.style_mut() {
            s.weight = Weight::Bold;
        }
        self
    }
    pub fn italic(mut self) -> Self {
        if let Some(s) = self.style_mut() {
            s.italic = true;
        }
        self
    }
    pub fn family(mut self, f: FamilyId) -> Self {
        if let Some(s) = self.style_mut() {
            s.family = f;
        }
        self
    }
    pub fn line_spacing(mut self, v: f32) -> Self {
        if let Some(s) = self.style_mut() {
            s.line_spacing = v;
        }
        self
    }
    /// Text or icon color (gray level).
    pub fn color(mut self, gray: u8) -> Self {
        match &mut self.kind {
            Kind::Text { style, .. } => style.color = gray,
            Kind::Image { tint, .. } => *tint = Some(gray),
            Kind::Rule { gray: g, .. } => *g = gray,
            _ => {}
        }
        self
    }
    pub fn max_lines(mut self, n: usize) -> Self {
        if let Kind::Text { max_lines, .. } = &mut self.kind {
            *max_lines = Some(n);
        }
        self
    }
    /// Text alignment (Start/Center/End).
    pub fn align_text(mut self, a: Align) -> Self {
        if let Kind::Text { align, .. } = &mut self.kind {
            *align = a;
        }
        self
    }
    pub fn center_text(self) -> Self {
        self.align_text(Align::Center)
    }

    // ---- flex modifiers ----
    pub fn gap(mut self, g: u32) -> Self {
        match &mut self.kind {
            Kind::Flex { gap, .. } | Kind::Paged { gap, .. } => *gap = g,
            _ => {}
        }
        self
    }
    /// Cross-axis alignment of children.
    pub fn align(mut self, a: Align) -> Self {
        if let Kind::Flex { cross, .. } = &mut self.kind {
            *cross = a;
        }
        self
    }
    pub fn justify(mut self, j: Justify) -> Self {
        if let Kind::Flex { justify, .. } = &mut self.kind {
            *justify = j;
        }
        self
    }

    /// Convert this tree's messages (used to embed an app's tree in the
    /// runtime's own).
    pub fn map<N: 'static>(self, f: &Rc<dyn Fn(M) -> N>) -> El<N>
    where
        M: 'static,
    {
        let kind = match self.kind {
            Kind::Empty => Kind::Empty,
            Kind::Text { text, style, max_lines, align } => Kind::Text { text, style, max_lines, align },
            Kind::Image { img, tint } => Kind::Image { img, tint },
            Kind::Rule { thickness, gray } => Kind::Rule { thickness, gray },
            Kind::Canvas { draw, w, h } => Kind::Canvas { draw, w, h },
            Kind::Flex { axis, children, gap, cross, justify } => {
                Kind::Flex { axis, children: children.into_iter().map(|c| c.map(f)).collect(), gap, cross, justify }
            }
            Kind::Stack { children } => Kind::Stack { children: children.into_iter().map(|c| c.map(f)).collect() },
            Kind::Paged { children, page, on_page, gap } => {
                let f2 = f.clone();
                Kind::Paged {
                    children: children.into_iter().map(|c| c.map(f)).collect(),
                    page,
                    on_page: Rc::new(move |p| f2(on_page(p))),
                    gap,
                }
            }
        };
        El {
            kind,
            width: self.width,
            height: self.height,
            padding: self.padding,
            bg: self.bg,
            border: self.border,
            radius: self.radius,
            on_tap: self.on_tap.map(|m| f(m)),
            on_hold: self.on_hold.map(|m| f(m)),
            block: self.block,
        }
    }
}

// ---- constructors ----

pub fn empty<M>() -> El<M> {
    El::new(Kind::Empty)
}

/// Flexible empty space (takes leftover room in a row or column).
pub fn spacer<M>() -> El<M> {
    El::new(Kind::Empty).fill()
}

pub fn text<M>(s: impl Into<String>) -> El<M> {
    El::new(Kind::Text { text: s.into(), style: TextStyle::new(22), max_lines: None, align: Align::Start })
}

pub fn row<M>(children: Vec<El<M>>) -> El<M> {
    El::new(Kind::Flex { axis: Axis::Row, children, gap: 0, cross: Align::Center, justify: Justify::Start })
}

pub fn col<M>(children: Vec<El<M>>) -> El<M> {
    El::new(Kind::Flex { axis: Axis::Col, children, gap: 0, cross: Align::Stretch, justify: Justify::Start })
}

pub fn stack<M>(children: Vec<El<M>>) -> El<M> {
    El::new(Kind::Stack { children })
}

/// A paged vertical list. `page` is owned by the app; `on_page` turns a
/// requested page number into a message. Swipe left/up or tap › for next.
pub fn paged<M>(children: Vec<El<M>>, page: usize, on_page: impl Fn(usize) -> M + 'static) -> El<M> {
    El::new(Kind::Paged { children, page, on_page: Rc::new(on_page), gap: 0 }).fill()
}

/// A thin line across the parent's cross axis.
pub fn divider<M>() -> El<M> {
    El::new(Kind::Rule { thickness: 1, gray: 190 })
}

pub fn rule<M>(thickness: u32, gray: u8) -> El<M> {
    El::new(Kind::Rule { thickness, gray })
}

/// A grayscale image (PNG/SVG loaded with [`crate::assets::image`]).
pub fn image<M>(img: Rc<GrayImage>) -> El<M> {
    El::new(Kind::Image { img, tint: None })
}

/// A bundled icon (see [`crate::icons::NAMES`]) at `size` pixels.
pub fn icon<M>(name: &str, size: u32) -> El<M> {
    match crate::icons::render(name, size) {
        Some(img) => El::new(Kind::Image { img, tint: Some(0) }),
        None => {
            eprintln!("unknown icon {name:?}");
            El::new(Kind::Empty).w(size).h(size)
        }
    }
}

/// Draw anything: charts, custom widgets. Gets the painter and its rect.
pub fn canvas<M>(w: u32, h: u32, draw: impl Fn(&mut Painter, Rect) + 'static) -> El<M> {
    El::new(Kind::Canvas { draw: Rc::new(draw), w, h })
}
