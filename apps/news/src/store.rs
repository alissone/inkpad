//! Everything kept on the Kindle for offline reading, under
//! `<data>/news/`:
//!
//! - `index.json`: the stories (title, feed, date, read or not…), which
//!   stories you deleted (so fetching doesn't bring them back) and when each
//!   feed was last fetched.
//! - `stories/<id>.json`: a story's text, as blocks.
//! - `img/<name>.png`: its pictures (grayscale, already sized for the
//!   screen) and `-t.png` thumbnails.

use crate::html::Block;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Story {
    pub id: String,
    pub feed: String,
    pub guid: String,
    pub title: String,
    pub link: String,
    #[serde(default)]
    pub author: Option<String>,
    /// Unix seconds (the fetch time if the feed gave no date).
    pub published: i64,
    pub fetched: i64,
    /// A line of text for the list.
    #[serde(default)]
    pub summary: String,
    /// Thumbnail file in `img/`.
    #[serde(default)]
    pub thumb: Option<String>,
    #[serde(default)]
    pub read: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FeedState {
    pub fetched_at: Option<i64>,
    pub error: Option<String>,
}

#[derive(Default, Serialize, Deserialize)]
pub struct Index {
    pub stories: Vec<Story>,
    /// Per feed: guids of deleted stories, newest last.
    #[serde(default)]
    pub deleted: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub feeds: BTreeMap<String, FeedState>,
    #[serde(default)]
    pub last_fetch: Option<i64>,
}

/// Deleted guids remembered per feed (feeds rarely keep more items).
const DELETED_KEEP: usize = 300;

pub fn dir() -> PathBuf {
    inkpad::ui::store::data_dir().join("news")
}
pub fn img_dir() -> PathBuf {
    dir().join("img")
}
fn story_path(id: &str) -> PathBuf {
    dir().join("stories").join(format!("{id}.json"))
}

/// FNV-1a: short stable ids for stories and image files.
pub fn hash(s: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

pub fn story_id(feed: &str, guid: &str) -> String {
    hash(&format!("{feed}\n{guid}"))
}

impl Index {
    pub fn load() -> Index {
        inkpad::ui::store::read_text("news/index.json").and_then(|s| serde_json::from_str(&s).map_err(|e| eprintln!("news: index: {e}")).ok()).unwrap_or_default()
    }

    pub fn save(&self) {
        match serde_json::to_string(self) {
            Ok(s) => {
                if let Err(e) = inkpad::ui::store::write_text("news/index.json", &s) {
                    eprintln!("news: saving index: {e}");
                }
            }
            Err(e) => eprintln!("news: {e}"),
        }
    }

    /// Newest first.
    pub fn sort(&mut self) {
        self.stories.sort_by(|a, b| b.published.cmp(&a.published).then(b.fetched.cmp(&a.fetched)));
    }

    /// Guids a fetch should skip for this feed: stored or deleted.
    pub fn known(&self, feed: &str) -> HashSet<String> {
        let mut s: HashSet<String> = self.stories.iter().filter(|x| x.feed == feed).map(|x| x.guid.clone()).collect();
        if let Some(d) = self.deleted.get(feed) {
            s.extend(d.iter().cloned());
        }
        s
    }

    pub fn count(&self, feed: &str) -> usize {
        self.stories.iter().filter(|s| s.feed == feed).count()
    }

    pub fn get(&self, id: &str) -> Option<&Story> {
        self.stories.iter().find(|s| s.id == id)
    }

    /// Delete a story and its files; remember it so it isn't fetched again.
    pub fn delete(&mut self, id: &str) {
        let Some(i) = self.stories.iter().position(|s| s.id == id) else { return };
        let s = self.stories.remove(i);
        remove_files(&s);
        let d = self.deleted.entry(s.feed).or_default();
        d.push(s.guid);
        if d.len() > DELETED_KEEP {
            d.drain(..d.len() - DELETED_KEEP);
        }
    }

    pub fn delete_feed(&mut self, feed: &str) -> usize {
        let ids: Vec<String> = self.stories.iter().filter(|s| s.feed == feed).map(|s| s.id.clone()).collect();
        for id in &ids {
            self.delete(id);
        }
        ids.len()
    }
}

pub fn save_body(id: &str, blocks: &[Block]) -> std::io::Result<()> {
    let s = serde_json::to_string(blocks).map_err(std::io::Error::other)?;
    inkpad::ui::store::write_atomic(&story_path(id), s.as_bytes())
}

pub fn load_body(id: &str) -> Vec<Block> {
    std::fs::read_to_string(story_path(id)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn remove_files(s: &Story) {
    for b in load_body(&s.id) {
        if let Block::Img(f) = b {
            let _ = std::fs::remove_file(img_dir().join(f));
        }
    }
    if let Some(t) = &s.thumb {
        let _ = std::fs::remove_file(img_dir().join(t));
    }
    let _ = std::fs::remove_file(story_path(&s.id));
}

/// Bytes used by stories and pictures.
pub fn disk_usage() -> u64 {
    let size = |d: PathBuf| std::fs::read_dir(d).map(|r| r.flatten().filter_map(|e| e.metadata().ok()).map(|m| m.len()).sum()).unwrap_or(0);
    size(dir().join("stories")) + size(img_dir())
}
