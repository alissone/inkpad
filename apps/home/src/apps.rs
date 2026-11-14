//! The apps the launcher shows: installed apps (from their manifests, see
//! `inkpad::ui::apps`) plus the system entries.

use inkpad::ui::apps::{self as registry, AppManifest};
use inkpad::ui::handoff;

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Run an installed app.
    Launch(AppManifest),
    /// The built-in Settings screen.
    Settings,
    /// Hand over to KOReader; come back to home when it closes.
    KOReader,
    /// Leave inkpad and go back to the stock Kindle UI.
    Kindle,
}

#[derive(Clone, Debug)]
pub struct AppEntry {
    pub name: String,
    pub icon: String,
    pub action: Action,
}

impl AppEntry {
    pub fn icon_el<M>(&self, size: u32) -> inkpad::ui::El<M> {
        match &self.action {
            Action::Launch(m) => registry::app_icon(m, size),
            _ => inkpad::ui::icon(&self.icon, size),
        }
    }
}

/// Installed apps in name order, then the system entries.
pub fn list(show_dev: bool) -> Vec<AppEntry> {
    let mut v: Vec<AppEntry> = registry::installed()
        .into_iter()
        .filter(|a| show_dev || !a.dev)
        .map(|m| AppEntry { name: m.name.clone(), icon: m.icon.clone(), action: Action::Launch(m) })
        .collect();
    // In the simulator KOReader is listed (it explains itself when tapped).
    if handoff::koreader_available() || cfg!(not(target_arch = "arm")) {
        v.push(AppEntry { name: "KOReader".into(), icon: "book-open".into(), action: Action::KOReader });
    }
    v.push(AppEntry { name: "Settings".into(), icon: "settings".into(), action: Action::Settings });
    v.push(AppEntry { name: "Kindle".into(), icon: "book-marked".into(), action: Action::Kindle });
    v
}
