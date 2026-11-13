//! The apps the launcher shows.

use inkpad::ui::app_installed;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Run an inkpad binary installed next to `home`.
    Launch(&'static str),
    /// The built-in Settings screen.
    Settings,
    /// Leave inkpad and start KOReader.
    KOReader,
    /// Leave inkpad and go back to the stock Kindle UI.
    Kindle,
}

#[derive(Clone, Copy, Debug)]
pub struct AppEntry {
    pub name: &'static str,
    pub icon: &'static str,
    pub action: Action,
    /// Developer tools, hidden unless enabled in Settings.
    pub dev: bool,
}

const KNOWN: &[AppEntry] = &[
    AppEntry { name: "Tasks", icon: "list-todo", action: Action::Launch("tasks"), dev: false },
    AppEntry { name: "Gallery", icon: "shapes", action: Action::Launch("gallery"), dev: false },
    AppEntry { name: "Touch test", icon: "pointer", action: Action::Launch("touchtest"), dev: true },
    AppEntry { name: "Graphics demo", icon: "palette", action: Action::Launch("gfxdemo"), dev: true },
];

pub const KOREADER_SH: &str = "/mnt/us/koreader/koreader.sh";
/// Exit code that tells `inkpad.sh` to start KOReader after restoring the screen.
pub const EXIT_KOREADER: i32 = 42;

pub fn koreader_installed() -> bool {
    std::path::Path::new(KOREADER_SH).exists() || cfg!(not(target_arch = "arm"))
}

/// Installed apps in launcher order, system entries last.
pub fn list(show_dev: bool) -> Vec<AppEntry> {
    let mut v: Vec<AppEntry> = KNOWN
        .iter()
        .filter(|a| show_dev || !a.dev)
        .filter(|a| match a.action {
            Action::Launch(bin) => app_installed(bin),
            _ => true,
        })
        .copied()
        .collect();
    if koreader_installed() {
        v.push(AppEntry { name: "KOReader", icon: "book-open", action: Action::KOReader, dev: false });
    }
    v.push(AppEntry { name: "Settings", icon: "settings", action: Action::Settings, dev: false });
    v.push(AppEntry { name: "Kindle", icon: "book-marked", action: Action::Kindle, dev: false });
    v
}
