//! Shared display formatters and Singapore-time helpers.
//! Ported from `lib/formatters.ts` plus the date/time primitives used by
//! `lib/amenities.ts`.
//!
//! Singapore has no DST (fixed UTC+8), so the time-zone conversion is a plain
//! offset — no tz database dependency is needed. ISO timestamps are parsed by
//! a small, dependency-free parser; timestamps without an explicit zone are
//! treated as UTC (upstream feeds include `Z` or an offset in practice).

/// A parsed ISO-8601 timestamp (UTC-normalised).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IsoTimestamp {
    pub epoch_seconds: i64,
}

/// Parse the common ISO-8601 shapes emitted by the upstream feeds:
/// `YYYY-MM-DD`, `YYYY-MM-DDTHH:MM[:SS[.fff]][Z|±HH:MM]`.
pub fn parse_iso(input: &str) -> Option<IsoTimestamp> {
    let s = input.trim();
    let bytes = s.as_bytes();
    if bytes.len() < 10 {
        return None;
    }
    let num = |a: usize, b: usize| -> Option<i64> { s.get(a..b)?.parse::<i64>().ok() };
    let year = num(0, 4)?;
    let month = num(5, 7)?;
    let day = num(8, 10)?;
    if bytes.get(4) != Some(&b'-') || bytes.get(7) != Some(&b'-') {
        return None;
    }
    let mut hour = 0i64;
    let mut minute = 0i64;
    let mut second = 0i64;
    if bytes.len() >= 16 && (bytes[10] == b'T' || bytes[10] == b' ') {
        hour = num(11, 13)?;
        minute = num(14, 16)?;
        if bytes.len() >= 19 && bytes.get(16) == Some(&b':') {
            second = num(17, 19)?;
        }
    }
    // Determine the timezone offset in minutes.
    let mut offset_minutes: i64 = 0;
    if let Some(pos) = s.find('Z') {
        let _ = pos; // UTC
    } else {
        // Look for a trailing +HH:MM / -HH:MM (after the time portion).
        let tail = &s[10..];
        if let Some(sign_pos) = tail.rfind(['+', '-']) {
            let sign = tail.as_bytes()[sign_pos];
            let off = &tail[sign_pos + 1..];
            let parts: Vec<&str> = off.split(':').collect();
            if parts.len() == 2
                && let (Ok(h), Ok(m)) = (parts[0].parse::<i64>(), parts[1].parse::<i64>())
            {
                offset_minutes = h * 60 + m;
                if sign == b'-' {
                    offset_minutes = -offset_minutes;
                }
            }
        }
    }
    let days = days_from_civil(year, month, day);
    let local_seconds = days * 86_400 + hour * 3_600 + minute * 60 + second;
    Some(IsoTimestamp {
        epoch_seconds: local_seconds - offset_minutes * 60,
    })
}

/// Howard Hinnant's days-from-civil algorithm (days since 1970-01-01).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = year - if month <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = month + if month > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Inverse of `days_from_civil`: (year, month, day).
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    (y + if m <= 2 { 1 } else { 0 }, m, d)
}

const SG_OFFSET_SECONDS: i64 = 8 * 3600;

fn sg_civil(ts: &IsoTimestamp) -> (i64, i64, i64, i64, i64, i64) {
    let local = ts.epoch_seconds + SG_OFFSET_SECONDS;
    let days = local.div_euclid(86_400);
    let rem = local.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    (y, m, d, rem / 3600, (rem % 3600) / 60, rem % 60)
}

/// `formatSgTime` — 12-hour time in en-SG style, e.g. `8:15:30 am`.
pub fn format_sg_time(iso: &str) -> String {
    let Some(ts) = parse_iso(iso) else {
        return String::new();
    };
    let (_, _, _, hour, minute, second) = sg_civil(&ts);
    let (h, period) = match hour {
        0 => (12, "am"),
        1..=11 => (hour, "am"),
        12 => (12, "pm"),
        _ => (hour - 12, "pm"),
    };
    format!("{}:{:02}:{:02} {}", h, minute, second, period)
}

/// `formatSgDateTime` — e.g. `16 Nov 2026, 8:15 am` (en-SG date + time).
pub fn format_sg_date_time(iso: &str) -> String {
    let Some(ts) = parse_iso(iso) else {
        return String::new();
    };
    let (y, m, d, hour, minute, _) = sg_civil(&ts);
    let month = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let (h, period) = match hour {
        0 => (12, "am"),
        1..=11 => (hour, "am"),
        12 => (12, "pm"),
        _ => (hour - 12, "pm"),
    };
    format!(
        "{} {} {}, {}:{:02} {}",
        d,
        month[(m - 1) as usize],
        y,
        h,
        minute,
        period
    )
}

/// Today's calendar date in Singapore (YYYY-MM-DD).
pub fn singapore_today() -> String {
    // Use the system clock via std only when called from an app; tests can use
    // `singapore_date_for` with an explicit timestamp.
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    singapore_date_for(IsoTimestamp {
        epoch_seconds: secs,
    })
}

pub fn singapore_date_for(ts: IsoTimestamp) -> String {
    let (y, m, d, _, _, _) = sg_civil(&ts);
    format!("{:04}-{:02}-{:02}", y, m, d)
}

pub fn format_speed_kmh(speed: Option<f64>) -> String {
    match speed {
        Some(s) if s.is_finite() => format!("{} km/h", (s * 3.6).round() as i64),
        _ => "N/A".into(),
    }
}

pub fn format_altitude_feet(altitude: Option<f64>) -> String {
    match altitude {
        Some(a) if a.is_finite() => format!("{} ft", group_thousands((a * 3.28084).round() as i64)),
        _ => "N/A".into(),
    }
}

/// Insert commas into an integer (toLocaleString("en-SG")).
pub fn group_thousands(value: i64) -> String {
    let negative = value < 0;
    let digits = value.abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if negative { format!("-{}", out) } else { out }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_iso_and_converts_to_singapore_date() {
        let ts = parse_iso("2026-11-16T16:30:00Z").unwrap();
        assert_eq!(singapore_date_for(ts), "2026-11-17");
        assert_eq!(format_sg_time("2026-11-16T16:30:00Z"), "12:30:00 am");
        assert!(parse_iso("not-a-date").is_none());
    }

    #[test]
    fn speed_altitude_and_thousands() {
        assert_eq!(format_speed_kmh(Some(10.0)), "36 km/h");
        assert_eq!(format_speed_kmh(None), "N/A");
        assert_eq!(format_altitude_feet(Some(1000.0)), "3,281 ft");
        assert_eq!(format_altitude_feet(None), "N/A");
        assert_eq!(group_thousands(1234567), "1,234,567");
    }
}
