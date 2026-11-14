//! Bundled icons (Lucide, ISC license) rendered from SVG at any size.

use inkpad_gfx::GrayImage;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

macro_rules! icons {
    ($($name:literal),* $(,)?) => {
        /// Names of the bundled icons.
        pub const NAMES: &[&str] = &[$($name),*];
        fn svg(name: &str) -> Option<&'static str> {
            match name {
                $($name => Some(include_str!(concat!("../icons/", $name, ".svg"))),)*
                _ => None,
            }
        }
    };
}

icons!(
    "app-window", "arrow-big-up", "arrow-down-to-line", "arrow-left", "arrow-up-to-line", "arrow-up-down", "battery", "battery-charging", "bed", "bell",
    "book-marked", "book-open", "book-text", "calculator", "calendar", "check", "chevron-down", "chevron-left", "chevron-right",
    "circle", "circle-check", "clock", "cloud", "code", "corner-down-left", "delete", "file-text", "folder",
    "folder-open", "grid-2x2", "hard-drive", "house", "image", "info", "layout-grid", "layout-list",
    "library-big", "list", "list-todo", "lock", "lock-open", "log-in", "log-out", "message-circle", "monitor",
    "moon", "palette", "pencil", "plus", "pointer", "power", "refresh-cw", "search", "settings", "shapes",
    "sliders-horizontal", "sparkles", "star", "sun", "tablet-smartphone", "tag", "trash-2", "triangle-alert", "type",
    "utensils", "wallpaper", "wifi", "wifi-off", "x",
);

thread_local! {
    static CACHE: RefCell<HashMap<(String, u32, u32), Rc<GrayImage>>> = RefCell::new(HashMap::new());
    /// Stroke width in 24-unit icon space; Lucide's default is 2. Slightly
    /// heavier reads better on e-ink.
    static STROKE: Cell<f32> = const { Cell::new(2.25) };
}

/// Set the icon stroke weight (Lucide default 2.0; inkpad default 2.25).
pub fn set_stroke(w: f32) {
    STROKE.with(|s| s.set(w));
}

/// Render a bundled icon as an alpha mask `size`×`size` (cached).
pub fn render(name: &str, size: u32) -> Option<Rc<GrayImage>> {
    let stroke = STROKE.with(|s| s.get());
    let key = (name.to_string(), size, (stroke * 100.0) as u32);
    if let Some(img) = CACHE.with(|c| c.borrow().get(&key).cloned()) {
        return Some(img);
    }
    let src = svg(name)?.replace("stroke-width=\"2\"", &format!("stroke-width=\"{stroke}\""));
    let img = Rc::new(GrayImage::from_svg(src.as_bytes(), Some((size, size))).ok()?);
    CACHE.with(|c| c.borrow_mut().insert(key, img.clone()));
    Some(img)
}

#[cfg(test)]
mod tests {
    #[test]
    fn all_icons_render() {
        for name in super::NAMES {
            let img = super::render(name, 32).unwrap_or_else(|| panic!("{name}"));
            assert_eq!((img.width, img.height), (32, 32));
            assert!(img.alpha.as_ref().unwrap().iter().any(|&a| a > 200), "{name} is blank");
        }
    }
}
