//! Loading images and fonts from disk, with sensible search paths on the
//! Kindle and on a development PC.

use inkpad_gfx::{FamilyId, Fonts, GrayImage};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// Where inkpad looks for asset files, in order:
/// `$INKPAD_ASSETS`, `/mnt/us/inkpad/assets` (Kindle), `./assets` and the
/// workspace's `assets/` (development).
pub fn asset_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Ok(p) = std::env::var("INKPAD_ASSETS") {
        v.push(PathBuf::from(p));
    }
    // An app's own assets, installed next to its binary.
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("assets"))) {
        v.push(dir);
    }
    v.push(PathBuf::from("/mnt/us/inkpad/assets"));
    v.push(PathBuf::from("assets"));
    v.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"));
    v
}

/// Font folders: inkpad's own, the Kindle's user and system fonts, then the
/// development `assets/fonts`.
pub fn font_dirs() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = asset_dirs().into_iter().map(|d| d.join("fonts")).collect();
    v.extend(["/mnt/us/fonts", "/usr/java/lib/fonts", "/usr/share/fonts", "/usr/share/fonts/truetype"].map(PathBuf::from));
    v
}

/// Load a font family by file stem (e.g. `"Bookerly"` finds
/// `Bookerly-Regular.ttf`, `Bookerly-Bold.ttf`, …) from [`font_dirs`].
pub fn load_font(fonts: &mut Fonts, name: &str, stem: &str) -> Option<FamilyId> {
    let id = fonts.load_family(name, stem, &font_dirs());
    if id.is_none() {
        eprintln!("font {stem:?} not found in {:?}", font_dirs());
    }
    id
}

thread_local! {
    static IMAGES: RefCell<HashMap<(PathBuf, u32, u32, bool), Option<Rc<GrayImage>>>> = RefCell::new(HashMap::new());
}

/// Load a PNG or SVG by relative path (searched in [`asset_dirs`]) or
/// absolute path, scaled to fit `max_w`×`max_h` (0 = natural size).
/// `dither` applies 16-level dithering, good for photos. Cached.
pub fn image(path: &str, max_w: u32, max_h: u32, dither: bool) -> Option<Rc<GrayImage>> {
    let p = Path::new(path);
    let full = if p.is_absolute() { Some(p.to_path_buf()) } else { asset_dirs().into_iter().map(|d| d.join(p)).find(|f| f.exists()) };
    let Some(full) = full else {
        eprintln!("image not found: {path}");
        return None;
    };
    let key = (full.clone(), max_w, max_h, dither);
    if let Some(hit) = IMAGES.with(|c| c.borrow().get(&key).cloned()) {
        return hit;
    }
    let loaded = load(&full, max_w, max_h, dither).map_err(|e| eprintln!("image {}: {e}", full.display())).ok().map(Rc::new);
    IMAGES.with(|c| c.borrow_mut().insert(key, loaded.clone()));
    loaded
}

fn load(path: &Path, max_w: u32, max_h: u32, dither: bool) -> Result<GrayImage, String> {
    let is_svg = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("svg"));
    let img = if is_svg && max_w > 0 && max_h > 0 {
        GrayImage::from_svg(&std::fs::read(path).map_err(|e| e.to_string())?, Some((max_w, max_h)))?
    } else {
        let img = GrayImage::load(path)?;
        if max_w > 0 && max_h > 0 && (img.width > max_w || img.height > max_h) { img.fit(max_w, max_h) } else { img }
    };
    Ok(if dither { img.dither16() } else { img })
}
