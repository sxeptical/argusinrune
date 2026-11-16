//! Small timestamp helpers replacing JavaScript's `Date.parse` / `toISOString`.
//!
//! Only the two shapes Argus actually sees are supported: ISO 8601 (with an
//! optional `Z` / `±HH:MM` offset, optional fractional seconds) and RFC 2822
//! (RSS `pubDate`, e.g. `Mon, 01 Jan 2026 00:00:00 GMT`). No `chrono` dependency
//! is needed for these fixed formats.

use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = y - if m <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = m + if m > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    (y + if m <= 2 { 1 } else { 0 }, m, d)
}

/// Format epoch milliseconds as an ISO-8601 UTC string (`Date.toISOString()`).
pub fn iso_from_millis(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let millis = ms.rem_euclid(1000);
    let days = secs.div_euclid(86_400);
    let sod = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    let hh = sod / 3600;
    let mm = (sod % 3600) / 60;
    let ss = sod % 60;
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}.{millis:03}Z")
}

fn month_number(name: &str) -> Option<i64> {
    let n = match name.to_ascii_lowercase().as_str() {
        "jan" => 1,
        "feb" => 2,
        "mar" => 3,
        "apr" => 4,
        "may" => 5,
        "jun" => 6,
        "jul" => 7,
        "aug" => 8,
        "sep" => 9,
        "oct" => 10,
        "nov" => 11,
        "dec" => 12,
        _ => return None,
    };
    Some(n)
}

/// Parse a timestamp to epoch milliseconds. Returns `None` for anything we do
/// not recognise, mirroring `Number.isFinite(Date.parse(v))`.
pub fn parse_timestamp(value: &str) -> Option<i64> {
    let s = value.trim();
    if s.is_empty() {
        return None;
    }

    // ISO 8601: YYYY-MM-DD[T| ]HH:MM:SS[.fff][Z|±HH:MM].
    if s.len() >= 10 {
        let b = s.as_bytes();
        if b[0..4].iter().all(u8::is_ascii_digit)
            && b[4] == b'-'
            && b[5..7].iter().all(u8::is_ascii_digit)
            && b[7] == b'-'
            && b[8..10].iter().all(u8::is_ascii_digit)
        {
            let y: i64 = s[0..4].parse().ok()?;
            let m: i64 = s[5..7].parse().ok()?;
            let d: i64 = s[8..10].parse().ok()?;
            if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
                return None;
            }
            let mut secs = days_from_civil(y, m, d) * 86_400;
            let rest = &s[10..];
            let mut millis = 0i64;
            if rest.len() >= 9 && (rest.as_bytes()[0] == b'T' || rest.as_bytes()[0] == b' ') {
                let hh: i64 = rest[1..3].parse().ok()?;
                let mm: i64 = rest[4..6].parse().ok()?;
                let ss: i64 = rest[7..9].parse().ok()?;
                secs += hh * 3600 + mm * 60 + ss;
                let tail = &rest[9..];
                if let Some(after_dot) = tail.strip_prefix('.') {
                    let digits: String = after_dot
                        .chars()
                        .take_while(|c| c.is_ascii_digit())
                        .collect();
                    if !digits.is_empty() {
                        let frac: f64 = format!("0.{digits}").parse().ok()?;
                        millis = (frac * 1000.0) as i64;
                    }
                }
                // An explicit offset shifts the local time back to UTC.
                for (token, sign) in [("Z", 0), ("+", -1), ("-", 1)] {
                    if let Some(off) = tail.strip_prefix(token) {
                        if token != "Z" && off.len() >= 5 {
                            let oh: i64 = off[0..2].parse().ok()?;
                            let om: i64 = off[3..5].parse().ok()?;
                            secs += sign * (oh * 3600 + om * 60);
                        }
                        break;
                    }
                }
            }
            return Some(secs * 1000 + millis);
        }
    }

    // RFC 2822 / RSS: "Mon, 01 Jan 2026 00:00:00 GMT" or "... +0800".
    let cleaned = s.replace(',', " ");
    let parts: Vec<&str> = cleaned.split_whitespace().collect();
    // Drop an optional leading weekday.
    let parts = if parts.len() >= 5 && parts[0].len() == 3 && month_number(parts[0]).is_none() {
        &parts[1..]
    } else {
        parts.as_slice()
    };
    if parts.len() < 5 {
        return None;
    }
    let day: i64 = parts[0].parse().ok()?;
    let month = month_number(parts[1])?;
    let year: i64 = parts[2].parse().ok()?;
    let mut time = parts[3].split(':');
    let hh: i64 = time.next()?.parse().ok()?;
    let mm: i64 = time.next()?.parse().ok()?;
    let ss: i64 = time.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let mut secs = days_from_civil(year, month, day) * 86_400 + hh * 3600 + mm * 60 + ss;
    // Numeric offset, if present, converts the local time to UTC.
    if parts.len() >= 6 {
        let off = parts[5];
        if (off.starts_with('+') || off.starts_with('-')) && off.len() >= 5 {
            let oh: i64 = off[1..3].parse().ok()?;
            let om: i64 = off[3..5].parse().ok()?;
            let delta = oh * 3600 + om * 60;
            secs += if off.starts_with('+') { -delta } else { delta };
        }
    }
    Some(secs * 1000)
}

/// Mirror of `latestIsoTimestamp`: the newest parseable timestamp, or "now".
pub fn latest_iso_timestamp(values: &[Option<&str>]) -> String {
    let mut newest: Option<i64> = None;
    for value in values.iter().flatten() {
        if let Some(ms) = parse_timestamp(value) {
            newest = Some(newest.map_or(ms, |n| n.max(ms)));
        }
    }
    iso_from_millis(newest.unwrap_or_else(now_millis))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_parse_roundtrip() {
        let ms = parse_timestamp("2026-01-02T03:04:05.123Z").unwrap();
        assert_eq!(iso_from_millis(ms), "2026-01-02T03:04:05.123Z");
    }

    #[test]
    fn offset_and_rfc2822_parse() {
        let a = parse_timestamp("2026-01-02T11:04:05+08:00").unwrap();
        let b = parse_timestamp("2026-01-02T03:04:05Z").unwrap();
        assert_eq!(a, b);
        let c = parse_timestamp("Mon, 01 Jan 2026 00:00:00 GMT").unwrap();
        assert_eq!(c, parse_timestamp("2026-01-01T00:00:00Z").unwrap());
    }

    #[test]
    fn latest_picks_newest() {
        let got = latest_iso_timestamp(&[
            Some("2026-01-01T00:00:00Z"),
            None,
            Some("2026-01-02T00:00:00Z"),
        ]);
        assert!(got.starts_with("2026-01-02"));
    }
}
