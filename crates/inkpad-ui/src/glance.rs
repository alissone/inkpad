//! Glances: small summaries apps publish for the system to show on the home
//! screen and the sleep screen (e.g. Tasks: "3 open" and the next tasks).
//! The app doesn't need to be running; the last published glance stays.
//!
//! Stored as `<data>/glance/<id>.json`.

use inkpad_store::serde_json::{self, json, Value};

#[derive(Clone, Debug, PartialEq)]
pub struct Glance {
    /// The publishing app's id; also used to open it from home.
    pub id: String,
    pub title: String,
    /// A bundled icon name.
    pub icon: String,
    /// Short summary, e.g. "3 open · synced 14:02".
    pub note: String,
    /// The items, most important first.
    pub lines: Vec<String>,
    /// Unix seconds.
    pub updated: i64,
}

fn dir() -> std::path::PathBuf {
    inkpad_store::data_dir().join("glance")
}

fn safe(id: &str) -> String {
    id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect()
}

/// Publish (replace) this app's glance.
pub fn publish(g: &Glance) {
    let v = json!({ "id": g.id, "title": g.title, "icon": g.icon, "note": g.note, "lines": g.lines, "updated": g.updated });
    let path = dir().join(format!("{}.json", safe(&g.id)));
    if let Err(e) = inkpad_store::write_atomic(&path, v.to_string().as_bytes()) {
        eprintln!("glance {}: {e}", g.id);
    }
}

/// Remove this app's glance (e.g. after signing out).
pub fn remove(id: &str) {
    let _ = std::fs::remove_file(dir().join(format!("{}.json", safe(id))));
}

/// All published glances, by title.
pub fn all() -> Vec<Glance> {
    let Ok(rd) = std::fs::read_dir(dir()) else { return Vec::new() };
    let mut v: Vec<Glance> = rd
        .flatten()
        .filter_map(|e| {
            let v: Value = serde_json::from_str(&std::fs::read_to_string(e.path()).ok()?).ok()?;
            let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
            Some(Glance {
                id: s("id"),
                title: s("title"),
                icon: s("icon"),
                note: s("note"),
                lines: v.get("lines").and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default(),
                updated: v.get("updated").and_then(Value::as_i64).unwrap_or(0),
            })
        })
        .filter(|g| !g.id.is_empty())
        .collect();
    v.sort_by(|a, b| a.title.cmp(&b.title));
    v
}

/// A card showing a glance: icon, title, note, then up to `max_lines` items.
pub fn glance_card<M>(g: &Glance, max_lines: usize, text_size: u16) -> crate::El<M> {
    use crate::el::*;
    let icon_name = if crate::icons::NAMES.contains(&g.icon.as_str()) { g.icon.as_str() } else { "app-window" };
    let mut kids = vec![row(vec![icon(icon_name, 24), text(g.title.clone()).size(20).bold().fill_w(), text(g.note.clone()).size(15).color(80).max_lines(1)]).gap(10)];
    for l in g.lines.iter().take(max_lines) {
        kids.push(row(vec![canvas(10, 10, |p, r| p.circle(r.x as i32 + 4, r.y as i32 + 5, 3, inkpad_gfx::BLACK, None)), text(l.clone()).size(text_size).max_lines(1).fill_w()]).gap(10));
    }
    if g.lines.len() > max_lines {
        kids.push(text(format!("+ {} more", g.lines.len() - max_lines)).size(text_size - 2).color(90).pad(20, 0));
    }
    if g.lines.is_empty() {
        kids.push(text("Nothing here.").size(text_size).color(90));
    }
    col(kids).gap(6).padding(14).border(2.0, inkpad_gfx::BLACK).radius(16.0).bg(inkpad_gfx::WHITE)
}
