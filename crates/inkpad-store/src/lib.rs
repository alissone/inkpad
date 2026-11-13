//! Persistent storage for inkpad apps, in the spirit of Android's
//! SharedPreferences: small key/value files per namespace, plus plain text
//! files, all under one data folder.
//!
//! ```ignore
//! let mut prefs = Prefs::open("tasks");
//! let n = prefs.get_i64("launches", 0);
//! prefs.set("launches", n + 1); // written to disk right away
//! ```
//!
//! Data folder: `$INKPAD_DATA`, else `/mnt/us/inkpad/data` on the Kindle,
//! else `data/` in the workspace (development).
//!
//! Writes are atomic (temp file + rename), so a crash or power loss never
//! leaves a half-written file. Values are JSON, so the files are easy to
//! read and edit by hand over USB.

use serde_json::{Map, Value};
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub use serde_json::{json, Value as JsonValue};

/// Namespace for settings shared by the whole system (home screen, lock,
/// sleep screen). Apps may read it; Settings writes it.
pub const SYSTEM: &str = "system";

/// The root folder for all stored data. Created if missing.
pub fn data_dir() -> PathBuf {
    let dir = if let Ok(p) = std::env::var("INKPAD_DATA") {
        PathBuf::from(p)
    } else if cfg!(all(target_os = "linux", target_arch = "arm")) {
        PathBuf::from("/mnt/us/inkpad/data")
    } else {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data")
    };
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Write `bytes` to `path` atomically.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

fn safe_name(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() || "-_.".contains(c) { c } else { '_' }).collect()
}

/// Read a text file from the data folder (`name` may contain `/`).
pub fn read_text(name: &str) -> Option<String> {
    std::fs::read_to_string(data_dir().join(name)).ok()
}

/// Write a text file in the data folder atomically.
pub fn write_text(name: &str, text: &str) -> io::Result<()> {
    write_atomic(&data_dir().join(name), text.as_bytes())
}

/// Key/value preferences for one namespace, stored in
/// `<data>/prefs/<namespace>.json`.
pub struct Prefs {
    path: PathBuf,
    map: Map<String, Value>,
    mtime: Option<SystemTime>,
}

impl Prefs {
    pub fn open(namespace: &str) -> Prefs {
        Self::open_at(data_dir().join("prefs").join(format!("{}.json", safe_name(namespace))))
    }

    /// The shared system preferences.
    pub fn system() -> Prefs {
        Self::open(SYSTEM)
    }

    pub fn open_at(path: PathBuf) -> Prefs {
        let mut p = Prefs { path, map: Map::new(), mtime: None };
        p.reload();
        p
    }

    fn file_mtime(&self) -> Option<SystemTime> {
        std::fs::metadata(&self.path).and_then(|m| m.modified()).ok()
    }

    /// Re-read the file (another app may have changed it).
    pub fn reload(&mut self) {
        self.mtime = self.file_mtime();
        self.map = std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).map_err(|e| eprintln!("prefs {}: {e}", self.path.display())).ok())
            .and_then(|v| if let Value::Object(m) = v { Some(m) } else { None })
            .unwrap_or_default();
    }

    /// Reload only if the file changed on disk since we last read it.
    pub fn refresh(&mut self) -> bool {
        if self.file_mtime() != self.mtime {
            self.reload();
            true
        } else {
            false
        }
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.map.get(key)
    }
    pub fn contains(&self, key: &str) -> bool {
        self.map.contains_key(key)
    }
    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.map.get(key).and_then(Value::as_str)
    }
    pub fn get_string(&self, key: &str, default: &str) -> String {
        self.get_str(key).unwrap_or(default).to_string()
    }
    pub fn get_bool(&self, key: &str, default: bool) -> bool {
        self.map.get(key).and_then(Value::as_bool).unwrap_or(default)
    }
    pub fn get_i64(&self, key: &str, default: i64) -> i64 {
        self.map.get(key).and_then(Value::as_i64).unwrap_or(default)
    }
    pub fn get_f64(&self, key: &str, default: f64) -> f64 {
        self.map.get(key).and_then(Value::as_f64).unwrap_or(default)
    }
    pub fn get_list(&self, key: &str) -> Vec<String> {
        self.map.get(key).and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default()
    }
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.map.keys()
    }

    /// Set a value and save. Errors are logged (settings must never crash an app).
    pub fn set(&mut self, key: &str, value: impl Into<Value>) {
        self.map.insert(key.to_string(), value.into());
        self.log_save();
    }

    /// Set several values with one write.
    pub fn set_many<V: Into<Value>>(&mut self, pairs: impl IntoIterator<Item = (&'static str, V)>) {
        for (k, v) in pairs {
            self.map.insert(k.to_string(), v.into());
        }
        self.log_save();
    }

    pub fn remove(&mut self, key: &str) {
        if self.map.remove(key).is_some() {
            self.log_save();
        }
    }

    fn log_save(&mut self) {
        if let Err(e) = self.save() {
            eprintln!("prefs {}: save failed: {e}", self.path.display());
        }
    }

    pub fn save(&mut self) -> io::Result<()> {
        let text = serde_json::to_string_pretty(&Value::Object(self.map.clone()))?;
        write_atomic(&self.path, text.as_bytes())?;
        self.mtime = self.file_mtime();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let dir = std::env::temp_dir().join(format!("inkpad-store-test-{}", std::process::id()));
        let path = dir.join("t.json");
        let mut p = Prefs::open_at(path.clone());
        assert_eq!(p.get_i64("n", 7), 7);
        p.set("n", 3);
        p.set("name", "Ação");
        p.set("on", true);
        p.set("list", vec!["a", "b"]);
        let q = Prefs::open_at(path.clone());
        assert_eq!(q.get_i64("n", 0), 3);
        assert_eq!(q.get_str("name"), Some("Ação"));
        assert!(q.get_bool("on", false));
        assert_eq!(q.get_list("list"), vec!["a", "b"]);
        p.remove("n");
        assert!(!Prefs::open_at(path).contains("n"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn corrupt_file_is_empty() {
        let dir = std::env::temp_dir().join(format!("inkpad-store-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bad.json");
        std::fs::write(&path, "{not json").unwrap();
        assert_eq!(Prefs::open_at(path).get_i64("x", 1), 1);
        let _ = std::fs::remove_dir_all(dir);
    }
}
