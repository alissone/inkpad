//! Finding books, reading their metadata once, and caching the result.
//!
//! The index (`<data>/library/index.json`) remembers every book by path,
//! size and modification time, so after the first scan only new or changed
//! files are opened. Cover thumbnails are cached as small PNGs.

use crate::meta::{self, Meta};
use inkpad::ui::GrayImage;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Book {
    pub path: String,
    pub size: u64,
    pub mtime: i64,
    pub title: String,
    pub author: String,
    /// "EPUB", "MOBI", "KFX", …
    pub format: String,
    /// Cached cover thumbnail (PNG), if the book has a cover.
    pub cover: Option<String>,
    /// Amazon KFX or DRM: only the Kindle reader can open it.
    pub kindle_only: bool,
    pub sample: bool,
    /// Folder relative to the documents folder ("" = top level).
    pub folder: String,
    /// Reading progress from KOReader (0–1), refreshed on every scan.
    #[serde(default)]
    pub progress: Option<f32>,
    #[serde(default)]
    pub finished: bool,
    /// When KOReader last saved progress (Unix seconds), 0 = never.
    #[serde(default)]
    pub last_read: i64,
}

impl Book {
    pub fn file_name(&self) -> String {
        Path::new(&self.path).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
    }
}

/// File types KOReader can open (plus Kindle-only KFX).
const EXTENSIONS: &[&str] = &["epub", "pdf", "mobi", "azw", "azw3", "prc", "pdb", "kfx", "fb2", "cbz", "djvu", "txt", "md", "html", "htm", "rtf", "docx", "chm"];
const COVER_W: u32 = 140;
const COVER_H: u32 = 200;

/// Where books are: `$INKPAD_DOCUMENTS`, else `/mnt/us/documents` on the
/// Kindle, else `sample-books/` in this repo.
pub fn documents_dir() -> PathBuf {
    if let Ok(p) = std::env::var("INKPAD_DOCUMENTS") {
        return PathBuf::from(p);
    }
    if cfg!(all(target_os = "linux", target_arch = "arm")) {
        return PathBuf::from("/mnt/us/documents");
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).join("sample-books")
}

/// The Kindle's own cover thumbnails (`system/thumbnails`, next to `documents`).
pub fn thumbnails_dir() -> PathBuf {
    if let Ok(p) = std::env::var("INKPAD_THUMBNAILS") {
        return PathBuf::from(p);
    }
    documents_dir().parent().map(|p| p.join("system").join("thumbnails")).unwrap_or_default()
}

pub fn cache_dir() -> PathBuf {
    let d = inkpad::ui::store::data_dir().join("library");
    let _ = std::fs::create_dir_all(d.join("covers"));
    d
}

fn mtime_of(md: &std::fs::Metadata) -> i64 {
    md.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// A book file found on disk.
#[derive(Clone, Debug)]
pub struct Found {
    pub path: PathBuf,
    pub size: u64,
    pub mtime: i64,
}

/// Every book file under `root`, skipping hidden files, sidecar folders
/// (`*.sdr`), dictionaries and the Kindle's clippings file.
pub fn list_files(root: &Path) -> Vec<Found> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let Ok(md) = e.metadata() else { continue };
            let path = e.path();
            if md.is_dir() {
                let skip = name.to_lowercase().ends_with(".sdr") || (dir == root && name.eq_ignore_ascii_case("dictionaries"));
                if !skip {
                    stack.push(path);
                }
                continue;
            }
            let ext = path.extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
            if EXTENSIONS.contains(&ext.as_str()) && name != "My Clippings.txt" {
                out.push(Found { path, size: md.len(), mtime: mtime_of(&md) });
            }
        }
    }
    out
}

/// Split found files into books we already know (unchanged) and files that
/// need indexing.
pub fn plan(found: Vec<Found>, cached: &[Book]) -> (Vec<Book>, Vec<Found>) {
    let mut known = Vec::new();
    let mut todo = Vec::new();
    for f in found {
        let p = f.path.to_string_lossy();
        match cached.iter().find(|b| b.path == p && b.size == f.size && b.mtime == f.mtime) {
            Some(b) => known.push(b.clone()),
            None => todo.push(f),
        }
    }
    (known, todo)
}

fn fnv(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3))
}

/// The Kindle's thumbnail for `asin`, if it made one.
fn kindle_thumbnail(thumbs: &[String], asin: &str) -> Option<PathBuf> {
    let prefix = format!("thumbnail_{asin}_");
    thumbs.iter().find(|n| n.starts_with(&prefix) && n.ends_with("_portrait.jpg")).map(|n| thumbnails_dir().join(n))
}

pub fn thumbnail_names() -> Vec<String> {
    std::fs::read_dir(thumbnails_dir()).map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default()
}

/// Read one book's metadata and cache its cover.
pub fn index_one(f: &Found, root: &Path, thumbs: &[String]) -> Book {
    let ext = f.path.extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
    let parsed = match ext.as_str() {
        "epub" => meta::epub(&f.path),
        "mobi" | "azw" | "azw3" | "prc" | "pdb" => meta::mobi(&f.path),
        _ => Err(String::new()),
    };
    if let Err(e) = &parsed {
        if !e.is_empty() {
            eprintln!("library: {}: {e}", f.path.display());
        }
    }
    let by_name = meta::from_file_name(&f.path);
    let m = parsed.unwrap_or_default();
    let asin = m.asin.clone().or(by_name.asin.clone());
    let sample = by_name.cdetype.as_deref() == Some("EBSP") || m.cdetype.as_deref() == Some("EBSP");
    let path_s = f.path.to_string_lossy().into_owned();

    // Cover: embedded, else the Kindle's thumbnail, else none (drawn by the UI).
    let cover_png = cache_dir().join("covers").join(format!("{:016x}.png", fnv(&path_s)));
    let source: Option<GrayImage> = m
        .cover
        .as_deref()
        .and_then(|b| GrayImage::from_bytes_for(b, COVER_W, COVER_H).ok())
        .or_else(|| {
            let p = asin.as_deref().and_then(|a| kindle_thumbnail(thumbs, a))?;
            GrayImage::from_bytes_for(&std::fs::read(p).ok()?, COVER_W, COVER_H).ok()
        });
    let cover = source.and_then(|img| {
        let png = img.cover(COVER_W, COVER_H).to_png().ok()?;
        inkpad::ui::store::write_atomic(&cover_png, &png).ok()?;
        Some(cover_png.to_string_lossy().into_owned())
    });

    let Meta { title, author, drm, .. } = m;
    let folder = f.path.parent().and_then(|p| p.strip_prefix(root).ok()).map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_default();
    Book {
        title: meta::tidy_title(&title.or(by_name.title).unwrap_or_else(|| "Untitled".into())),
        author: meta::tidy_author(&author.or(by_name.author).unwrap_or_default()),
        format: ext.to_uppercase(),
        cover,
        kindle_only: ext == "kfx" || drm,
        sample,
        folder,
        path: path_s,
        size: f.size,
        mtime: f.mtime,
        progress: None,
        finished: false,
        last_read: 0,
    }
}

/// Refresh reading progress from KOReader's sidecar
/// (`<book>.sdr/metadata.<ext>.lua`).
pub fn apply_progress(b: &mut Book) {
    let p = Path::new(&b.path);
    let (Some(stem), Some(ext)) = (p.file_stem(), p.extension()) else { return };
    let sdr = p.with_file_name(format!("{}.sdr", stem.to_string_lossy()));
    let file = sdr.join(format!("metadata.{}.lua", ext.to_string_lossy()));
    let Ok(text) = std::fs::read_to_string(&file) else { return };
    b.progress = lua_number(&text, "percent_finished").map(|v| v.clamp(0.0, 1.0) as f32);
    b.finished = text.contains("[\"status\"] = \"complete\"");
    b.last_read = std::fs::metadata(&file).map(|m| mtime_of(&m)).unwrap_or(0);
}

fn lua_number(text: &str, key: &str) -> Option<f64> {
    let i = text.find(&format!("[\"{key}\"]"))?;
    let rest = &text[i..];
    let v = rest[rest.find('=')? + 1..].trim_start();
    let end = v.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == 'e' || c == 'E')).unwrap_or(v.len());
    v[..end].parse().ok()
}

#[derive(Serialize, Deserialize, Default)]
struct IndexFile {
    version: u32,
    books: Vec<Book>,
}

const INDEX: &str = "library/index.json";

pub fn load_index() -> Vec<Book> {
    inkpad::ui::store::read_text(INDEX).and_then(|s| serde_json::from_str::<IndexFile>(&s).ok()).filter(|i| i.version == 1).map(|i| i.books).unwrap_or_default()
}

pub fn save_index(books: &[Book]) {
    let idx = IndexFile { version: 1, books: books.to_vec() };
    if let Ok(s) = serde_json::to_string(&idx) {
        if let Err(e) = inkpad::ui::store::write_text(INDEX, &s) {
            eprintln!("library: saving index: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_koreader_progress() {
        let t = "return {\n    [\"percent_finished\"] = 0.4523,\n    [\"summary\"] = {\n        [\"status\"] = \"reading\",\n    },\n}";
        assert_eq!(lua_number(t, "percent_finished"), Some(0.4523));
        assert_eq!(lua_number(t, "missing"), None);
    }
}
