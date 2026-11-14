//! Handing the device to another program (KOReader) and coming back.
//!
//! An inkpad app can't run KOReader itself: KOReader's launcher needs the
//! screen, which inkpad holds. Instead the app writes a request to the
//! handoff file and exits with [`EXIT_HANDOFF`]. Home passes that code on,
//! and the launcher script (`inkpad.sh`) gives the screen back, runs
//! KOReader, and when KOReader closes, starts home again, reopening the app
//! named in the request.
//!
//! File format, one item per line: the target (`koreader` or `kindle`), the
//! file to open (may be empty), the app id to return to (may be empty).
//!
//! `kindle` opens the file in the Kindle's own reader (needed for Amazon
//! KFX and DRM books). inkpad closes for good in that case: you're back in
//! the stock UI when you leave the book.

use crate::runtime::Ctx;
use std::path::{Path, PathBuf};

/// Exit code meaning "read the handoff file and do what it says".
pub const EXIT_HANDOFF: i32 = 42;

const KOREADER_SH: &str = "/mnt/us/koreader/koreader.sh";

pub fn handoff_file() -> PathBuf {
    if cfg!(all(target_os = "linux", target_arch = "arm")) {
        PathBuf::from("/var/tmp/inkpad-handoff")
    } else {
        std::env::temp_dir().join("inkpad-handoff")
    }
}

/// Whether KOReader is installed (always false in the simulator).
pub fn koreader_available() -> bool {
    cfg!(all(target_os = "linux", target_arch = "arm")) && Path::new(KOREADER_SH).exists()
}

/// Close inkpad, open `file` (or KOReader's file browser) in KOReader, and
/// come back to app `return_to` (or home) when KOReader exits.
pub fn open_in_koreader<M>(ctx: &mut Ctx<M>, file: Option<&Path>, return_to: Option<&str>) -> Result<(), String> {
    if !koreader_available() {
        return Err(if cfg!(target_arch = "arm") { "KOReader is not installed".into() } else { "KOReader only runs on the Kindle".into() });
    }
    write(ctx, "koreader", file, return_to)
}

/// Close inkpad and open `file` in the Kindle's own reader.
pub fn open_in_kindle_reader<M>(ctx: &mut Ctx<M>, file: &Path) -> Result<(), String> {
    if !cfg!(all(target_os = "linux", target_arch = "arm")) {
        return Err("The Kindle reader only runs on the Kindle".into());
    }
    write(ctx, "kindle", Some(file), None)
}

fn write<M>(ctx: &mut Ctx<M>, target: &str, file: Option<&Path>, return_to: Option<&str>) -> Result<(), String> {
    let file = file.map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
    if file.contains('\n') {
        return Err("file name contains a line break".into());
    }
    let body = format!("{target}\n{file}\n{}\n", return_to.unwrap_or(""));
    std::fs::write(handoff_file(), body).map_err(|e| e.to_string())?;
    ctx.exit(EXIT_HANDOFF);
    Ok(())
}
