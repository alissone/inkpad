//! Turning a story's HTML into simple blocks the reader can lay out like a
//! book page: paragraphs, headings, quotes, code, list items and images.
//! Deliberately forgiving: feeds carry every kind of HTML.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", content = "v")]
pub enum Block {
    P(String),
    H(String),
    Quote(String),
    Code(String),
    Li(String),
    /// While parsing: the image URL. Once downloaded: the local file name.
    Img(String),
    Hr,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    P,
    H,
    Li,
}

struct Builder {
    blocks: Vec<Block>,
    text: String,
    kind: Kind,
    quote: usize,
    pre: bool,
}

impl Builder {
    fn flush(&mut self) {
        let raw = std::mem::take(&mut self.text);
        if self.pre {
            let t = clean_chars(&decode_entities(&raw));
            let t = t.trim_matches('\n').to_string();
            if !t.trim().is_empty() {
                self.blocks.push(Block::Code(t));
            }
            return;
        }
        let t = collapse(&clean_chars(&decode_entities(&raw)));
        if t.is_empty() {
            return;
        }
        self.blocks.push(match (self.kind, self.quote > 0) {
            (Kind::H, _) => Block::H(t),
            (Kind::Li, _) => Block::Li(t),
            (Kind::P, true) => Block::Quote(t),
            (Kind::P, false) => Block::P(t),
        });
    }
}

/// Collapse runs of whitespace into single spaces.
fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Drop characters the e-ink fonts don't have (emoji and friends), and
/// invisible joiners, so they don't show up as boxes.
pub fn clean_chars(s: &str) -> String {
    s.chars()
        .filter(|&c| {
            let u = c as u32;
            !(u >= 0x1F000 || (0x2600..=0x27BF).contains(&u) || (0xFE00..=0xFE0F).contains(&u) || u == 0x200D || u == 0x200B || u == 0xFEFF || (0xE000..=0xF8FF).contains(&u))
        })
        .map(|c| if c == '\u{a0}' { ' ' } else { c })
        .collect()
}

/// Tags whose content is never shown.
const SKIP: &[&str] = &["script", "style", "iframe", "noscript", "svg", "button", "form", "select", "textarea", "video", "audio", "object", "head", "title"];
/// Tags that start or end a block.
const BLOCKS: &[&str] = &[
    "p", "div", "section", "article", "header", "footer", "main", "aside", "nav", "figure", "figcaption", "table", "thead", "tbody", "tr", "td", "th", "dl", "dt", "dd", "ul", "ol", "center", "details", "summary", "caption",
];

struct Tag<'a> {
    name: String,
    closing: bool,
    attrs: &'a str,
}

fn attr(attrs: &str, name: &str) -> Option<String> {
    let lower = attrs.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find(name) {
        let at = from + i;
        from = at + name.len();
        // Whole attribute name only (src, not data-src).
        if at > 0 && !lower.as_bytes()[at - 1].is_ascii_whitespace() {
            continue;
        }
        let rest = attrs[from..].trim_start();
        let Some(rest) = rest.strip_prefix('=') else { continue };
        let rest = rest.trim_start();
        let v = match rest.chars().next() {
            Some(q @ ('"' | '\'')) => rest[1..].split(q).next().unwrap_or(""),
            _ => rest.split(|c: char| c.is_whitespace() || c == '>').next().unwrap_or(""),
        };
        return Some(decode_entities(v));
    }
    None
}

fn parse_tag(s: &str) -> Tag<'_> {
    let closing = s.starts_with('/');
    let s = s.trim_start_matches('/');
    let end = s.find(|c: char| c.is_whitespace() || c == '/' || c == '>').unwrap_or(s.len());
    Tag { name: s[..end].to_ascii_lowercase(), closing, attrs: &s[end..] }
}

/// HTML to blocks. Image URLs are made absolute against `base`.
pub fn to_blocks(html: &str, base: &str) -> Vec<Block> {
    let mut b = Builder { blocks: Vec::new(), text: String::new(), kind: Kind::P, quote: 0, pre: false };
    let mut rest = html;
    let mut skip: Option<String> = None;
    while !rest.is_empty() {
        let Some(lt) = rest.find('<') else {
            if skip.is_none() {
                b.text.push_str(rest);
            }
            break;
        };
        if skip.is_none() {
            b.text.push_str(&rest[..lt]);
        }
        rest = &rest[lt..];
        if let Some(r) = rest.strip_prefix("<!--") {
            rest = r.find("-->").map_or("", |i| &r[i + 3..]);
            continue;
        }
        let Some(gt) = rest.find('>') else {
            // A lone "<" in text.
            if skip.is_none() {
                b.text.push('<');
            }
            rest = &rest[1..];
            continue;
        };
        let inner = &rest[1..gt];
        rest = &rest[gt + 1..];
        if inner.starts_with('!') || inner.starts_with('?') {
            continue;
        }
        let tag = parse_tag(inner);
        if let Some(s) = &skip {
            if tag.closing && &tag.name == s {
                skip = None;
            }
            continue;
        }
        let name = tag.name.as_str();
        if SKIP.contains(&name) {
            if !tag.closing && !inner.ends_with('/') {
                skip = Some(tag.name.clone());
            }
            continue;
        }
        match name {
            "br" => {
                if b.pre {
                    b.text.push('\n');
                } else {
                    b.flush();
                }
            }
            "img" if !tag.closing => {
                let src = attr(tag.attrs, "src").filter(|s| !s.starts_with("data:")).or_else(|| attr(tag.attrs, "data-src"));
                if let Some(url) = src.and_then(|s| resolve(base, &s)) {
                    // Skip tracking pixels and tiny icons when the size is given.
                    let tiny = |a: &str| attr(tag.attrs, a).and_then(|v| v.trim_end_matches("px").parse::<u32>().ok()).is_some_and(|n| n < 40);
                    if !tiny("width") && !tiny("height") {
                        b.flush();
                        b.blocks.push(Block::Img(url));
                    }
                }
            }
            "hr" => {
                b.flush();
                b.blocks.push(Block::Hr);
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                b.flush();
                b.kind = if tag.closing { Kind::P } else { Kind::H };
            }
            "li" => {
                b.flush();
                b.kind = if tag.closing { Kind::P } else { Kind::Li };
            }
            "blockquote" => {
                b.flush();
                b.quote = if tag.closing { b.quote.saturating_sub(1) } else { b.quote + 1 };
            }
            "pre" => {
                b.flush();
                b.pre = !tag.closing;
            }
            _ if BLOCKS.contains(&name) => {
                b.flush();
                if name == "ul" || name == "ol" {
                    b.kind = Kind::P;
                }
            }
            _ => {
                // Inline tags: keep words apart where tags separate them.
                if matches!(name, "td" | "th" | "span") && !b.text.ends_with(' ') {
                    b.text.push(' ');
                }
            }
        }
    }
    b.flush();
    b.blocks
}

/// Plain text of an HTML fragment (titles, summaries).
pub fn to_text(html: &str) -> String {
    if !html.contains('<') {
        return collapse(&clean_chars(&decode_entities(html)));
    }
    to_blocks(html, "")
        .into_iter()
        .filter_map(|b| match b {
            Block::P(t) | Block::H(t) | Block::Quote(t) | Block::Li(t) | Block::Code(t) => Some(t),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Make a link absolute ("/img/a.png" against "https://site/post").
pub fn resolve(base: &str, url: &str) -> Option<String> {
    let url = url.trim();
    if url.is_empty() {
        return None;
    }
    if url.starts_with("http://") || url.starts_with("https://") {
        return Some(url.to_string());
    }
    if let Some(r) = url.strip_prefix("//") {
        return Some(format!("https://{r}"));
    }
    let scheme_end = base.find("://")? + 3;
    let host_end = base[scheme_end..].find('/').map_or(base.len(), |i| scheme_end + i);
    if url.starts_with('/') {
        return Some(format!("{}{url}", &base[..host_end]));
    }
    let dir_end = base[host_end..].rfind('/').map_or(base.len(), |i| host_end + i + 1);
    let dir = if dir_end > base.len() { base.to_string() + "/" } else { base[..dir_end].to_string() };
    Some(format!("{}{url}", if dir.ends_with('/') { dir } else { dir + "/" }))
}

const ENTITIES: &[(&str, &str)] = &[
    ("amp", "&"), ("lt", "<"), ("gt", ">"), ("quot", "\""), ("apos", "'"), ("nbsp", " "), ("ndash", "–"), ("mdash", "—"),
    ("lsquo", "‘"), ("rsquo", "’"), ("sbquo", "‚"), ("ldquo", "“"), ("rdquo", "”"), ("bdquo", "„"), ("hellip", "…"),
    ("copy", "©"), ("reg", "®"), ("trade", "™"), ("laquo", "«"), ("raquo", "»"), ("middot", "·"), ("bull", "•"),
    ("deg", "°"), ("times", "×"), ("divide", "÷"), ("euro", "€"), ("pound", "£"), ("yen", "¥"), ("cent", "¢"),
    ("sect", "§"), ("para", "¶"), ("ordf", "ª"), ("ordm", "º"), ("iexcl", "¡"), ("iquest", "¿"), ("szlig", "ß"),
    ("rarr", "→"), ("larr", "←"), ("uarr", "↑"), ("darr", "↓"), ("harr", "↔"), ("minus", "−"), ("plusmn", "±"),
    ("frac12", "½"), ("frac14", "¼"), ("frac34", "¾"), ("prime", "′"), ("Prime", "″"), ("thinsp", " "), ("ensp", " "),
    ("emsp", " "), ("shy", ""), ("zwnj", ""), ("zwj", ""), ("lrm", ""), ("rlm", ""), ("micro", "µ"), ("aelig", "æ"),
    ("AElig", "Æ"), ("oslash", "ø"), ("Oslash", "Ø"), ("aring", "å"), ("Aring", "Å"), ("eth", "ð"), ("thorn", "þ"),
];

/// Accented letters: "eacute" → é, "Atilde" → Ã, "ccedil" → ç.
fn accented(name: &str) -> Option<char> {
    let (base, mark) = name.split_at(1);
    let b = base.chars().next()?;
    let table: &[(char, &str)] = match mark {
        "acute" => &[('a', "á"), ('e', "é"), ('i', "í"), ('o', "ó"), ('u', "ú"), ('y', "ý"), ('A', "Á"), ('E', "É"), ('I', "Í"), ('O', "Ó"), ('U', "Ú"), ('Y', "Ý")],
        "grave" => &[('a', "à"), ('e', "è"), ('i', "ì"), ('o', "ò"), ('u', "ù"), ('A', "À"), ('E', "È"), ('I', "Ì"), ('O', "Ò"), ('U', "Ù")],
        "circ" => &[('a', "â"), ('e', "ê"), ('i', "î"), ('o', "ô"), ('u', "û"), ('A', "Â"), ('E', "Ê"), ('I', "Î"), ('O', "Ô"), ('U', "Û")],
        "tilde" => &[('a', "ã"), ('o', "õ"), ('n', "ñ"), ('A', "Ã"), ('O', "Õ"), ('N', "Ñ")],
        "uml" => &[('a', "ä"), ('e', "ë"), ('i', "ï"), ('o', "ö"), ('u', "ü"), ('y', "ÿ"), ('A', "Ä"), ('E', "Ë"), ('I', "Ï"), ('O', "Ö"), ('U', "Ü")],
        "cedil" => &[('c', "ç"), ('C', "Ç")],
        _ => return None,
    };
    table.iter().find(|(c, _)| *c == b).and_then(|(_, s)| s.chars().next())
}

/// Decode `&amp;`, `&#39;`, `&#x2019;`, `&eacute;`… Unknown ones stay as-is.
pub fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let end = rest[1..].find(|c: char| !(c.is_ascii_alphanumeric() || c == '#')).map(|j| j + 1);
        let decoded = end.filter(|&e| e > 1 && e < 12 && rest[e..].starts_with(';')).and_then(|e| {
            let name = &rest[1..e];
            let ch = if let Some(num) = name.strip_prefix('#') {
                let n = if let Some(h) = num.strip_prefix('x').or_else(|| num.strip_prefix('X')) { u32::from_str_radix(h, 16).ok() } else { num.parse().ok() };
                n.and_then(char::from_u32).map(|c| c.to_string())
            } else {
                ENTITIES.iter().find(|(k, _)| *k == name).map(|(_, v)| v.to_string()).or_else(|| accented(name).map(String::from))
            };
            ch.map(|c| (c, e + 1))
        });
        match decoded {
            Some((c, len)) => {
                out.push_str(&c);
                rest = &rest[len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks() {
        let html = r#"<p>Hello <b>world</b> &amp; friends&#8217;s.</p><img src="/a.jpg" width="600"><h2>Title</h2>
            <ul><li>one</li><li>two &eacute;</li></ul><blockquote><p>quoted</p></blockquote><pre>let x = 1;
let y = 2;</pre><script>alert(1)</script><img src="pixel.gif" width="1" height="1"><p>after<br>break</p>"#;
        let b = to_blocks(html, "https://site.com/post/1");
        assert_eq!(
            b,
            vec![
                Block::P("Hello world & friends’s.".into()),
                Block::Img("https://site.com/a.jpg".into()),
                Block::H("Title".into()),
                Block::Li("one".into()),
                Block::Li("two é".into()),
                Block::Quote("quoted".into()),
                Block::Code("let x = 1;\nlet y = 2;".into()),
                Block::P("after".into()),
                Block::P("break".into()),
            ]
        );
    }

    #[test]
    fn text_and_urls() {
        assert_eq!(to_text("Rust &amp; Go &lt;3 &#x1F600; &unknown; caf&eacute; S&atilde;o"), "Rust & Go <3 &unknown; café São");
        assert_eq!(to_text("<a href='x'>Comments</a>"), "Comments");
        assert_eq!(resolve("https://a.com/x/y.html", "img/b.png").as_deref(), Some("https://a.com/x/img/b.png"));
        assert_eq!(resolve("https://a.com", "b.png").as_deref(), Some("https://a.com/b.png"));
        assert_eq!(resolve("x", "//cdn.com/c.jpg").as_deref(), Some("https://cdn.com/c.jpg"));
        assert_eq!(attr(r#" data-src="x.jpg" src='y.jpg'"#, "src").as_deref(), Some("y.jpg"));
    }
}
