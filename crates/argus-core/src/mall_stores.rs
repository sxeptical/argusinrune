//! Mall store-directory helpers (cleaning, grouping, sorting, filtering).
//! Ported from `lib/mall-stores.ts`.

use crate::types::MallStore;

fn text(value: Option<&str>) -> Option<String> {
    value
        .map(|v| v.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|s| !s.is_empty())
}

fn is_alnum(b: u8) -> bool {
    b.is_ascii_alphanumeric()
}

/// URL-slug units into Singapore's `#level-unit` form.
pub fn normalize_unit(raw: Option<&str>) -> Option<String> {
    let value = text(raw)?;
    let lower = value.to_ascii_lowercase();
    let mut s = lower.as_str();
    if let Some(rest) = s.strip_prefix("unit-") {
        s = rest;
    }
    // Split at the first hyphen into group1 and group2.
    let (g1, g2) = match s.find('-') {
        Some(i) => (&s[..i], &s[i + 1..]),
        None => return Some(value),
    };
    // group1: (jp\d)?b?\d{1,2}
    let mut rest = g1;
    if let Some(r) = rest.strip_prefix("jp") {
        let Some((digit, after)) = r.split_at_first_digit() else {
            return Some(value);
        };
        let _ = digit;
        rest = after;
    }
    if let Some(r) = rest.strip_prefix('b') {
        rest = r;
    }
    if rest.is_empty() || rest.len() > 2 || !rest.bytes().all(|b| b.is_ascii_digit()) {
        return Some(value);
    }
    // group2: alnum segments separated by single hyphens.
    if g2.is_empty()
        || g2
            .split('-')
            .any(|seg| seg.is_empty() || !seg.bytes().all(is_alnum))
    {
        return Some(value);
    }
    let units = g2.replace("-to-", " TO ").replace('-', "/");
    Some(format!("#{}-{}", g1, units).to_uppercase())
}

trait SplitDigit {
    fn split_at_first_digit(&self) -> Option<(&str, &str)>;
}
impl SplitDigit for str {
    fn split_at_first_digit(&self) -> Option<(&str, &str)> {
        let i = self.find(|c: char| c.is_ascii_digit())?;
        Some((&self[..i + 1], &self[i + 1..]))
    }
}

const MAX_LEVEL: i64 = 60;
fn level_key(basement: bool, number: i64, mezzanine: bool) -> Option<String> {
    if (1..=MAX_LEVEL).contains(&number) {
        Some(format!(
            "{}{}{}",
            if basement { "B" } else { "L" },
            number,
            if mezzanine { "M" } else { "" }
        ))
    } else {
        None
    }
}

/// One level token ("01", "L2", "Level 3", "Basement 1", "Level B2", "B2M").
fn parse_level_token(token: &str) -> Option<String> {
    let mut value = token.trim().to_string();
    // strip leading "block <letter> "
    let lower = value.to_ascii_lowercase();
    if lower.starts_with("block ") && lower.len() >= 8 {
        let bytes = lower.as_bytes();
        if bytes[6].is_ascii_alphabetic() && bytes[7] == b' ' {
            value = value[8..].to_string();
        }
    }
    let lower = value.to_ascii_lowercase();
    // basement: (basement | level b | b) 0*(\d{1,2}) (m?)
    let basement_body = if let Some(rest) = lower.strip_prefix("basement") {
        Some(rest.trim_start())
    } else if let Some(rest) = lower.strip_prefix("level b") {
        Some(rest)
    } else {
        lower.strip_prefix('b')
    };
    if let Some(body) = basement_body {
        let mezz = body.ends_with('m');
        let digits = if mezz { &body[..body.len() - 1] } else { body };
        let digits = digits.trim_start_matches('0');
        if !digits.is_empty() && digits.len() <= 2 && digits.bytes().all(|b| b.is_ascii_digit()) {
            return level_key(true, digits.parse().unwrap(), mezz);
        }
    }
    // above: (level | l)? 0*(\d{1,2})
    let above = lower
        .strip_prefix("level")
        .or_else(|| lower.strip_prefix('l'))
        .unwrap_or(&lower)
        .trim();
    let digits = above.trim_start_matches('0');
    if !digits.is_empty() && digits.len() <= 2 && digits.bytes().all(|b| b.is_ascii_digit()) {
        return level_key(false, digits.parse().unwrap(), false);
    }
    None
}

/// Level key for grouping: the level field, then the unit's floor, else a
/// named area, else None.
pub fn normalize_level(raw_level: Option<&str>, unit: Option<&str>) -> Option<String> {
    let level = text(raw_level);
    if let Some(level) = &level {
        for part in level.split(',') {
            let trimmed = part.trim();
            if let Some(key) = parse_level_token(trimmed)
                .or_else(|| parse_level_token(trimmed.split_whitespace().next().unwrap_or("")))
            {
                return Some(key);
            }
        }
    }
    // fromUnit: #(JP\d)?(B\d{1,2}|\d{2})(M?)-
    if let Some(unit) = unit {
        let upper = unit.to_ascii_uppercase();
        let mut rest = upper.strip_prefix('#').unwrap_or(&upper);
        if let Some(r) = rest.strip_prefix("JP") {
            let (_, after) = r.split_at_first_digit()?;
            rest = after;
        }
        // capture B?\d{1,2} then optional M then '-'
        let bytes = rest.as_bytes();
        let mut i = 0;
        let basement = bytes.first() == Some(&b'B');
        if basement {
            i += 1;
        }
        let digit_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() && i - digit_start < 2 {
            i += 1;
        }
        if i > digit_start && bytes.get(i) == Some(&b'-') {
            let mut token = rest[..i].to_string();
            if basement {
                token = format!("B{}", token.trim_start_matches('B'));
            }
            if let Some(key) = parse_level_token(&token) {
                return Some(key);
            }
        } else if i > digit_start && bytes.get(i) == Some(&b'M') && bytes.get(i + 1) == Some(&b'-')
        {
            let token = format!("{}M", &rest[..i]);
            if let Some(key) = parse_level_token(&token) {
                return Some(key);
            }
        }
    }
    match level {
        Some(l) if !l.bytes().all(|b| b.is_ascii_digit()) => Some(l),
        _ => None,
    }
}

/// Only http(s) links; bare "www.x.com" gains https://.
pub fn normalize_website(raw: Option<&str>) -> Option<String> {
    let value = text(raw)?;
    if value.bytes().any(|b| b.is_ascii_whitespace()) || !value.contains('.') {
        return None;
    }
    let lower = value.to_ascii_lowercase();
    if lower.starts_with("javascript:") || lower.starts_with("data:") {
        return None;
    }
    let mut url = if lower.starts_with("http://") || lower.starts_with("https://") {
        value
    } else {
        format!("https://{}", value)
    };
    // new URL(...).href appends a trailing slash when there is no path.
    let after_scheme = url.split_once("://").map(|(_, r)| r).unwrap_or(&url);
    if !after_scheme.contains('/') {
        url.push('/');
    }
    Some(url)
}

#[derive(Clone, Debug, Default)]
pub struct RawStoreRow<'a> {
    pub store_name: Option<&'a str>,
    pub unit: Option<&'a str>,
    pub level: Option<&'a str>,
    pub scope_of_business: Option<&'a str>,
    pub raw_category: Option<&'a str>,
    pub is_fnb: bool,
    pub is_halal: bool,
    pub website: Option<&'a str>,
}

pub fn clean_store_row(row: &RawStoreRow<'_>) -> Option<MallStore> {
    let name = text(row.store_name)?;
    let unit = normalize_unit(row.unit);
    let fnb = row.is_fnb;
    Some(MallStore {
        name,
        level: normalize_level(row.level, unit.as_deref()),
        category: text(row.scope_of_business)
            .or_else(|| text(row.raw_category))
            .or_else(|| fnb.then(|| "Food & Beverage".to_string())),
        unit,
        fnb,
        halal: row.is_halal,
        website: normalize_website(row.website),
    })
}

pub fn normalize_mall_name(name: &str) -> String {
    let mut out = String::new();
    let mut chars = name.chars().peekable();
    while chars.peek().is_some() {
        if chars.peek() == Some(&'(') {
            for c in chars.by_ref() {
                if c == ')' {
                    break;
                }
            }
            continue;
        }
        let c = chars.next().unwrap();
        if c == '&' {
            out.push_str("and");
        } else {
            out.push(c);
        }
    }
    let lower = out.to_lowercase();
    lower
        .replace("singapore", "")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

fn without_suffix(name: &str) -> String {
    let lower = name.to_lowercase();
    let trimmed = lower
        .trim_end_matches("shopping centre")
        .trim_end_matches("shopping center")
        .trim_end_matches("mall")
        .trim();
    normalize_mall_name(trimmed)
}

#[derive(Clone, Debug, Default)]
pub struct CuratedMall<'a> {
    pub mall_name: &'a str,
    pub postal_code: Option<&'a str>,
    pub latitude: Option<&'a str>,
    pub longitude: Option<&'a str>,
}

pub fn match_store_list_mall(
    list_name: &str,
    malls: &[crate::types::Mall],
    curated: &[CuratedMall<'_>],
    aliases: &std::collections::BTreeMap<String, String>,
) -> Option<crate::types::Mall> {
    if let Some(alias) = aliases.get(list_name) {
        return malls.iter().find(|m| m.name == *alias).cloned();
    }
    let key = normalize_mall_name(list_name);
    if let Some(mall) = malls.iter().find(|m| normalize_mall_name(&m.name) == key) {
        return Some(mall.clone());
    }
    let record = curated
        .iter()
        .find(|info| normalize_mall_name(info.mall_name) == key);
    if let Some(postcode) = record.and_then(|r| r.postal_code)
        && let Some(mall) = malls
            .iter()
            .find(|m| m.address.as_deref().is_some_and(|a| a.contains(postcode)))
    {
        return Some(mall.clone());
    }
    if let (Some(lat), Some(lng)) = (
        record
            .and_then(|r| r.latitude)
            .and_then(|s| s.parse::<f64>().ok()),
        record
            .and_then(|r| r.longitude)
            .and_then(|s| s.parse::<f64>().ok()),
    ) {
        let nearest = malls
            .iter()
            .map(|m| (m, (m.lat - lat).hypot(m.lng - lng)))
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        if let Some((mall, distance)) = nearest
            && distance < 0.003
        {
            return Some(mall.clone());
        }
    }
    let loose: Vec<_> = malls
        .iter()
        .filter(|m| without_suffix(&m.name) == without_suffix(list_name))
        .collect();
    if loose.len() == 1 {
        Some(loose[0].clone())
    } else {
        None
    }
}

// --------------------------------------------------------------------------- presentation

fn level_match(level: &str) -> Option<(char, i64, bool)> {
    let bytes = level.as_bytes();
    if bytes.len() < 2 || (bytes[0] != b'B' && bytes[0] != b'L') {
        return None;
    }
    let digits: String = level[1..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    let mezz = level.ends_with('M');
    let number: i64 = digits.parse().ok()?;
    Some((bytes[0] as char, number, mezz))
}

pub fn level_label(level: Option<&str>) -> String {
    let Some(level) = level else {
        return "Level not listed".into();
    };
    let Some((prefix, number, mezz)) = level_match(level) else {
        return level.into();
    };
    format!(
        "{} {}{}",
        if prefix == 'B' { "Basement" } else { "Level" },
        number,
        if mezz { " Mezzanine" } else { "" }
    )
}

fn level_rank(level: Option<&str>) -> f64 {
    let Some(level) = level else {
        return f64::INFINITY;
    };
    let Some((prefix, number, mezz)) = level_match(level) else {
        return 1_000.0;
    };
    (if prefix == 'B' {
        -number as f64
    } else {
        number as f64
    }) + if mezz { 0.5 } else { 0.0 }
}

pub fn compare_levels(a: Option<&str>, b: Option<&str>) -> std::cmp::Ordering {
    level_rank(a)
        .partial_cmp(&level_rank(b))
        .unwrap()
        .then_with(|| (a.unwrap_or("")).cmp(b.unwrap_or("")))
}

pub fn compare_stores(a: &MallStore, b: &MallStore) -> std::cmp::Ordering {
    a.name
        .to_lowercase()
        .cmp(&b.name.to_lowercase())
        .then_with(|| {
            a.unit
                .as_deref()
                .unwrap_or("")
                .cmp(b.unit.as_deref().unwrap_or(""))
        })
}

#[derive(Clone, Debug, Default)]
pub struct StoreFilter<'a> {
    pub query: &'a str,
    pub category: Option<&'a str>,
    pub halal_only: bool,
}

pub fn filter_stores<'a>(stores: &'a [MallStore], filter: &StoreFilter<'_>) -> Vec<&'a MallStore> {
    let needle = filter
        .query
        .trim()
        .to_lowercase()
        .trim_start_matches('#')
        .to_string();
    stores
        .iter()
        .filter(|store| {
            filter
                .category
                .is_none_or(|c| store.category.as_deref() == Some(c))
                && (!filter.halal_only || store.halal)
                && (needle.is_empty()
                    || store.name.to_lowercase().contains(&needle)
                    || store
                        .unit
                        .as_deref()
                        .is_some_and(|u| u.to_lowercase().contains(&needle)))
        })
        .collect()
}

pub fn count_categories(stores: &[MallStore]) -> Vec<(String, i64)> {
    let mut counts: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
    for store in stores {
        if let Some(category) = &store.category {
            *counts.entry(category.clone()).or_default() += 1;
        }
    }
    let mut result: Vec<_> = counts.into_iter().collect();
    result.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    result
}

pub struct StoreLevelGroup<'a> {
    pub level: Option<&'a str>,
    pub label: String,
    pub stores: Vec<&'a MallStore>,
}

pub fn group_stores_by_level(stores: &[MallStore]) -> Vec<StoreLevelGroup<'_>> {
    let mut groups: std::collections::BTreeMap<Option<&str>, Vec<&MallStore>> =
        std::collections::BTreeMap::new();
    for store in stores {
        groups
            .entry(store.level.as_deref())
            .or_default()
            .push(store);
    }
    let mut entries: Vec<_> = groups.into_iter().collect();
    entries.sort_by(|(a, _), (b, _)| compare_levels(*a, *b));
    entries
        .into_iter()
        .map(|(level, mut items)| {
            items.sort_by(|a, b| compare_stores(a, b));
            StoreLevelGroup {
                level,
                label: level_label(level),
                stores: items,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_units_and_levels() {
        assert_eq!(normalize_unit(Some("unit-b1-k12")).unwrap(), "#B1-K12");
        assert_eq!(normalize_unit(Some("unit-01-62-63")).unwrap(), "#01-62/63");
        assert_eq!(normalize_unit(Some("01-02-to-05")).unwrap(), "#01-02 TO 05");
        assert_eq!(normalize_unit(Some("b1-23")).unwrap(), "#B1-23");
        assert_eq!(
            normalize_unit(Some("North Wing, #01-23")).unwrap(),
            "North Wing, #01-23"
        );
        assert_eq!(normalize_unit(Some("  ")), None);
        for raw in [
            "01",
            "L1",
            "Level 1",
            "Level 01",
            "South Wing, Level 1",
            "BLOCK D 01",
            "L1 OPEN PLAZA",
        ] {
            assert_eq!(
                normalize_level(Some(raw), None).unwrap(),
                "L1",
                "raw={}",
                raw
            );
        }
        for raw in ["B1", "Basement 1", "Level B1", "North Wing, Basement 1"] {
            assert_eq!(
                normalize_level(Some(raw), None).unwrap(),
                "B1",
                "raw={}",
                raw
            );
        }
        assert_eq!(normalize_level(Some("B2M"), None).unwrap(), "B2M");
        assert_eq!(normalize_level(None, Some("#B2-07")).unwrap(), "B2");
        assert_eq!(normalize_level(None, Some("#03-15/16")).unwrap(), "L3");
        assert_eq!(
            normalize_level(Some("Tower 3"), Some("Suntec Tower 3")).unwrap(),
            "Tower 3"
        );
    }

    #[test]
    fn websites_and_store_rows() {
        assert_eq!(
            normalize_website(Some("www.dbs.com.sg")).unwrap(),
            "https://www.dbs.com.sg/"
        );
        assert_eq!(
            normalize_website(Some("https://www.uniqlo.com/sg/")).unwrap(),
            "https://www.uniqlo.com/sg/"
        );
        assert_eq!(normalize_website(Some("javascript:alert(1)")), None);
        let row = RawStoreRow {
            store_name: Some("  4Fingers  Crispy Chicken "),
            unit: Some("unit-b1-12"),
            level: None,
            scope_of_business: None,
            raw_category: None,
            is_fnb: true,
            is_halal: true,
            website: Some("https://www.4fingers.com.sg/"),
        };
        let store = clean_store_row(&row).unwrap();
        assert_eq!(store.name, "4Fingers Crispy Chicken");
        assert_eq!(store.unit.as_deref(), Some("#B1-12"));
        assert_eq!(store.level.as_deref(), Some("B1"));
        assert_eq!(store.category.as_deref(), Some("Food & Beverage"));
    }

    #[test]
    fn presentation_sort_filter_group() {
        let store = |name: &str, level: Option<&str>, unit: Option<&str>| MallStore {
            name: name.into(),
            unit: unit.map(str::to_string),
            level: level.map(str::to_string),
            category: None,
            fnb: false,
            halal: false,
            website: None,
        };
        let stores = vec![
            store("Zara", Some("L2"), Some("#02-01")),
            store("a.p.c.", Some("B1"), Some("#B1-05")),
        ];
        assert_eq!(level_label(Some("B2M")), "Basement 2 Mezzanine");
        assert_eq!(group_stores_by_level(&stores).len(), 2);
        assert_eq!(
            compare_levels(Some("B2"), Some("L1")),
            std::cmp::Ordering::Less
        );
        assert_eq!(
            filter_stores(
                &stores,
                &StoreFilter {
                    query: "zara",
                    ..Default::default()
                }
            )
            .len(),
            1
        );
    }
}
