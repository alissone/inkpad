//! Reading RSS 2.0, RSS 1.0 (RDF) and Atom feeds, including YouTube's and
//! Reddit's flavours, into plain items.

use crate::html;
use roxmltree::{Document, Node, ParsingOptions};

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub guid: String,
    pub title: String,
    pub link: String,
    /// The story's HTML (full content if the feed has it, else the summary).
    pub html: String,
    pub author: Option<String>,
    pub published: Option<i64>,
    /// A picture for the story (enclosure, media:thumbnail…), if any.
    pub image: Option<String>,
}

fn kids<'a, 'i>(n: Node<'a, 'i>, name: &'a str) -> impl Iterator<Item = Node<'a, 'i>> + 'a {
    n.children().filter(move |c| c.is_element() && c.tag_name().name() == name)
}

fn kid<'a, 'i>(n: Node<'a, 'i>, name: &'a str) -> Option<Node<'a, 'i>> {
    kids(n, name).next()
}

/// All text inside an element (CDATA included).
fn text_of(n: Node) -> String {
    n.descendants().filter(|d| d.is_text()).filter_map(|d| d.text()).collect::<String>().trim().to_string()
}

fn kid_text(n: Node, name: &str) -> Option<String> {
    kids(n, name).map(text_of).find(|t| !t.is_empty())
}

fn parse_item(n: Node) -> Item {
    let title = kid_text(n, "title").map(|t| html::to_text(&t)).unwrap_or_default();
    // Atom: <link rel="alternate" href="…"/>; RSS: <link>…</link>.
    let link = kids(n, "link")
        .find_map(|l| match l.attribute("href") {
            Some(h) if matches!(l.attribute("rel"), None | Some("alternate")) => Some(h.to_string()),
            _ => None,
        })
        .or_else(|| kid_text(n, "link"))
        .unwrap_or_default();
    let guid = kid_text(n, "guid").or_else(|| kid_text(n, "id")).unwrap_or_else(|| if link.is_empty() { title.clone() } else { link.clone() });
    let mut body = ["encoded", "content", "description", "summary"].iter().find_map(|k| kid_text(n, k));
    // YouTube: <media:group><media:description>.
    if body.is_none() {
        body = n.descendants().filter(|d| d.is_element() && d.tag_name().name() == "description").map(text_of).find(|t| !t.is_empty());
    }
    let mut body = body.unwrap_or_default();
    if !body.contains('<') {
        // Plain text (YouTube descriptions): keep its line breaks.
        body = body.split('\n').map(|l| format!("<p>{}</p>", l.replace('&', "&amp;").replace('<', "&lt;"))).collect();
    }
    let author = kid(n, "author")
        .map(|a| kid_text(a, "name").unwrap_or_else(|| text_of(a)))
        .or_else(|| kid_text(n, "creator"))
        .map(|a| html::to_text(&a))
        .filter(|a| !a.is_empty());
    let published = ["pubDate", "published", "updated", "date"].iter().find_map(|k| kid_text(n, k)).and_then(|d| parse_date(&d));
    let is_image = |m: &Node| m.attribute("type").is_some_and(|t| t.starts_with("image/")) || m.attribute("medium") == Some("image");
    let image = kids(n, "enclosure")
        .find(is_image)
        .and_then(|e| e.attribute("url"))
        .or_else(|| n.descendants().find(|d| d.is_element() && d.tag_name().name() == "thumbnail").and_then(|t| t.attribute("url")))
        .or_else(|| n.descendants().find(|d| d.is_element() && d.tag_name().name() == "content" && is_image(d)).and_then(|c| c.attribute("url")))
        .or_else(|| kid(n, "image").and_then(|i| i.attribute("href")))
        .map(String::from);
    Item { guid, title, link, html: body, author, published, image }
}

/// Parse a feed. Items come in the feed's order (usually newest first).
pub fn parse(xml: &str) -> Result<Vec<Item>, String> {
    let xml = xml.trim_start_matches('\u{feff}').trim_start();
    if !xml.starts_with('<') {
        return Err("not a feed (the site sent something else)".into());
    }
    let opts = ParsingOptions { allow_dtd: true, ..Default::default() };
    let doc = Document::parse_with_options(xml, opts).map_err(|e| format!("couldn't read the feed: {e}"))?;
    let items: Vec<Item> = doc.descendants().filter(|n| n.is_element() && matches!(n.tag_name().name(), "item" | "entry")).map(parse_item).collect();
    if items.is_empty() && !matches!(doc.root_element().tag_name().name(), "rss" | "feed" | "RDF") {
        return Err("not a feed (the site sent a web page)".into());
    }
    Ok(items)
}

// ---- dates ----

fn month(s: &str) -> Option<u32> {
    const M: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
    let l = s.get(..3)?.to_ascii_lowercase();
    M.iter().position(|m| *m == l).map(|i| i as u32 + 1)
}

/// Offset in minutes for "+0300", "-03:00", "Z", "GMT", "EST"…
fn zone(z: &str) -> Option<i64> {
    let z = z.trim();
    match z.to_ascii_uppercase().as_str() {
        "" | "Z" | "GMT" | "UT" | "UTC" => return Some(0),
        "EST" => return Some(-300),
        "EDT" => return Some(-240),
        "CST" => return Some(-360),
        "CDT" => return Some(-300),
        "MST" => return Some(-420),
        "MDT" => return Some(-360),
        "PST" => return Some(-480),
        "PDT" => return Some(-420),
        _ => {}
    }
    let sign = match z.chars().next()? {
        '+' => 1,
        '-' => -1,
        _ => return None,
    };
    let d: String = z[1..].chars().filter(char::is_ascii_digit).collect();
    if d.len() != 4 {
        return None;
    }
    Some(sign * (d[..2].parse::<i64>().ok()? * 60 + d[2..].parse::<i64>().ok()?))
}

fn unix(y: i32, mo: u32, d: u32, h: i64, mi: i64, s: i64, offset_min: i64) -> Option<i64> {
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || s > 60 {
        return None;
    }
    Some(inkpad::ui::clock::days_from_civil(y, mo, d) * 86400 + h * 3600 + mi * 60 + s - offset_min * 60)
}

/// RFC 822 ("Sat, 14 Nov 2026 18:00:00 +0000") or RFC 3339
/// ("2026-11-14T18:00:00Z") to Unix seconds.
pub fn parse_date(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.len() >= 10 && s.as_bytes()[4] == b'-' {
        let (y, mo, d) = (s[..4].parse().ok()?, s[5..7].parse().ok()?, s[8..10].parse().ok()?);
        if s.len() < 16 {
            return unix(y, mo, d, 0, 0, 0, 0);
        }
        let t = &s[11..];
        let (h, mi) = (t[..2].parse().ok()?, t[3..5].parse().ok()?);
        let sec = t.get(6..8).and_then(|x| x.parse().ok()).unwrap_or(0);
        let tz_at = t[5..].find(|c| c == 'Z' || c == '+' || c == '-').map(|i| i + 5);
        let off = tz_at.and_then(|i| zone(&t[i..])).unwrap_or(0);
        return unix(y, mo, d, h, mi, sec, off);
    }
    let s = s.split_once(',').map_or(s, |(_, r)| r).trim();
    let p: Vec<&str> = s.split_whitespace().collect();
    if p.len() < 4 {
        return None;
    }
    let d = p[0].parse().ok()?;
    let mo = month(p[1])?;
    let mut y: i32 = p[2].parse().ok()?;
    if y < 100 {
        y += 2000;
    }
    let t: Vec<&str> = p[3].split(':').collect();
    let (h, mi) = (t.first()?.parse().ok()?, t.get(1)?.parse().ok()?);
    let sec = t.get(2).and_then(|x| x.parse().ok()).unwrap_or(0);
    let off = p.get(4).and_then(|z| zone(z)).unwrap_or(0);
    unix(y, mo, d, h, mi, sec, off)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        let base = parse_date("2026-11-14T18:00:00Z").unwrap();
        assert_eq!(parse_date("Sat, 14 Nov 2026 18:00:00 +0000"), Some(base));
        assert_eq!(parse_date("Sat, 14 Nov 2026 15:00:00 -0300"), Some(base));
        assert_eq!(parse_date("14 Nov 2026 13:00:00 EST"), Some(base));
        assert_eq!(parse_date("2026-11-14T19:00:00.123+01:00"), Some(base));
        assert_eq!(parse_date("2026-11-14T18:00:00-00:00"), Some(base));
        assert_eq!(parse_date("Sat, 14 Nov 2026 18:00 GMT"), Some(base));
        assert_eq!(base, 1794679200);
        assert_eq!(parse_date("nonsense"), None);
    }

    #[test]
    fn rss() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:content="http://purl.org/rss/1.0/modules/content/" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:media="http://search.yahoo.com/mrss/">
<channel><title>Site</title>
<item><title>First &amp; best</title><link>https://site.com/1</link><guid isPermaLink="false">id-1</guid>
<description>Short</description><content:encoded><![CDATA[<p>Full <b>text</b></p>]]></content:encoded>
<dc:creator>Ana</dc:creator><pubDate>Sat, 14 Nov 2026 18:00:00 +0000</pubDate>
<media:thumbnail url="https://site.com/t.jpg"/></item>
<item><title>Second</title><link>https://site.com/2</link><description>&lt;p&gt;Escaped html&lt;/p&gt;</description>
<enclosure url="https://site.com/e.png" type="image/png" length="1"/></item>
</channel></rss>"#;
        let items = parse(xml).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "First & best");
        assert_eq!(items[0].guid, "id-1");
        assert_eq!(items[0].html, "<p>Full <b>text</b></p>");
        assert_eq!(items[0].author.as_deref(), Some("Ana"));
        assert_eq!(items[0].image.as_deref(), Some("https://site.com/t.jpg"));
        assert_eq!(items[0].published, Some(1794679200));
        assert_eq!(items[1].guid, "https://site.com/2");
        assert_eq!(items[1].html, "<p>Escaped html</p>");
        assert_eq!(items[1].image.as_deref(), Some("https://site.com/e.png"));
    }

    #[test]
    fn atom_and_youtube() {
        let xml = r#"<feed xmlns="http://www.w3.org/2005/Atom" xmlns:media="http://search.yahoo.com/mrss/" xmlns:yt="http://www.youtube.com/xml/schemas/2015">
<entry><id>yt:video:abc</id><title>Video</title><link rel="alternate" href="https://www.youtube.com/watch?v=abc"/>
<author><name>Channel</name></author><published>2026-11-14T18:00:00+00:00</published>
<media:group><media:thumbnail url="https://i.ytimg.com/vi/abc/hqdefault.jpg" width="480" height="360"/>
<media:description>Line one
Line two &amp; more</media:description></media:group></entry>
<entry><id>tag:x</id><title type="html">Post &lt;em&gt;one&lt;/em&gt;</title><link href="https://x.com/p"/><link rel="replies" href="https://x.com/c"/>
<content type="html">&lt;p&gt;Hi&lt;/p&gt;</content><updated>2026-11-13T10:00:00Z</updated></entry></feed>"#;
        let items = parse(xml).unwrap();
        assert_eq!(items[0].link, "https://www.youtube.com/watch?v=abc");
        assert_eq!(items[0].author.as_deref(), Some("Channel"));
        assert_eq!(items[0].image.as_deref(), Some("https://i.ytimg.com/vi/abc/hqdefault.jpg"));
        assert_eq!(items[0].html, "<p>Line one</p><p>Line two &amp; more</p>");
        assert_eq!(items[1].title, "Post one");
        assert_eq!(items[1].link, "https://x.com/p");
        assert!(parse("<html><body>blocked</body></html>").is_err());
    }
}
