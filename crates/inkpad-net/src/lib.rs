//! Networking for inkpad apps: HTTPS requests and Wi-Fi on demand.
//!
//! Requests go through the system's `curl` (in the Kindle firmware, and
//! built into Windows 10+). That keeps TLS out of our static binaries, which
//! would otherwise need a C cross-compiler for the crypto. On the Kindle a
//! current CA bundle (`inkpad/assets/certs/cacert.pem`, from Mozilla) is
//! passed along, so an old certificate store can't break HTTPS.
//!
//! Everything sensitive (headers with tokens, request bodies) is written to
//! a private temporary curl config file, never put on the command line where
//! other processes could see it.
//!
//! ```ignore
//! let r = Request::get("https://example.com/api").header("Accept", "application/json").send()?;
//! if r.ok() { println!("{}", r.body) }
//! ```

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct Request {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
    pub timeout: Duration,
}

#[derive(Clone, Debug)]
pub struct Response {
    pub status: u16,
    pub body: String,
}

impl Response {
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum NetError {
    /// No network / host unreachable / DNS failed.
    Offline(String),
    /// TLS handshake or certificate problem (often a wrong clock).
    Tls(String),
    Timeout,
    /// curl missing or broken.
    Tool(String),
}

impl std::fmt::Display for NetError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            NetError::Offline(d) => write!(f, "No connection ({d})"),
            NetError::Tls(d) => write!(f, "Secure connection failed ({d}). Is the date right?"),
            NetError::Timeout => write!(f, "The server took too long to answer"),
            NetError::Tool(d) => write!(f, "{d}"),
        }
    }
}

impl Request {
    pub fn new(method: &str, url: &str) -> Request {
        Request { method: method.into(), url: url.into(), headers: Vec::new(), body: None, timeout: Duration::from_secs(30) }
    }
    pub fn get(url: &str) -> Request {
        Self::new("GET", url)
    }
    pub fn post(url: &str) -> Request {
        Self::new("POST", url)
    }
    pub fn patch(url: &str) -> Request {
        Self::new("PATCH", url)
    }
    pub fn header(mut self, k: &str, v: &str) -> Request {
        self.headers.push((k.into(), v.into()));
        self
    }
    pub fn body(mut self, b: impl Into<String>) -> Request {
        self.body = Some(b.into());
        self
    }
    pub fn json(self, b: impl Into<String>) -> Request {
        self.header("Content-Type", "application/json").body(b)
    }
    pub fn timeout(mut self, d: Duration) -> Request {
        self.timeout = d;
        self
    }
    pub fn send(&self) -> Result<Response, NetError> {
        send(self)
    }
}

/// Quote a string for a curl config file.
fn cfg_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn tmp_dir() -> PathBuf {
    if cfg!(all(target_os = "linux", target_arch = "arm")) {
        PathBuf::from("/var/tmp")
    } else {
        std::env::temp_dir()
    }
}

/// A CA bundle to use, if one is installed (Kindle only; Windows uses its own store).
pub fn ca_bundle() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("INKPAD_CACERT") {
        return Some(PathBuf::from(p));
    }
    if !cfg!(target_os = "linux") {
        return None;
    }
    let mut candidates = vec![PathBuf::from("/mnt/us/inkpad/assets/certs/cacert.pem"), PathBuf::from("/mnt/us/koreader/data/ca-bundle.crt")];
    if let Some(d) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        candidates.insert(0, d.join("assets/certs/cacert.pem"));
    }
    candidates.into_iter().find(|p| p.exists())
}

fn write_private(path: &Path, data: &[u8]) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path)?;
        f.write_all(data)
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, data)
    }
}

static SEQ: AtomicU32 = AtomicU32::new(0);

/// Removes the temporary files when dropped.
struct TempFiles(Vec<PathBuf>);
impl Drop for TempFiles {
    fn drop(&mut self) {
        for p in &self.0 {
            let _ = std::fs::remove_file(p);
        }
    }
}

pub fn send(req: &Request) -> Result<Response, NetError> {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let base = tmp_dir().join(format!("inkpad-curl-{}-{n}", std::process::id()));
    let cfg_path = base.with_extension("cfg");
    let body_path = base.with_extension("body");
    let _cleanup = TempFiles(vec![cfg_path.clone(), body_path.clone()]);

    let mut cfg = String::new();
    cfg.push_str(&format!("url = {}\n", cfg_quote(&req.url)));
    cfg.push_str(&format!("request = {}\n", cfg_quote(&req.method)));
    for (k, v) in &req.headers {
        cfg.push_str(&format!("header = {}\n", cfg_quote(&format!("{k}: {v}"))));
    }
    if let Some(b) = &req.body {
        write_private(&body_path, b.as_bytes()).map_err(|e| NetError::Tool(format!("temp file: {e}")))?;
        cfg.push_str(&format!("data-binary = {}\n", cfg_quote(&format!("@{}", body_path.display()))));
    }
    cfg.push_str("silent\nshow-error\n");
    cfg.push_str(&format!("max-time = {}\nconnect-timeout = 15\n", req.timeout.as_secs().max(5)));
    cfg.push_str(&format!("write-out = {}\n", cfg_quote("\n%{http_code}")));
    if let Some(ca) = ca_bundle() {
        cfg.push_str(&format!("cacert = {}\n", cfg_quote(&ca.display().to_string())));
    }
    write_private(&cfg_path, cfg.as_bytes()).map_err(|e| NetError::Tool(format!("temp file: {e}")))?;

    let out = Command::new("curl").arg("-K").arg(&cfg_path).output().map_err(|e| NetError::Tool(format!("curl not available: {e}")))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(match out.status.code() {
            Some(5 | 6 | 7) => NetError::Offline(err),
            Some(28) => NetError::Timeout,
            Some(35 | 51 | 53 | 54 | 58 | 59 | 60 | 77 | 80 | 82 | 83 | 90 | 91) => NetError::Tls(err),
            c => NetError::Tool(format!("curl failed ({}): {err}", c.unwrap_or(-1))),
        });
    }
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let (body, code) = text.rsplit_once('\n').unwrap_or(("", text.as_str()));
    let status = code.trim().parse().map_err(|_| NetError::Tool(format!("unexpected curl output: {code:?}")))?;
    Ok(Response { status, body: body.to_string() })
}

/// Percent-encode for URLs, keeping `safe` characters (plus unreserved).
pub fn urlencode(s: &str, safe: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        let c = b as char;
        if c.is_ascii_alphanumeric() || "-._~".contains(c) || safe.contains(c) {
            out.push(c);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Whether the system clock is believable. TLS certificates can't be
/// checked with a clock that's years off.
pub fn clock_plausible() -> bool {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() > 1_735_689_600).unwrap_or(false) // 2025-01-01
}

// ---- Wi-Fi ----------------------------------------------------------------

/// Whether the device has a working network connection (Wi-Fi associated
/// with an IP). Always true in the simulator.
pub fn connected() -> bool {
    #[cfg(all(target_os = "linux", target_arch = "arm"))]
    {
        let out = Command::new("lipc-get-prop").args(["com.lab126.wifid", "cmState"]).output();
        out.map(|o| String::from_utf8_lossy(&o.stdout).trim() == "CONNECTED").unwrap_or(false)
    }
    #[cfg(not(all(target_os = "linux", target_arch = "arm")))]
    {
        true
    }
}

/// Make sure we're online, switching Wi-Fi on and waiting up to `wait` for
/// it to connect. Returns `true` if we switched it on (so the caller can
/// switch it off again when done).
pub fn ensure_online(wait: Duration) -> Result<bool, NetError> {
    if connected() {
        return Ok(false);
    }
    let was_on = inkpad_hal::system::wifi_on().unwrap_or(false);
    if !was_on {
        inkpad_hal::system::set_wifi(true).map_err(|e| NetError::Tool(format!("couldn't turn Wi-Fi on: {e}")))?;
    }
    let start = Instant::now();
    while start.elapsed() < wait {
        std::thread::sleep(Duration::from_millis(500));
        if connected() {
            // The interface reports connected slightly before DNS works.
            std::thread::sleep(Duration::from_millis(800));
            return Ok(!was_on);
        }
    }
    if !was_on {
        let _ = inkpad_hal::system::set_wifi(false);
    }
    Err(NetError::Offline("Wi-Fi didn't connect; is a network saved in the Kindle's settings?".into()))
}

/// Switch Wi-Fi off (after [`ensure_online`] turned it on).
pub fn wifi_off() {
    let _ = inkpad_hal::system::set_wifi(false);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_and_encoding() {
        assert_eq!(cfg_quote("a\"b\\c\n"), "\"a\\\"b\\\\c\\n\"");
        assert_eq!(urlencode("(is_completed.eq.false,x)", "(),.:"), "(is_completed.eq.false,x)");
        assert_eq!(urlencode("a b&c", ""), "a%20b%26c");
        assert!(clock_plausible());
    }
}
