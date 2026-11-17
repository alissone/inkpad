//! Downloading: feeds, new stories and their pictures. Runs on background
//! threads; everything a story needs is saved so it reads offline.

use crate::feeds::Feed;
use crate::html::{self, Block};
use crate::store::{self, Story};
use crate::feed;
use inkpad::gfx::GrayImage;
use inkpad::net::{self, Request};
use std::collections::HashSet;
use std::sync::Mutex;
use std::time::Duration;

/// Some sites refuse requests without a browser-like User-Agent.
const UA: &str = "Mozilla/5.0 (X11; Linux armv7l) inkpad-news/0.1 (offline e-reader)";
/// Pictures kept per story.
const MAX_IMAGES: usize = 4;
/// Pictures are stored at the reader's text width.
pub const IMG_W: u32 = 544;
/// Short enough that a lead picture fits on the first page under the title.
pub const IMG_MAX_H: u32 = 440;
pub const THUMB: u32 = 92;
/// Downloads at once.
const PARALLEL: usize = 4;

#[derive(Clone, Debug)]
pub struct FeedResult {
    pub feed: String,
    pub stories: Vec<Story>,
    pub error: Option<String>,
    pub at: i64,
}

fn now() -> i64 {
    inkpad::ui::clock::unix_now()
}

/// Get online if needed (`Ok(true)` = we switched Wi-Fi on).
pub fn go_online() -> Result<bool, String> {
    if std::env::var_os("INKPAD_NEWS_OFFLINE").is_some() {
        return Err("No Wi-Fi connection".into());
    }
    if !net::clock_plausible() {
        return Err("The Kindle's clock is wrong, so secure connections fail. Set the date in the Kindle's settings.".into());
    }
    net::ensure_online(Duration::from_secs(30)).map_err(|e| e.to_string())
}

fn get(url: &str, accept: &str, max: u64) -> Result<Vec<u8>, String> {
    // Reddit rate-limits (429), YouTube's feeds fail at random (404/500)
    // and some servers are just slow: try a few times, waiting longer each
    // time (Reddit much longer).
    let flaky = url.contains("youtube.com/feeds");
    let pause = if url.contains("reddit.com") { 15 } else { 4 };
    let mut attempt = 0;
    let r = loop {
        attempt += 1;
        let r = Request::get(url).header("User-Agent", UA).header("Accept", accept).follow_redirects().max_bytes(max).timeout(Duration::from_secs(40)).send();
        let retry = match &r {
            Ok(r) => r.status == 429 || (500..=504).contains(&r.status) || (flaky && r.status == 404),
            Err(e) => *e == net::NetError::Timeout,
        };
        if !retry || attempt >= 3 {
            break r.map_err(|e| e.to_string())?;
        }
        std::thread::sleep(Duration::from_secs(pause * attempt as u64));
    };
    match r.status {
        200..=299 => Ok(r.bytes),
        429 => Err("the site says too many requests; try later".into()),
        403 => Err("the site refused the request (HTTP 403)".into()),
        404 => Err("feed not found (HTTP 404)".into()),
        s => Err(format!("HTTP {s}")),
    }
}

/// Download a picture, store it sized for the screen (and a thumbnail).
/// Returns the file names, or `None` for formats we can't show (GIF),
/// tiny icons and failures.
fn get_image(url: &str, story_id: &str) -> Option<(String, String)> {
    // No image/*: servers would send AVIF or GIF, which we can't decode.
    let bytes = get(url, "image/jpeg,image/png,image/webp;q=0.9", 8 << 20).map_err(|e| eprintln!("news: image {url}: {e}")).ok()?;
    let mut img = decode(&bytes).map_err(|e| eprintln!("news: image {url}: {e}")).ok()?;
    if img.width < 80 || img.height < 60 {
        return None;
    }
    // Transparent pictures go on white paper.
    if let Some(a) = img.alpha.take() {
        for (p, a) in img.pixels.iter_mut().zip(a) {
            *p = ((*p as u32 * a as u32 + 255 * (255 - a as u32)) / 255) as u8;
        }
    }
    if img.width > IMG_W || img.height > IMG_MAX_H {
        img = img.fit(IMG_W, IMG_MAX_H);
    }
    let name = store::hash(&format!("{story_id}\n{url}"));
    let dir = store::img_dir();
    let (file, thumb) = (format!("{name}.png"), format!("{name}-t.png"));
    let write = |f: &str, i: &GrayImage| i.to_png().ok().and_then(|png| inkpad::ui::store::write_atomic(&dir.join(f), &png).ok());
    write(&file, &img)?;
    write(&thumb, &img.cover(THUMB, THUMB))?;
    Some((file, thumb))
}

/// JPEG, PNG, SVG (inkpad) or WebP (image-webp).
fn decode(bytes: &[u8]) -> Result<GrayImage, String> {
    if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        let mut d = image_webp::WebPDecoder::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
        let (w, h) = d.dimensions();
        let ch = if d.has_alpha() { 4 } else { 3 };
        let mut buf = vec![0u8; d.output_buffer_size().ok_or("WebP too big")?];
        d.read_image(&mut buf).map_err(|e| e.to_string())?;
        let luma = |c: &[u8]| ((c[0] as u32 * 299 + c[1] as u32 * 587 + c[2] as u32 * 114) / 1000) as u8;
        let pixels = buf.chunks_exact(ch).map(luma).collect();
        let alpha = (ch == 4).then(|| buf.chunks_exact(4).map(|c| c[3]).collect());
        return Ok(GrayImage { width: w, height: h, pixels, alpha });
    }
    // The JPEG decoder shrinks while *either* side stays above the request,
    // so ask for an impossible height to make the width decide.
    GrayImage::from_bytes_for(bytes, IMG_W, u16::MAX as u32)
}

fn summary(blocks: &[Block]) -> String {
    let text = blocks.iter().find_map(|b| match b {
        Block::P(t) | Block::Quote(t) | Block::Li(t) if t.chars().count() > 3 => Some(t.as_str()),
        _ => None,
    });
    let t = text.unwrap_or("");
    if t.chars().count() > 180 {
        format!("{}…", t.chars().take(180).collect::<String>().trim_end())
    } else {
        t.to_string()
    }
}

/// Fetch one feed and save up to `want` stories it has that we don't
/// (skipping deleted ones). Runs on a background thread.
pub fn fetch_feed(feed: &Feed, known: HashSet<String>, want: usize, images: bool) -> FeedResult {
    let id = feed.id();
    let mut out = FeedResult { feed: id.clone(), stories: Vec::new(), error: None, at: now() };
    let xml = match get(&feed.url, "application/rss+xml, application/atom+xml, application/xml;q=0.9, text/xml;q=0.9, */*;q=0.5", 10 << 20) {
        Ok(b) => String::from_utf8_lossy(&b).into_owned(),
        Err(e) => {
            out.error = Some(e);
            return out;
        }
    };
    let items = match feed::parse(&xml) {
        Ok(i) => i,
        Err(e) => {
            out.error = Some(e);
            return out;
        }
    };
    let fetched = now();
    let new: Vec<(usize, feed::Item)> = items.into_iter().enumerate().filter(|(_, it)| !known.contains(&it.guid)).take(want).collect();

    // Text first, collecting the pictures to download.
    let mut stories: Vec<(Story, Vec<Block>)> = Vec::new();
    let mut jobs: Vec<(usize, usize, String)> = Vec::new(); // (story, block, url)
    for (order, it) in new {
        let sid = store::story_id(&id, &it.guid);
        let base = if it.link.is_empty() { feed.url.as_str() } else { it.link.as_str() };
        let mut blocks = html::to_blocks(&it.html, base);
        if let Some(img) = it.image.as_ref().and_then(|u| html::resolve(base, u)) {
            if !blocks.iter().any(|b| matches!(b, Block::Img(_))) {
                blocks.insert(0, Block::Img(img));
            }
        }
        // Keep the first few distinct pictures (or none).
        let mut seen = HashSet::new();
        blocks.retain(|b| match b {
            Block::Img(u) => images && seen.len() < MAX_IMAGES && seen.insert(u.clone()),
            _ => true,
        });
        let si = stories.len();
        for (bi, b) in blocks.iter().enumerate() {
            if let Block::Img(u) = b {
                jobs.push((si, bi, u.clone()));
            }
        }
        let story = Story {
            id: sid,
            feed: id.clone(),
            guid: it.guid.clone(),
            title: if it.title.is_empty() { "(untitled)".into() } else { it.title.clone() },
            link: it.link.clone(),
            author: it.author.clone(),
            // Undated items keep the feed's order.
            published: it.published.unwrap_or(fetched - order as i64),
            fetched,
            summary: summary(&blocks),
            thumb: None,
            read: false,
        };
        stories.push((story, blocks));
    }

    // Pictures, a few at a time.
    let results: Mutex<Vec<(usize, usize, Option<(String, String)>)>> = Mutex::new(Vec::new());
    let ids: Vec<String> = stories.iter().map(|(s, _)| s.id.clone()).collect();
    std::thread::scope(|scope| {
        for chunk in 0..PARALLEL {
            let (jobs, results, ids) = (&jobs, &results, &ids);
            scope.spawn(move || {
                for (si, bi, url) in jobs.iter().skip(chunk).step_by(PARALLEL) {
                    let r = get_image(url, &ids[*si]);
                    results.lock().unwrap().push((*si, *bi, r));
                }
            });
        }
    });
    let mut results = results.into_inner().unwrap();
    results.sort_by_key(|(si, bi, _)| (*si, *bi));
    let mut drop_blocks: Vec<(usize, usize)> = Vec::new();
    for (si, bi, r) in results {
        let (story, blocks) = &mut stories[si];
        match r {
            Some((file, thumb)) => {
                blocks[bi] = Block::Img(file);
                if story.thumb.is_none() {
                    story.thumb = Some(thumb);
                } else {
                    let _ = std::fs::remove_file(store::img_dir().join(thumb));
                }
            }
            None => drop_blocks.push((si, bi)),
        }
    }
    for (si, bi) in drop_blocks.into_iter().rev() {
        stories[si].1.remove(bi);
    }

    for (story, blocks) in stories {
        if let Err(e) = store::save_body(&story.id, &blocks) {
            eprintln!("news: saving {}: {e}", story.id);
            continue;
        }
        out.stories.push(story);
    }
    out.at = now();
    out
}

