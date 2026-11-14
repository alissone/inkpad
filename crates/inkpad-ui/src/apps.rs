//! Installed apps. Every app lives in its own folder under [`apps_dir`]:
//!
//! ```text
//! apps/library/
//!   app.json      manifest (below)
//!   library       the binary (library.exe in the simulator)
//!   assets/       optional: the app's own images, fonts, icon.svg
//! ```
//!
//! `app.json`:
//!
//! ```json
//! { "id": "library", "name": "Library", "exec": "library", "icon": "library-big",
//!   "version": "0.1.0", "description": "Your books", "dev": false }
//! ```
//!
//! `icon` is a bundled icon name ([`crate::icons::NAMES`]) or a file in the
//! app folder (`assets/icon.svg`). Apps are developed in their own
//! repositories and installed with inkpad's `scripts/install-app.ps1`.

use inkpad_store::serde_json::{self, Value};
use std::path::{Path, PathBuf};

/// Where apps are installed: `$INKPAD_APPS`, else `/mnt/us/inkpad/apps`
/// on the Kindle, else a per-user folder on the PC
/// (`%LOCALAPPDATA%\inkpad\apps`, `~/.local/share/inkpad/apps`).
pub fn apps_dir() -> PathBuf {
    if let Ok(p) = std::env::var("INKPAD_APPS") {
        return PathBuf::from(p);
    }
    if cfg!(all(target_os = "linux", target_arch = "arm")) {
        return PathBuf::from("/mnt/us/inkpad/apps");
    }
    if let Ok(p) = std::env::var("LOCALAPPDATA") {
        return Path::new(&p).join("inkpad").join("apps");
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    Path::new(&home).join(".local/share/inkpad/apps")
}

#[derive(Clone, Debug, PartialEq)]
pub struct AppManifest {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub description: String,
    pub version: String,
    /// Developer tools, hidden unless enabled in Settings.
    pub dev: bool,
    /// The app's folder.
    pub dir: PathBuf,
    /// Full path of the binary.
    pub exec: PathBuf,
}

impl AppManifest {
    /// Parse `app.json` found in `dir`.
    pub fn parse(json: &str, dir: &Path) -> Result<AppManifest, String> {
        let v: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
        let id = s("id").ok_or("missing \"id\"")?;
        let exec_name = s("exec").unwrap_or_else(|| id.clone());
        let mut exec = dir.join(&exec_name);
        let suffix = std::env::consts::EXE_SUFFIX;
        if !suffix.is_empty() && !exec_name.ends_with(suffix) {
            exec = dir.join(format!("{exec_name}{suffix}"));
        }
        Ok(AppManifest {
            name: s("name").unwrap_or_else(|| id.clone()),
            icon: s("icon").unwrap_or_else(|| "app-window".into()),
            description: s("description").unwrap_or_default(),
            version: s("version").unwrap_or_default(),
            dev: v.get("dev").and_then(Value::as_bool).unwrap_or(false),
            dir: dir.to_path_buf(),
            exec,
            id,
        })
    }

    /// Whether `icon` names a file in the app folder rather than a bundled icon.
    pub fn icon_file(&self) -> Option<PathBuf> {
        let l = self.icon.to_ascii_lowercase();
        (l.ends_with(".svg") || l.ends_with(".png")).then(|| self.dir.join(&self.icon))
    }
}

/// All installed apps whose binary exists, sorted by name.
pub fn installed() -> Vec<AppManifest> {
    let Ok(rd) = std::fs::read_dir(apps_dir()) else { return Vec::new() };
    let mut v: Vec<AppManifest> = rd
        .flatten()
        .filter_map(|e| {
            let dir = e.path();
            let json = std::fs::read_to_string(dir.join("app.json")).ok()?;
            match AppManifest::parse(&json, &dir) {
                Ok(m) if m.exec.exists() => Some(m),
                Ok(m) => {
                    eprintln!("apps: {} has no binary at {}", m.id, m.exec.display());
                    None
                }
                Err(e) => {
                    eprintln!("apps: {}/app.json: {e}", dir.display());
                    None
                }
            }
        })
        .collect();
    v.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    v
}

/// Find an installed app by id.
pub fn find(id: &str) -> Option<AppManifest> {
    installed().into_iter().find(|a| a.id == id)
}

/// The icon element for an app (bundled icon or the app's own file).
pub fn app_icon<M>(app: &AppManifest, size: u32) -> crate::El<M> {
    if let Some(f) = app.icon_file() {
        if let Some(img) = crate::assets::image(&f.to_string_lossy(), size, size, false) {
            return crate::image(img);
        }
    }
    if crate::icons::NAMES.contains(&app.icon.as_str()) {
        crate::icon(&app.icon, size)
    } else {
        crate::icon("app-window", size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_manifest() {
        let m = AppManifest::parse(r#"{"id":"library","name":"Library","icon":"library-big","dev":false}"#, Path::new("/apps/library")).unwrap();
        assert_eq!(m.id, "library");
        assert_eq!(m.name, "Library");
        assert!(m.exec.to_string_lossy().contains("library"));
        assert_eq!(m.icon_file(), None);
        let m2 = AppManifest::parse(r#"{"id":"x","icon":"assets/icon.svg"}"#, Path::new("/apps/x")).unwrap();
        assert_eq!(m2.name, "x");
        assert!(m2.icon_file().is_some());
        assert!(AppManifest::parse(r#"{"name":"no id"}"#, Path::new("/")).is_err());
    }
}
