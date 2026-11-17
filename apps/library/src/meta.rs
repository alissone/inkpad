//! Book metadata straight from the files: title, author, cover image, and
//! the Amazon id the Kindle uses to name its cover thumbnails.

use crate::zip::Zip;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

#[derive(Default, Debug, Clone)]
pub struct Meta {
    pub title: Option<String>,
    pub author: Option<String>,
    /// Encoded cover image (JPEG/PNG), if the file has one.
    pub cover: Option<Vec<u8>>,
    /// ASIN or Kindle content key (names `system/thumbnails/thumbnail_<id>_*`).
    pub asin: Option<String>,
    /// Kindle content type (`EBOK`, `EBSP` sample, `PDOC`).
    pub cdetype: Option<String>,
    /// DRM-protected: only the Kindle reader can open it.
    pub drm: bool,
}

// ---- tiny XML helpers (enough for OPF/container files) ----

fn decode_entities(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&#39;", "'").replace("&amp;", "&")
}

/// Text inside the first `<name ...>` or `<prefix:name ...>` element.
fn tag_text(xml: &str, name: &str) -> Option<String> {
    let mut from = 0;
    while let Some(i) = xml[from..].find('<').map(|i| i + from) {
        from = i + 1;
        let rest = &xml[i + 1..];
        let end_name = rest.find(|c: char| c.is_whitespace() || c == '>' || c == '/')?;
        let tag = &rest[..end_name];
        let local = tag.rsplit(':').next().unwrap_or(tag);
        if local != name || tag.starts_with('/') {
            continue;
        }
        let open_end = rest.find('>')? + i + 2;
        let close = xml[open_end..].find("</")? + open_end;
        let t = decode_entities(xml[open_end..close].trim());
        return (!t.is_empty()).then_some(t);
    }
    None
}

/// Value of attribute `attr` in a tag string.
fn attr(tag: &str, attr: &str) -> Option<String> {
    for q in ['"', '\''] {
        let pat = format!("{attr}={q}");
        if let Some(i) = tag.find(&pat) {
            // Make sure we matched a whole attribute name.
            if i > 0 && !tag[..i].ends_with(|c: char| c.is_whitespace()) {
                continue;
            }
            let v = &tag[i + pat.len()..];
            return v.find(q).map(|e| decode_entities(&v[..e]));
        }
    }
    None
}

/// All start tags named `name` (any prefix), as raw strings.
fn tags<'a>(xml: &'a str, name: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = xml[from..].find('<').map(|i| i + from) {
        let Some(end) = xml[i..].find('>').map(|e| e + i) else { break };
        let tag = &xml[i + 1..end];
        let tname = tag.split(|c: char| c.is_whitespace() || c == '/').next().unwrap_or("");
        if tname.rsplit(':').next() == Some(name) {
            out.push(tag);
        }
        from = end + 1;
    }
    out
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Resolve `href` relative to the directory of `base` inside a zip.
fn join_zip(base: &str, href: &str) -> String {
    let mut parts: Vec<&str> = base.rsplit_once('/').map(|(d, _)| d.split('/').collect()).unwrap_or_default();
    for p in href.split('/') {
        match p {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            p => parts.push(p),
        }
    }
    parts.join("/")
}

pub fn epub(path: &Path) -> Result<Meta, String> {
    let mut z = Zip::open(path)?;
    let container = String::from_utf8_lossy(&z.read("META-INF/container.xml")?).into_owned();
    let opf_path = tags(&container, "rootfile").first().and_then(|t| attr(t, "full-path")).ok_or("no rootfile")?;
    let opf = String::from_utf8_lossy(&z.read(&opf_path)?).into_owned();
    let mut m = Meta { title: tag_text(&opf, "title"), author: tag_text(&opf, "creator"), ..Meta::default() };
    let items = tags(&opf, "item");
    // EPUB 3: properties="cover-image"; EPUB 2: <meta name="cover" content="id">.
    let cover_id = tags(&opf, "meta").into_iter().find(|t| attr(t, "name").as_deref() == Some("cover")).and_then(|t| attr(t, "content"));
    fn is_image(t: &str) -> bool {
        attr(t, "media-type").is_some_and(|m| m.starts_with("image/"))
    }
    let cover_item = items
        .iter()
        .find(|t| attr(t, "properties").is_some_and(|p| p.split_whitespace().any(|x| x == "cover-image")))
        .or_else(|| cover_id.as_ref().and_then(|id| items.iter().find(|t| attr(t, "id").as_deref() == Some(id.as_str()))))
        .or_else(|| items.iter().filter(|t| is_image(t)).find(|t| attr(t, "id").or(attr(t, "href")).is_some_and(|s| s.to_lowercase().contains("cover"))));
    if let Some(href) = cover_item.filter(|t| is_image(t)).and_then(|t| attr(t, "href")) {
        m.cover = z.read(&join_zip(&opf_path, &percent_decode(&href))).ok();
    }
    Ok(m)
}

// ---- MOBI / AZW / AZW3 (PalmDB) ----

fn be16(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(o..o + 2)?.try_into().ok()?))
}
fn be32(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(o..o + 4)?.try_into().ok()?))
}

fn decode_text(b: &[u8], utf8: bool) -> String {
    let s = if utf8 { String::from_utf8_lossy(b).into_owned() } else { b.iter().map(|&c| c as char).collect() };
    s.trim_matches(char::from(0)).trim().to_string()
}

pub fn mobi(path: &Path) -> Result<Meta, String> {
    let mut f = File::open(path).map_err(|e| e.to_string())?;
    let mut hdr = [0u8; 78];
    f.read_exact(&mut hdr).map_err(|e| e.to_string())?;
    if &hdr[60..68] != b"BOOKMOBI" && &hdr[60..68] != b"TEXtREAd" {
        return Err("not a MOBI file".into());
    }
    let n = be16(&hdr, 76).unwrap_or(0) as usize;
    let mut table = vec![0u8; n * 8];
    f.read_exact(&mut table).map_err(|e| e.to_string())?;
    let offsets: Vec<u64> = (0..n).map(|i| be32(&table, i * 8).unwrap_or(0) as u64).collect();
    let len = f.metadata().map_err(|e| e.to_string())?.len();
    let mut read_record = |i: usize| -> Option<Vec<u8>> {
        let start = *offsets.get(i)?;
        let end = offsets.get(i + 1).copied().unwrap_or(len);
        if end <= start || end - start > 16 << 20 {
            return None;
        }
        let mut buf = vec![0u8; (end - start) as usize];
        f.seek(SeekFrom::Start(start)).ok()?;
        f.read_exact(&mut buf).ok()?;
        Some(buf)
    };
    let r0 = read_record(0).ok_or("no record 0")?;
    let mut m = Meta { drm: be16(&r0, 12).unwrap_or(0) != 0, ..Meta::default() };
    if r0.get(16..20) != Some(b"MOBI") {
        // Plain PalmDOC: the title is the database name.
        m.title = Some(decode_text(&hdr[..32], false));
        return Ok(m);
    }
    let mobi_len = be32(&r0, 20).unwrap_or(0) as usize;
    let utf8 = be32(&r0, 28) == Some(65001);
    let first_image = be32(&r0, 108).unwrap_or(u32::MAX);
    if let (Some(off), Some(nl)) = (be32(&r0, 84), be32(&r0, 88)) {
        if let Some(b) = r0.get(off as usize..(off + nl) as usize) {
            m.title = Some(decode_text(b, utf8)).filter(|s| !s.is_empty());
        }
    }
    let mut cover_off = None;
    if be32(&r0, 128).unwrap_or(0) & 0x40 != 0 {
        let e = 16 + mobi_len;
        if r0.get(e..e + 4) == Some(b"EXTH") {
            let count = be32(&r0, e + 8).unwrap_or(0);
            let mut p = e + 12;
            for _ in 0..count {
                let (Some(ty), Some(rl)) = (be32(&r0, p), be32(&r0, p + 4)) else { break };
                let Some(data) = r0.get(p + 8..p + rl.max(8) as usize) else { break };
                match ty {
                    100 => m.author = Some(decode_text(data, utf8)).filter(|s| !s.is_empty()).or(m.author.take()),
                    503 => m.title = Some(decode_text(data, utf8)).filter(|s| !s.is_empty()).or(m.title.take()),
                    201 => cover_off = be32(data, 0),
                    113 | 504 if m.asin.is_none() => m.asin = Some(decode_text(data, true)).filter(|s| !s.is_empty()),
                    501 => m.cdetype = Some(decode_text(data, true)),
                    _ => {}
                }
                p += rl.max(8) as usize;
            }
        }
    }
    if let Some(co) = cover_off {
        if first_image != u32::MAX && co != u32::MAX {
            m.cover = read_record((first_image + co) as usize).filter(|b| b.starts_with(&[0xFF, 0xD8]) || b.starts_with(&[0x89, b'P']));
        }
    }
    Ok(m)
}

// ---- KFX and other files: what the file name tells us ----

/// "Dom Casmurro (Portuguese Edition)_B089N98YDT.kfx" → title, ASIN, sample.
/// Calibre names: "Title - Author.mobi" inside an "Author" folder.
pub fn from_file_name(path: &Path) -> Meta {
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let mut m = Meta::default();
    let mut name = stem.as_str();
    if let Some(s) = name.strip_suffix("_sample") {
        name = s;
        m.cdetype = Some("EBSP".into());
    }
    if let Some((t, id)) = name.rsplit_once('_') {
        let is_asin = id.len() == 10 && id.starts_with('B') && id.chars().all(|c| c.is_ascii_alphanumeric());
        let is_uuid = id.len() == 36 && id.chars().filter(|&c| c == '-').count() == 4;
        if is_asin || is_uuid {
            m.asin = Some(id.to_string());
            name = t;
        }
    }
    // Calibre: "<Author>/<Title> - <Author>.ext". Only trust " - Author" when
    // the folder agrees, so subtitles like "- Vol. 5" stay in the title.
    let folder = path.parent().and_then(|p| p.file_name()).map(|s| s.to_string_lossy().into_owned());
    let mut title = name.to_string();
    if let Some((t, a)) = name.rsplit_once(" - ") {
        if tidy_author(a).is_empty() {
            // "Title - Unknown" / "Title - __": a placeholder author.
            title = t.to_string();
        } else if folder.as_deref().is_some_and(|f| f == a || tidy_author(f) == tidy_author(a)) {
            title = t.to_string();
            m.author = Some(a.to_string());
        }
    }
    // Placeholder folder ("Unknown/As Meninas - Lygia Fagundes Telles"): the
    // author may still be at the end of the title.
    // Also when the "author" is just one word like a folder name ("Desktop").
    let weak = m.author.as_deref().is_none_or(|a| !a.contains(' ') && !a.contains(','));
    if weak {
        if let Some((t, a)) = title.rsplit_once(" - ") {
            if looks_like_name(a) {
                m.author = Some(a.to_string());
                title = t.to_string();
            }
        }
    }
    // "Title - Author - Author": drop the repeat.
    if let Some(a) = &m.author {
        if let Some(t) = title.strip_suffix(&format!(" - {a}")) {
            title = t.to_string();
        }
    }
    // Calibre/Kindle replace ':' with '_' in file names.
    m.title = Some(title.replace("_ ", ": ").trim().to_string());
    m
}

/// Calibre-style sort titles ("Arte de Argumentar, A" → "A Arte de
/// Argumentar") and Amazon's "(Portuguese Edition)" suffixes.
pub fn tidy_title(t: &str) -> String {
    let mut t = t.trim().to_string();
    for suffix in [" (Portuguese Edition)", " (English Edition)", " (Edição em Português)", " (Spanish Edition)"] {
        if let Some(base) = t.strip_suffix(suffix) {
            t = base.trim().to_string();
        }
    }
    for art in [", A", ", O", ", As", ", Os", ", The", ", Um", ", Uma"] {
        if let Some(base) = t.strip_suffix(art) {
            return format!("{} {base}", &art[2..]);
        }
    }
    t
}

/// "Mukherjee, Siddhartha" → "Siddhartha Mukherjee"; placeholders → "".
pub fn tidy_author(a: &str) -> String {
    let a = a.trim().trim_end_matches([';', ',']).trim();
    let lower = a.to_lowercase();
    if a.chars().all(|c| !c.is_alphanumeric()) || ["unknown", "desconhecido", "anonymous", "autor desconhecido"].contains(&lower.as_str()) {
        return String::new();
    }
    // Several authors: tidy each.
    if a.contains(';') || a.contains(" & ") {
        return a.split([';', '&']).map(tidy_author).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", ");
    }
    match a.split_once(", ") {
        Some((last, first)) if last == first => last.to_string(),
        Some((last, first)) if !first.contains(',') && first.split_whitespace().count() <= 3 => {
            let fix = |s: &str| if s.len() > 2 && s.chars().all(|c| !c.is_lowercase()) { title_case(s) } else { s.to_string() };
            format!("{} {}", fix(first), fix(last))
        }
        _ => a.to_string(),
    }
}

/// "Maria Firmina dos Reis": 2–5 words, capitalized or small connectors, no digits.
fn looks_like_name(s: &str) -> bool {
    let words: Vec<&str> = s.split_whitespace().collect();
    (2..=5).contains(&words.len())
        && !s.chars().any(|c| c.is_ascii_digit() || "()[]:".contains(c))
        && words.iter().all(|w| w.chars().next().is_some_and(char::is_uppercase) || ["da", "de", "do", "das", "dos", "e", "van", "von"].contains(w))
}

fn title_case(s: &str) -> String {
    s.split(' ')
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xml_helpers() {
        let opf = r#"<package><metadata><dc:title>Dom &amp; Casmurro</dc:title><dc:creator opf:role="aut">Machado</dc:creator>
            <meta name="cover" content="img1"/></metadata><manifest><item id="img1" href="Images/c%20over.jpg" media-type="image/jpeg"/></manifest></package>"#;
        assert_eq!(tag_text(opf, "title").as_deref(), Some("Dom & Casmurro"));
        assert_eq!(tag_text(opf, "creator").as_deref(), Some("Machado"));
        let item = tags(opf, "item")[0];
        assert_eq!(attr(item, "href").as_deref(), Some("Images/c%20over.jpg"));
        assert_eq!(attr(item, "id").as_deref(), Some("img1"));
        assert_eq!(join_zip("OEBPS/content.opf", &percent_decode("Images/c%20over.jpg")), "OEBPS/Images/c over.jpg");
        assert_eq!(join_zip("OEBPS/x/content.opf", "../img.png"), "OEBPS/img.png");
    }

    #[test]
    fn file_names() {
        let m = from_file_name(Path::new("/d/Dom Casmurro (Portuguese Edition)_B089N98YDT.kfx"));
        assert_eq!(m.title.as_deref(), Some("Dom Casmurro (Portuguese Edition)"));
        assert_eq!(m.asin.as_deref(), Some("B089N98YDT"));
        let s = from_file_name(Path::new("/d/Livro De Receitas_B01LVTOKD5_sample.kfx"));
        assert_eq!((s.asin.as_deref(), s.cdetype.as_deref()), (Some("B01LVTOKD5"), Some("EBSP")));
        let c = from_file_name(Path::new("/d/Ariely, Dan/Payoff_ The Hidden Logic - Dan Ariely.mobi"));
        assert_eq!(c.title.as_deref(), Some("Payoff: The Hidden Logic"));
        assert_eq!(c.author.as_deref(), Some("Dan Ariely"));
        assert_eq!(tidy_title("Arte de Argumentar, A"), "A Arte de Argumentar");
        assert_eq!(tidy_title("Dom Casmurro (Portuguese Edition)"), "Dom Casmurro");
        let v = from_file_name(Path::new("/d/Histórias Curtas_ Vol. 5 - Vol. 5 (Portuguese Edition)_B0923FN6J8.kfx"));
        assert_eq!(v.author, None);
        assert_eq!(tidy_author("Mukherjee, Siddhartha"), "Siddhartha Mukherjee");
        assert_eq!(tidy_author("MEYER, Joyce"), "Joyce Meyer");
        assert_eq!(tidy_author("Lane, Nick;"), "Nick Lane");
        assert_eq!(tidy_author("__"), "");
        let u = from_file_name(Path::new("/d/Unknown/Cantos a beira-mar - Maria Firmina dos Reis - Maria Firmina dos Reis.pdf"));
        assert_eq!((u.title.as_deref(), u.author.as_deref()), (Some("Cantos a beira-mar"), Some("Maria Firmina dos Reis")));
        let p = from_file_name(Path::new("/d/Unknown/Entre a Espada e a Rosa - Unknown.pdf"));
        assert_eq!((p.title.as_deref(), p.author), (Some("Entre a Espada e a Rosa"), None));
        assert_eq!(tidy_author("Unknown"), "");
        assert_eq!(tidy_author("Codeagro, Codeagro"), "Codeagro");
    }
}
