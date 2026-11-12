//! Layout: turns an element tree into draw operations and tap areas.
//!
//! Model: every element gets a rectangle from its parent. Rows and columns
//! measure non-`Fill` children first, then split the leftover space between
//! `Fill` children by weight (like Flutter's `Expanded`).

use crate::el::{Align, Axis, El, Justify, Kind, Size};
use inkpad_gfx::{Fonts, GrayImage, Painter, Rect, TextLayout, TextStyle};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub const PAGER_H: u32 = 56;

pub enum Op {
    Box { rect: Rect, bg: Option<u8>, border: Option<(f32, u8)>, radius: f32 },
    Text { layout: Rc<TextLayout>, x: i32, y: i32, w: u32, align: f32 },
    Image { img: Rc<GrayImage>, x: i32, y: i32, tint: Option<u8> },
    Rule { rect: Rect, gray: u8 },
    Canvas { draw: crate::el::DrawFn, rect: Rect },
}

pub struct Hit<M> {
    pub rect: Rect,
    pub tap: Option<M>,
    pub hold: Option<M>,
}

pub struct Swipe<M> {
    pub rect: Rect,
    pub prev: Option<M>,
    pub next: Option<M>,
}

pub struct Out<M> {
    pub ops: Vec<Op>,
    pub hits: Vec<Hit<M>>,
    pub swipes: Vec<Swipe<M>>,
}

impl<M> Default for Out<M> {
    fn default() -> Self {
        Out { ops: Vec::new(), hits: Vec::new(), swipes: Vec::new() }
    }
}

type TextKey = (String, u16, u8, bool, u8, u32, u32, usize);

pub struct Layouter<'a> {
    pub fonts: &'a Fonts,
    texts: RefCell<HashMap<TextKey, Rc<TextLayout>>>,
}

fn inset(r: Rect, e: &crate::el::Edges) -> Rect {
    Rect::new(r.x + e.left.min(r.w), r.y + e.top.min(r.h), r.w.saturating_sub(e.h()), r.h.saturating_sub(e.v()))
}

impl<'a> Layouter<'a> {
    pub fn new(fonts: &'a Fonts) -> Self {
        Layouter { fonts, texts: RefCell::new(HashMap::new()) }
    }

    fn text(&self, s: &str, style: TextStyle, max_w: u32, max_lines: Option<usize>) -> Rc<TextLayout> {
        let key = (
            s.to_string(),
            style.size,
            style.weight as u8,
            style.italic,
            style.family,
            (style.line_spacing * 100.0) as u32 ^ ((style.color as u32) << 16),
            max_w,
            max_lines.unwrap_or(0),
        );
        if let Some(l) = self.texts.borrow().get(&key) {
            return l.clone();
        }
        let l = Rc::new(self.fonts.layout(s, style, Some(max_w.max(1)), max_lines));
        self.texts.borrow_mut().insert(key, l.clone());
        l
    }

    /// Outer size an element wants within `aw`×`ah`.
    pub fn measure<M>(&self, el: &El<M>, aw: u32, ah: u32) -> (u32, u32) {
        let fw = if let Size::Px(p) = el.width { Some(p) } else { None };
        let fh = if let Size::Px(p) = el.height { Some(p) } else { None };
        let iw = fw.unwrap_or(aw).saturating_sub(el.padding.h());
        let ih = fh.unwrap_or(ah).saturating_sub(el.padding.v());
        let (cw, ch) = self.content_size(el, iw, ih);
        let w = fw.unwrap_or_else(|| (cw + el.padding.h()).min(aw));
        let h = fh.unwrap_or_else(|| (ch + el.padding.v()).min(ah));
        (w, h)
    }

    fn content_size<M>(&self, el: &El<M>, iw: u32, ih: u32) -> (u32, u32) {
        match &el.kind {
            Kind::Empty => (0, 0),
            Kind::Text { text, style, max_lines, .. } => {
                let l = self.text(text, *style, iw, *max_lines);
                (l.width.min(iw), l.height)
            }
            Kind::Image { img, .. } => (img.width, img.height),
            Kind::Rule { thickness, .. } => (*thickness, *thickness),
            Kind::Canvas { w, h, .. } => (*w, *h),
            Kind::Stack { children } => children.iter().fold((0, 0), |(w, h), c| {
                let (cw, ch) = self.measure(c, iw, ih);
                (w.max(cw), h.max(ch))
            }),
            Kind::Paged { .. } => (iw, ih),
            Kind::Flex { axis, children, gap, .. } => {
                let (main, cross) = if *axis == Axis::Row { (iw, ih) } else { (ih, iw) };
                let sizes = self.flex_sizes(children, *axis, main, cross, *gap);
                let has_fill = children.iter().any(|c| matches!(main_size(c, *axis), Size::Fill(_)));
                let used: u32 = sizes.iter().map(|s| s.0).sum::<u32>() + gap * children.len().saturating_sub(1) as u32;
                let m = if has_fill { main } else { used.min(main) };
                let c = sizes.iter().map(|s| s.1).max().unwrap_or(0);
                if *axis == Axis::Row { (m, c) } else { (c, m) }
            }
        }
    }

    /// (main, cross) size of each child.
    fn flex_sizes<M>(&self, children: &[El<M>], axis: Axis, main: u32, cross: u32, gap: u32) -> Vec<(u32, u32)> {
        let mut sizes = vec![(0u32, 0u32); children.len()];
        let mut used = gap * children.len().saturating_sub(1) as u32;
        let mut weights = 0u32;
        let pick = |w: u32, h: u32| if axis == Axis::Row { (w, h) } else { (h, w) };
        for (i, c) in children.iter().enumerate() {
            match main_size(c, axis) {
                Size::Fill(wt) => weights += wt.max(1) as u32,
                _ => {
                    let rem = main.saturating_sub(used);
                    let (w, h) = if axis == Axis::Row { self.measure(c, rem, cross) } else { self.measure(c, cross, rem) };
                    sizes[i] = pick(w, h);
                    used += sizes[i].0;
                }
            }
        }
        if weights > 0 {
            let leftover = main.saturating_sub(used);
            let mut given = 0;
            let fills: Vec<usize> = (0..children.len()).filter(|&i| matches!(main_size(&children[i], axis), Size::Fill(_))).collect();
            for (k, &i) in fills.iter().enumerate() {
                let Size::Fill(wt) = main_size(&children[i], axis) else { unreachable!() };
                let share = if k + 1 == fills.len() { leftover - given } else { leftover * wt.max(1) as u32 / weights };
                given += share;
                let (w, h) = if axis == Axis::Row { self.measure(&children[i], share, cross) } else { self.measure(&children[i], cross, share) };
                sizes[i] = (share, pick(w, h).1);
            }
        }
        sizes
    }

    /// Place `el` exactly in `rect`, emitting draw ops and tap areas.
    pub fn place<M: Clone>(&self, el: &El<M>, rect: Rect, out: &mut Out<M>) {
        if el.bg.is_some() || el.border.is_some() {
            out.ops.push(Op::Box { rect, bg: el.bg, border: el.border, radius: el.radius });
        }
        if el.on_tap.is_some() || el.on_hold.is_some() || el.block {
            out.hits.push(Hit { rect, tap: el.on_tap.clone(), hold: el.on_hold.clone() });
        }
        let inner = inset(rect, &el.padding);
        match &el.kind {
            Kind::Empty => {}
            Kind::Text { text, style, max_lines, align } => {
                let l = self.text(text, *style, inner.w, *max_lines);
                let a = match align {
                    Align::Center => 0.5,
                    Align::End => 1.0,
                    _ => 0.0,
                };
                out.ops.push(Op::Text { layout: l, x: inner.x as i32, y: inner.y as i32, w: inner.w, align: a });
            }
            Kind::Image { img, tint } => {
                let x = inner.x as i32 + (inner.w as i32 - img.width as i32) / 2;
                let y = inner.y as i32 + (inner.h as i32 - img.height as i32) / 2;
                out.ops.push(Op::Image { img: img.clone(), x, y, tint: *tint });
            }
            Kind::Rule { gray, .. } => out.ops.push(Op::Rule { rect: inner, gray: *gray }),
            Kind::Canvas { draw, .. } => out.ops.push(Op::Canvas { draw: draw.clone(), rect: inner }),
            Kind::Stack { children } => {
                for c in children {
                    self.place(c, inner, out);
                }
            }
            Kind::Flex { axis, children, gap, cross, justify } => self.place_flex(children, *axis, *gap, *cross, *justify, inner, out),
            Kind::Paged { children, page, on_page, gap } => self.place_paged(children, *page, on_page, *gap, inner, out),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn place_flex<M: Clone>(&self, children: &[El<M>], axis: Axis, gap: u32, cross_align: Align, justify: Justify, r: Rect, out: &mut Out<M>) {
        let (main, cross) = if axis == Axis::Row { (r.w, r.h) } else { (r.h, r.w) };
        let sizes = self.flex_sizes(children, axis, main, cross, gap);
        let n = children.len() as u32;
        let used: u32 = sizes.iter().map(|s| s.0).sum::<u32>() + gap * n.saturating_sub(1);
        let free = main.saturating_sub(used);
        let (mut pos, extra) = match justify {
            Justify::Start => (0, 0),
            Justify::Center => (free / 2, 0),
            Justify::End => (free, 0),
            Justify::SpaceBetween => (0, if n > 1 { free / (n - 1) } else { 0 }),
        };
        for (c, &(m, cs)) in children.iter().zip(&sizes) {
            let stretch = cross_align == Align::Stretch || matches!(cross_size(c, axis), Size::Fill(_));
            let csz = if stretch { cross } else { cs.min(cross) };
            let coff = match (stretch, cross_align) {
                (true, _) => 0,
                (_, Align::Center) => (cross - csz) / 2,
                (_, Align::End) => cross - csz,
                _ => 0,
            };
            let child = if axis == Axis::Row { Rect::new(r.x + pos, r.y + coff, m, csz) } else { Rect::new(r.x + coff, r.y + pos, csz, m) };
            self.place(c, child, out);
            pos += m + gap + extra;
        }
    }

    fn place_paged<M: Clone>(&self, children: &[El<M>], page: usize, on_page: &Rc<dyn Fn(usize) -> M>, gap: u32, r: Rect, out: &mut Out<M>) {
        let heights: Vec<u32> = children.iter().map(|c| self.measure(c, r.w, r.h).1).collect();
        let total: u32 = heights.iter().sum::<u32>() + gap * heights.len().saturating_sub(1) as u32;
        let avail = if total > r.h { r.h.saturating_sub(PAGER_H) } else { r.h };
        // Greedy split into pages.
        let mut pages: Vec<(usize, usize)> = Vec::new();
        let (mut start, mut acc) = (0usize, 0u32);
        for (i, &h) in heights.iter().enumerate() {
            let add = if i == start { h } else { h + gap };
            if i > start && acc + add > avail {
                pages.push((start, i));
                start = i;
                acc = h;
            } else {
                acc += add;
            }
        }
        pages.push((start, children.len()));
        let count = pages.len();
        let page = page.min(count - 1);
        let (a, b) = pages[page];
        let mut y = r.y;
        for i in a..b {
            self.place(&children[i], Rect::new(r.x, y, r.w, heights[i].min(avail)), out);
            y += heights[i] + gap;
        }
        let prev = (page > 0).then(|| on_page(page - 1));
        let next = (page + 1 < count).then(|| on_page(page + 1));
        if count > 1 {
            let fy = r.y + r.h - PAGER_H;
            out.ops.push(Op::Rule { rect: Rect::new(r.x, fy, r.w, 1), gray: 190 });
            let label = self.text(&format!("{} / {}", page + 1, count), TextStyle::new(20).color(60), r.w, Some(1));
            let ly = fy + (PAGER_H - label.height) / 2;
            out.ops.push(Op::Text { layout: label, x: r.x as i32, y: ly as i32, w: r.w, align: 0.5 });
            let bw = 120.min(r.w / 3);
            for (msg, x, name) in [(&prev, r.x, "chevron-left"), (&next, r.x + r.w - bw, "chevron-right")] {
                if let (Some(m), Some(img)) = (msg, crate::icons::render(name, 32)) {
                    let ix = x as i32 + (bw as i32 - 32) / 2;
                    out.ops.push(Op::Image { img, x: ix, y: fy as i32 + (PAGER_H as i32 - 32) / 2, tint: Some(0) });
                    out.hits.push(Hit { rect: Rect::new(x, fy, bw, PAGER_H), tap: Some(m.clone()), hold: None });
                }
            }
        }
        out.swipes.push(Swipe { rect: r, prev, next });
    }

    pub fn draw(&self, ops: &[Op], p: &mut Painter) {
        for op in ops {
            match op {
                Op::Box { rect, bg, border, radius } => {
                    if *radius > 0.5 {
                        if let Some(g) = bg {
                            p.round_rect(*rect, *radius, *g, None);
                        }
                        if let Some((w, g)) = border {
                            p.round_rect(*rect, *radius, *g, Some(*w));
                        }
                    } else {
                        if let Some(g) = bg {
                            p.fill_rect(*rect, *g);
                        }
                        if let Some((w, g)) = border {
                            p.stroke_rect(*rect, w.round().max(1.0) as u32, *g);
                        }
                    }
                }
                Op::Text { layout, x, y, w, align } => self.fonts.draw_aligned(p, layout, *x, *y, *w, *align),
                Op::Image { img, x, y, tint } => match tint {
                    Some(g) => p.draw_tinted(*x, *y, img, *g),
                    None => p.draw_image(*x, *y, img),
                },
                Op::Rule { rect, gray } => p.fill_rect(*rect, *gray),
                Op::Canvas { draw, rect } => {
                    p.set_clip(*rect);
                    draw(p, *rect);
                    p.reset_clip();
                }
            }
        }
    }
}

fn main_size<M>(el: &El<M>, axis: Axis) -> Size {
    if axis == Axis::Row { el.width } else { el.height }
}

fn cross_size<M>(el: &El<M>, axis: Axis) -> Size {
    if axis == Axis::Row { el.height } else { el.width }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::el::*;

    fn layout(el: &El<u8>, w: u32, h: u32) -> Out<u8> {
        let fonts = Fonts::new();
        let l = Layouter::new(&fonts);
        let mut out = Out::default();
        l.place(el, Rect::new(0, 0, w, h), &mut out);
        out
    }

    #[test]
    fn fill_splits_leftover() {
        let el = row(vec![empty().w(100).on_tap(1), empty().fill_w().on_tap(2), empty().width(Size::Fill(3)).on_tap(3)]).align(Align::Stretch);
        let out = layout(&el, 500, 50);
        let r: Vec<Rect> = out.hits.iter().map(|h| h.rect).collect();
        assert_eq!(r[0], Rect::new(0, 0, 100, 50));
        assert_eq!(r[1], Rect::new(100, 0, 100, 50));
        assert_eq!(r[2], Rect::new(200, 0, 300, 50));
    }

    #[test]
    fn column_stacks_with_gap_and_padding() {
        let el = col(vec![empty().h(40).on_tap(1), empty().h(60).on_tap(2)]).gap(10).padding(20);
        let out = layout(&el, 300, 400);
        assert_eq!(out.hits[0].rect, Rect::new(20, 20, 260, 40));
        assert_eq!(out.hits[1].rect, Rect::new(20, 70, 260, 60));
    }

    #[test]
    fn row_centers_cross_axis() {
        let el = col(vec![row(vec![empty().w(10).h(10).on_tap(1)]).h(50)]);
        let out = layout(&el, 100, 100);
        assert_eq!(out.hits[0].rect, Rect::new(0, 20, 10, 10));
    }

    #[test]
    fn space_between() {
        let el = row(vec![empty().w(10).on_tap(1), empty().w(10).on_tap(2)]).justify(Justify::SpaceBetween);
        let out = layout(&el, 100, 10);
        assert_eq!(out.hits[1].rect.x, 90);
    }

    #[test]
    fn paged_splits_and_wires_navigation() {
        let items: Vec<El<u8>> = (0..10).map(|i| empty().h(100).on_tap(i)).collect();
        let el = paged(items, 1, |p| 100 + p as u8);
        let out = layout(&el, 300, 456); // 400 for items + 56 pager -> 4 per page
        let taps: Vec<u8> = out.hits.iter().filter_map(|h| h.tap).collect();
        assert_eq!(taps, vec![4, 5, 6, 7, 100, 102]);
        assert_eq!(out.swipes.len(), 1);
        assert_eq!((out.swipes[0].prev, out.swipes[0].next), (Some(100), Some(102)));
    }

    #[test]
    fn text_wraps_to_parent_width() {
        let el: El<u8> = col(vec![text("one two three four five six seven eight nine ten")]);
        let out = layout(&el, 120, 500);
        let Op::Text { layout, .. } = &out.ops[0] else { panic!() };
        assert!(layout.lines.len() > 2);
    }
}
