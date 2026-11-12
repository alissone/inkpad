//! Local wall-clock time without external crates.
//!
//! The UTC offset comes from `INKPAD_TZ_OFFSET` (minutes) if set, otherwise
//! from the system: `date +%z` on Linux/Kindle, PowerShell on Windows. It is
//! re-read every hour so DST changes are picked up.

use std::cell::Cell;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalTime {
    pub year: i32,
    /// 1–12
    pub month: u32,
    /// 1–31
    pub day: u32,
    /// 0 = Monday … 6 = Sunday
    pub weekday: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    /// Offset from UTC in minutes.
    pub offset_min: i32,
}

const WEEKDAYS: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

impl LocalTime {
    /// "14:05"
    pub fn hm(&self) -> String {
        format!("{:02}:{:02}", self.hour, self.minute)
    }
    /// "2026-11-12"
    pub fn iso_date(&self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
    /// "Thursday, 12 November"
    pub fn long_date(&self) -> String {
        format!("{}, {} {}", self.weekday_name(), self.day, self.month_name())
    }
    pub fn weekday_name(&self) -> &'static str {
        WEEKDAYS[self.weekday as usize % 7]
    }
    pub fn month_name(&self) -> &'static str {
        MONTHS[(self.month as usize + 11) % 12]
    }
    /// Unix seconds of local midnight today (as UTC instant).
    pub fn midnight_unix(&self) -> i64 {
        days_from_civil(self.year, self.month, self.day) * 86400 - self.offset_min as i64 * 60
    }
}

/// Days since 1970-01-01 → (year, month, day). Howard Hinnant's algorithm.
pub fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    ((if m <= 2 { y + 1 } else { y }) as i32, m, d)
}

pub fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y as i64 - 1 } else { y as i64 };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = if m > 2 { m - 3 } else { m + 9 } as i64;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Convert Unix seconds + offset to local fields.
pub fn from_unix(secs: i64, offset_min: i32) -> LocalTime {
    let t = secs + offset_min as i64 * 60;
    let days = t.div_euclid(86400);
    let sod = t.rem_euclid(86400);
    let (year, month, day) = civil_from_days(days);
    LocalTime {
        year,
        month,
        day,
        weekday: (days + 3).rem_euclid(7) as u32, // 1970-01-01 was a Thursday
        hour: (sod / 3600) as u32,
        minute: (sod / 60 % 60) as u32,
        second: (sod % 60) as u32,
        offset_min,
    }
}

thread_local! {
    static OFFSET: Cell<Option<(i32, Instant)>> = const { Cell::new(None) };
}

#[cfg_attr(windows, allow(dead_code))]
fn parse_z(s: &str) -> Option<i32> {
    // "+0100", "-0300"
    let s = s.trim();
    if s.len() < 5 {
        return None;
    }
    let sign = match &s[..1] {
        "+" => 1,
        "-" => -1,
        _ => return None,
    };
    let h: i32 = s[1..3].parse().ok()?;
    let m: i32 = s[3..5].parse().ok()?;
    Some(sign * (h * 60 + m))
}

fn system_offset() -> i32 {
    if let Some(v) = std::env::var("INKPAD_TZ_OFFSET").ok().and_then(|v| v.trim().parse().ok()) {
        return v;
    }
    #[cfg(windows)]
    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", "[int][TimeZoneInfo]::Local.GetUtcOffset([DateTime]::Now).TotalMinutes"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok());
    #[cfg(not(windows))]
    let out = std::process::Command::new("date").arg("+%z").output().ok().and_then(|o| parse_z(&String::from_utf8_lossy(&o.stdout)));
    out.unwrap_or(0)
}

/// Current UTC offset in minutes (cached for an hour).
pub fn offset_minutes() -> i32 {
    OFFSET.with(|c| match c.get() {
        Some((v, at)) if at.elapsed() < Duration::from_secs(3600) => v,
        _ => {
            let v = system_offset();
            c.set(Some((v, Instant::now())));
            v
        }
    })
}

pub fn unix_now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// The current local time.
pub fn now() -> LocalTime {
    from_unix(unix_now(), offset_minutes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_dates() {
        // 2026-11-12 18:20:47 UTC
        let t = from_unix(1_794_507_647, 0);
        assert_eq!((t.year, t.month, t.day, t.hour, t.minute), (2026, 11, 12, 18, 20));
        assert_eq!(t.weekday_name(), "Thursday");
        let local = from_unix(1_794_507_647, -180);
        assert_eq!((local.day, local.hour, local.hm()), (12, 15, "15:20".to_string()));
        assert_eq!(from_unix(0, 0).weekday_name(), "Thursday");
        assert_eq!(from_unix(951_782_400, 0).iso_date(), "2000-02-29");
        for d in [-1000, 0, 11_000, 20_769] {
            let (y, m, dd) = civil_from_days(d);
            assert_eq!(days_from_civil(y, m, dd), d);
        }
        assert_eq!(local.midnight_unix(), 1_794_452_400); // 2026-11-12 00:00 at UTC-3
    }

    #[test]
    fn parses_offsets() {
        assert_eq!(parse_z("+0100\n"), Some(60));
        assert_eq!(parse_z("-0330"), Some(-210));
        assert_eq!(parse_z("UTC"), None);
    }
}
