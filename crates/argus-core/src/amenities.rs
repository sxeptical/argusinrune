//! Popup copy and display helpers for the mall / hawker layers.
//! Ported from `lib/amenities.ts`.

use std::collections::BTreeMap;

use crate::format::singapore_today;
use crate::mobility::plural;
use crate::types::{AvailabilityTone, HawkerCentre, HawkerClosure, Mall, PlacePhoto, PopupLine};

/// Turn common OpenStreetMap `opening_hours` shapes into plain English.
/// Returns None for "off".
pub fn format_opening_hours(raw: &str) -> Option<String> {
    // trim, drop ",PH", convert "HH:MM - HH:MM" to "HH:MM–HH:MM"
    let trimmed = raw.trim();
    let without_ph = trimmed.replace(",PH", "");
    let replaced = replace_time_ranges(without_ph.trim());
    if replaced == "off" {
        return None;
    }
    if replaced == "24/7" || replaced == "Mo-Su 00:00–24:00" {
        return Some("Open 24 hours".into());
    }
    if is_time_range_only(&replaced) {
        return Some(format!("Daily {}", replaced));
    }
    let mut value = replaced;
    value = value.replacen("Mo-Su", "Daily", 1);
    // Replace day tokens with their abbreviations (word boundaries).
    for (code, name) in [
        ("Mo", "Mon"),
        ("Tu", "Tue"),
        ("We", "Wed"),
        ("Th", "Thu"),
        ("Fr", "Fri"),
        ("Sa", "Sat"),
        ("Su", "Sun"),
    ] {
        value = replace_word(&value, code, name);
    }
    value = value.replace("; ", ", ");
    Some(value)
}

fn is_time_range_only(s: &str) -> bool {
    let parts: Vec<&str> = s.split('–').collect();
    parts.len() == 2
        && parts[0].len() == 5
        && parts[0].as_bytes().get(2) == Some(&b':')
        && parts[1].len() == 5
        && parts[1].as_bytes().get(2) == Some(&b':')
}

fn replace_time_ranges(s: &str) -> String {
    // Convert "HH:MM - HH:MM" into "HH:MM–HH:MM".
    let mut out = String::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 5 <= bytes.len()
            && is_digit(bytes[i])
            && is_digit(bytes[i + 1])
            && bytes.get(i + 2) == Some(&b':')
            && is_digit(bytes[i + 3])
            && is_digit(bytes[i + 4])
        {
            let mut j = i + 5;
            while j < bytes.len() && bytes[j] == b' ' {
                j += 1;
            }
            if bytes.get(j) == Some(&b'-') {
                let mut k = j + 1;
                while k < bytes.len() && bytes[k] == b' ' {
                    k += 1;
                }
                if k + 5 <= bytes.len()
                    && is_digit(bytes[k])
                    && is_digit(bytes[k + 1])
                    && bytes.get(k + 2) == Some(&b':')
                    && is_digit(bytes[k + 3])
                    && is_digit(bytes[k + 4])
                {
                    out.push_str(&s[i..i + 5]);
                    out.push('–');
                    out.push_str(&s[k..k + 5]);
                    i = k + 5;
                    continue;
                }
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn is_digit(b: u8) -> bool {
    b.is_ascii_digit()
}

/// Replace whole-word `word` in `s` with `replacement`.
fn replace_word(s: &str, word: &str, replacement: &str) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < s.len() {
        if s[i..].starts_with(word) {
            let before_ok = i == 0 || !is_word_byte(s.as_bytes()[i - 1]);
            let after = i + word.len();
            let after_ok = after == s.len() || !is_word_byte(s.as_bytes()[after]);
            if before_ok && after_ok {
                out.push_str(replacement);
                i = after;
                continue;
            }
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}
fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

const MONTHS: &[&str] = &[
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

fn split_date(s: &str) -> (i64, i64, i64) {
    let parts: Vec<i64> = s.split('-').filter_map(|p| p.parse().ok()).collect();
    (parts[0], parts[1], parts[2])
}

/// "12 Dec", "12–15 Dec", or "28 Dec 2025–2 Jan 2026" for ISO date ranges.
pub fn format_closure_range(start: &str, end: &str) -> String {
    let (sy, sm, sd) = split_date(start);
    let (ey, em, ed) = split_date(end);
    let sm = MONTHS[(sm - 1) as usize];
    let em = MONTHS[(em - 1) as usize];
    if start == end {
        format!("{} {}", sd, sm)
    } else if sy == ey && sm == em {
        format!("{}–{} {}", sd, ed, sm)
    } else if sy == ey {
        format!("{} {}–{} {}", sd, sm, ed, em)
    } else {
        format!("{} {} {}–{} {} {}", sd, sm, sy, ed, em, ey)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HawkerClosureStatus {
    pub current: bool,
    pub text: String,
}

/// The closure worth showing for a hawker centre.
pub fn describe_hawker_closure(
    closures: &[HawkerClosure],
    today_iso: &str,
) -> Option<HawkerClosureStatus> {
    let valid: Vec<&HawkerClosure> = closures.iter().filter(|c| c.end >= c.start).collect();
    let mut current: Vec<&HawkerClosure> = valid
        .iter()
        .copied()
        .filter(|c| c.start.as_str() <= today_iso && today_iso <= c.end.as_str())
        .collect();
    current.sort_by(|a, b| a.end.cmp(&b.end));
    let current = current.first().copied();
    let next = if current.is_some() {
        None
    } else {
        let mut upcoming: Vec<&HawkerClosure> = valid
            .iter()
            .copied()
            .filter(|c| c.start.as_str() > today_iso)
            .collect();
        upcoming.sort_by(|a, b| a.start.cmp(&b.start));
        upcoming.first().copied()
    };
    let closure = current.or(next)?;
    let kind = if closure.kind == crate::types::HawkerClosureKind::Cleaning {
        "cleaning"
    } else {
        "other works"
    };
    let remarks = closure
        .remarks
        .as_ref()
        .filter(|s| !s.is_empty())
        .map(|s| format!(" — {}", s))
        .unwrap_or_default();
    let text = if current.is_some() {
        format!(
            "Closed now for {} until {}{}",
            kind,
            format_closure_range(&closure.end, &closure.end),
            remarks
        )
    } else {
        format!(
            "Closed for {} {}{}",
            kind,
            format_closure_range(&closure.start, &closure.end),
            remarks
        )
    };
    Some(HawkerClosureStatus {
        current: current.is_some(),
        text,
    })
}

pub fn describe_hawker_centre(hawker: &HawkerCentre, today_iso: &str) -> Vec<PopupLine> {
    let closure = describe_hawker_closure(&hawker.closures, today_iso);
    let mut lines = vec![PopupLine::new(&hawker.name, "name")];
    if hawker.status == crate::types::HawkerStatus::Open {
        lines.push(PopupLine::toned(
            "In operation",
            "status",
            AvailabilityTone::Success,
        ));
    } else {
        let text = match hawker.expected_year {
            Some(year) => format!("Under construction · expected {}", year),
            None => "Under construction".into(),
        };
        lines.push(PopupLine::toned(text, "status", AvailabilityTone::Muted));
    }
    if let Some(status) = closure {
        let (kind, tone) = if status.current {
            ("status", AvailabilityTone::Warning)
        } else {
            ("detail", AvailabilityTone::Muted)
        };
        lines.push(PopupLine::toned(status.text, kind, tone));
    }
    if let Some(stalls) = hawker.stalls {
        lines.push(PopupLine::new(
            plural(stalls, "cooked food stall", "cooked food stalls"),
            "count",
        ));
    }
    if let Some(address) = &hawker.address
        && !address.is_empty()
    {
        lines.push(PopupLine::new(address, "detail"));
    }
    lines.extend(credit_line(&hawker.photos));
    lines.push(PopupLine::new("Source: NEA via data.gov.sg", "hint"));
    lines
}

/// "Photos: A, B (CC BY-SA 4.0); C (CC BY 2.5) · Wikimedia Commons" or
/// "Photo: NEA" for NEA's own hawker centre photo.
pub fn photo_credit(photos: &[PlacePhoto]) -> Option<String> {
    let commons: Vec<&PlacePhoto> = photos
        .iter()
        .filter(|p| p.source == crate::types::PlacePhotoSource::Commons)
        .collect();
    if commons.is_empty() {
        return photos
            .iter()
            .any(|p| p.source == crate::types::PlacePhotoSource::Nea)
            .then(|| "Photo: NEA".to_string());
    }
    let mut by_license: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for photo in &commons {
        let license = photo.license.as_deref().unwrap_or("unknown licence");
        let authors = by_license.entry(license).or_default();
        if !authors.contains(&photo.author.as_str()) {
            authors.push(photo.author.as_str());
        }
    }
    let groups: Vec<String> = by_license
        .iter()
        .map(|(license, authors)| format!("{} ({})", authors.join(", "), license))
        .collect();
    Some(format!(
        "{}: {} · Wikimedia Commons",
        if commons.len() == 1 {
            "Photo"
        } else {
            "Photos"
        },
        groups.join("; ")
    ))
}

fn credit_line(photos: &[PlacePhoto]) -> Vec<PopupLine> {
    photo_credit(photos)
        .map(|credit| vec![PopupLine::new(credit, "hint")])
        .unwrap_or_default()
}

pub fn describe_mall(mall: &Mall) -> Vec<PopupLine> {
    let hours = mall.opening_hours.as_deref().and_then(format_opening_hours);
    let mut lines = vec![PopupLine::new(&mall.name, "name")];
    if let Some(stores) = mall.stores {
        let base = plural(stores, "shop", "shops");
        let text = match mall.fnb {
            Some(fnb) => format!("{} · {} food & drink", base, fnb),
            None => base,
        };
        lines.push(PopupLine::new(text, "count"));
    }
    if let Some(mrt) = &mall.nearest_mrt
        && !mrt.is_empty()
    {
        lines.push(PopupLine::new(format!("Nearest MRT: {}", mrt), "detail"));
    }
    if let Some(hours) = hours {
        lines.push(PopupLine::new(hours, "detail"));
    }
    if let Some(address) = &mall.address
        && !address.is_empty()
    {
        lines.push(PopupLine::new(address, "detail"));
    }
    lines.extend(credit_line(&mall.photos));
    let hint = if mall.stores.is_none() {
        "Location: © OpenStreetMap contributors"
    } else {
        "Shops: curioputterings/singapore-mall-data · Location: © OpenStreetMap"
    };
    lines.push(PopupLine::new(hint, "hint"));
    lines
}

/// Re-export so callers can derive "today" without depending on `format`.
pub fn today() -> String {
    singapore_today()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opening_hours_and_ranges() {
        assert_eq!(
            format_opening_hours("Mo-Su 10:00-22:00").unwrap(),
            "Daily 10:00–22:00"
        );
        assert_eq!(
            format_opening_hours("Mo-Su,PH 10:00 - 22:00").unwrap(),
            "Daily 10:00–22:00"
        );
        assert_eq!(
            format_opening_hours("10:00-21:30").unwrap(),
            "Daily 10:00–21:30"
        );
        assert_eq!(format_opening_hours("24/7").unwrap(), "Open 24 hours");
        assert_eq!(format_opening_hours("off"), None);
        assert_eq!(
            format_opening_hours("Su-Th 10:00-22:00; Fr-Sa 10:00-23:00").unwrap(),
            "Sun-Thu 10:00–22:00, Fri-Sat 10:00–23:00"
        );
        assert_eq!(
            format_closure_range("2026-12-12", "2026-12-15"),
            "12–15 Dec"
        );
        assert_eq!(
            format_closure_range("2026-12-28", "2027-01-02"),
            "28 Dec 2026–2 Jan 2027"
        );
    }

    #[test]
    fn photo_credit_groups_by_license() {
        let photo = PlacePhoto {
            thumb: "https://thumb.wikimedia.org/a.jpg".into(),
            page: Some("https://commons.wikimedia.org/wiki/File:A.jpg".into()),
            author: "Terence Ong".into(),
            license: Some("CC BY 2.5".into()),
            source: crate::types::PlacePhotoSource::Commons,
            year: None,
        };
        assert_eq!(
            photo_credit(std::slice::from_ref(&photo)).unwrap(),
            "Photo: Terence Ong (CC BY 2.5) · Wikimedia Commons"
        );
        let nea = PlacePhoto {
            source: crate::types::PlacePhotoSource::Nea,
            license: None,
            ..photo
        };
        assert_eq!(photo_credit(&[nea]).unwrap(), "Photo: NEA");
    }
}
