//! UI preference parsers and localStorage-backed history merge helpers.
//! Ported from `lib/ui-preferences.ts` and the pure parts of `lib/local-history.ts`.
//!
//! The TypeScript functions take `unknown` (parsed JSON); here they take
//! `&serde_json::Value`, with `Value::Null` standing in for `null`/missing.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::geo::{GEO_FILTER_MAX_RADIUS_M, GEO_FILTER_MIN_RADIUS_M};
use crate::mrt_routing::is_routeable_mrt_station;
use crate::signals::{NotificationPrefs, SeenSignals, SignalCategory};
use crate::types::LngLat;

pub const SENSOR_VISIBILITY_KEY: &str = "argus.prefs.sensors.v1";
pub const AMENITY_FILTERS_KEY: &str = "argus.prefs.amenity-filters.v1";
pub const GEO_FILTER_CENTER_KEY: &str = "argus.prefs.geo-filter.center.v1";
pub const GEO_FILTER_RADIUS_KEY: &str = "argus.prefs.geo-filter.radius.v1";
pub const MRT_START_KEY: &str = "argus.prefs.mrt.start.v1";
pub const MRT_END_KEY: &str = "argus.prefs.mrt.end.v1";
pub const NOTIFICATION_PREFS_KEY: &str = "argus.prefs.notifications.v1";
pub const SEEN_SIGNALS_KEY: &str = "argus.prefs.notifications.seen.v1";
pub const AIR_SECTION_PINNED_KEY: &str = "argus.prefs.air.section.pinned.v1";

/// Take each known sensor's stored boolean flag, falling back to its default.
pub fn parse_sensor_visibility(
    stored: &Value,
    defaults: &BTreeMap<String, bool>,
) -> Option<BTreeMap<String, bool>> {
    let obj = stored.as_object()?;
    let mut next = defaults.clone();
    for key in defaults.keys() {
        if let Some(flag) = obj.get(key).and_then(Value::as_bool) {
            next.insert(key.clone(), flag);
        }
    }
    Some(next)
}

/// `null` is valid (the filter was cleared); malformed arrays are rejected.
pub fn parse_geo_filter_center(stored: &Value) -> Result<Option<LngLat>, ()> {
    if stored.is_null() {
        return Ok(None);
    }
    let arr = stored.as_array().ok_or(())?;
    if arr.len() != 2 {
        return Err(());
    }
    let lng = arr[0].as_f64().ok_or(())?;
    let lat = arr[1].as_f64().ok_or(())?;
    if !lng.is_finite() || !lat.is_finite() || lng.abs() > 180.0 || lat.abs() > 90.0 {
        return Err(());
    }
    Ok(Some([lng, lat]))
}

pub fn parse_geo_filter_radius(stored: &Value) -> Option<f64> {
    let n = stored.as_f64()?;
    if !n.is_finite() {
        return None;
    }
    Some(n.clamp(GEO_FILTER_MIN_RADIUS_M, GEO_FILTER_MAX_RADIUS_M))
}

pub fn parse_mrt_station(stored: &Value) -> Option<String> {
    let s = stored.as_str()?;
    if s.is_empty() || !is_routeable_mrt_station(s) {
        return None;
    }
    Some(s.to_string())
}

pub fn parse_notification_prefs(stored: &Value) -> Option<NotificationPrefs> {
    let flags = parse_sensor_visibility(stored, &notification_defaults())?;
    Some(NotificationPrefs {
        rail: *flags.get("rail").unwrap_or(&true),
        road: *flags.get("road").unwrap_or(&true),
        weather: *flags.get("weather").unwrap_or(&true),
        browser: *flags.get("browser").unwrap_or(&false),
    })
}

fn notification_defaults() -> BTreeMap<String, bool> {
    [
        ("rail", true), ("road", true), ("weather", true), ("browser", false),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), *v))
    .collect()
}

fn all_categories() -> [SignalCategory; 3] {
    [SignalCategory::Rail, SignalCategory::Road, SignalCategory::Weather]
}

pub fn parse_seen_signals(stored: &Value) -> Option<SeenSignals> {
    let obj = stored.as_object()?;
    let mut seen = SeenSignals::new();
    for category in all_categories() {
        if let Some(ids) = obj.get(category.as_str()).and_then(Value::as_array) {
            if ids.iter().all(Value::is_string) {
                seen.insert(category, ids.iter().filter_map(|v| v.as_str().map(str::to_string)).collect());
            }
        }
    }
    Some(seen)
}

/// Like `parse_sensor_visibility` but a stored all-false selection falls back
/// to all-on (so a layer is never stranded empty).
pub fn parse_layer_option_selection(
    stored: &Value,
    defaults: &BTreeMap<String, bool>,
) -> Option<BTreeMap<String, bool>> {
    let parsed = parse_sensor_visibility(stored, defaults)?;
    if parsed.values().all(|flag| !*flag) {
        return Some(defaults.clone());
    }
    Some(parsed)
}

pub fn parse_stored_boolean(stored: &Value) -> Option<bool> {
    stored.as_bool()
}

/// Namespaced key for a pref derived from text.
pub fn pref_key(namespace: &str, slug: &str) -> String {
    let normalized: String = slug
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let normalized = normalized.trim_matches('-').replace("--", "-");
    format!("argus.prefs.{}.v1:{}", namespace, normalized)
}

// ---------------------------------------------------------------------------
// local-history merge helpers
// ---------------------------------------------------------------------------

pub trait Timestamped {
    fn timestamp(&self) -> &str;
}

/// Union of two point lists; on a shared timestamp the right-hand wins.
pub fn merge_point_lists<T: Timestamped + Clone>(
    persisted: &[T],
    current: &[T],
) -> Vec<T> {
    if persisted.is_empty() { return current.to_vec(); }
    if current.is_empty() { return persisted.to_vec(); }
    let mut merged: BTreeMap<String, T> = BTreeMap::new();
    for point in persisted { merged.insert(point.timestamp().to_string(), point.clone()); }
    for point in current { merged.insert(point.timestamp().to_string(), point.clone()); }
    merged.into_values().collect()
}

/// Per-series union; on a shared timestamp the right-hand store wins.
pub fn merge_point_stores<T: Timestamped + Clone + PartialEq>(
    persisted: &BTreeMap<String, Vec<T>>,
    current: &BTreeMap<String, Vec<T>>,
) -> BTreeMap<String, Vec<T>> {
    if current.is_empty() {
        return persisted.clone();
    }
    let mut changed = persisted.keys().any(|key| !current.contains_key(key));
    let mut merged = persisted.clone();
    for (key, points) in current {
        let previous = persisted.get(key).map(|v| v.as_slice()).unwrap_or(&[]);
        let list = merge_point_lists(previous, points);
        if list.len() != points.len() || !list.iter().zip(points.iter()).all(|(a, b)| a == b) {
            changed = true;
        }
        merged.insert(key.clone(), list);
    }
    if changed { merged } else { current.clone() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_sensor_and_geo_prefs() {
        let defaults: BTreeMap<String, bool> = [("flights".into(), true), ("parks".into(), false)].into_iter().collect();
        let parsed = parse_sensor_visibility(&json!({"flights": false}), &defaults).unwrap();
        assert_eq!(parsed["flights"], false);
        assert_eq!(parsed["parks"], false);
        assert!(parse_sensor_visibility(&Value::Null, &defaults).is_none());
        let all_on: BTreeMap<String, bool> = [("flights".into(), true), ("parks".into(), true)].into_iter().collect();
        assert_eq!(parse_layer_option_selection(&json!({"flights": false, "parks": false}), &all_on).unwrap(), all_on);
        assert_eq!(parse_geo_filter_center(&Value::Null).unwrap(), None);
        assert_eq!(parse_geo_filter_center(&json!([103.85, 1.29])).unwrap(), Some([103.85, 1.29]));
        assert!(parse_geo_filter_center(&json!([200, 1.29])).is_err());
        assert_eq!(parse_geo_filter_radius(&json!(1)).unwrap(), GEO_FILTER_MIN_RADIUS_M);
        assert_eq!(parse_geo_filter_radius(&json!(1e9)).unwrap(), GEO_FILTER_MAX_RADIUS_M);
    }

    #[test]
    fn parses_mrt_notifications_and_keys() {
        assert_eq!(parse_mrt_station(&json!("Raffles Place")).unwrap(), "Raffles Place");
        assert!(parse_mrt_station(&json!("")).is_none());
        let prefs = parse_notification_prefs(&json!({"road": false, "browser": "yes"})).unwrap();
        assert!(!prefs.road && prefs.rail && !prefs.browser);
        let seen = parse_seen_signals(&json!({"rail": ["a"], "road": [1]})).unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(pref_key("panel-expanded", "MRT ROUTER"), "argus.prefs.panel-expanded.v1:mrt-router");
    }

    #[derive(Clone, PartialEq, Debug)]
    struct Point { timestamp: String, value: i64 }
    impl Timestamped for Point {
        fn timestamp(&self) -> &str { &self.timestamp }
    }

    #[test]
    fn merges_point_lists_and_stores() {
        let p1 = Point { timestamp: "2026-01-01T00:00:00.000Z".into(), value: 1 };
        let p2 = Point { timestamp: "2026-01-01T00:05:00.000Z".into(), value: 2 };
        let p2b = Point { timestamp: "2026-01-01T00:05:00.000Z".into(), value: 20 };
        let merged = merge_point_lists(&[p1.clone(), p2.clone()], &[p2b.clone()]);
        assert_eq!(merged, vec![p1, p2b.clone()]);
        let current: BTreeMap<String, Vec<Point>> = [("service".to_string(), vec![p2b])].into_iter().collect();
        assert!(merge_point_stores(&BTreeMap::new(), &current) == current);
        let persisted: BTreeMap<String, Vec<Point>> = [("service".to_string(), vec![Point { timestamp: "2026-01-01T00:00:00.000Z".into(), value: 1 }])].into_iter().collect();
        assert!(merge_point_stores(&persisted, &current) != current);
    }
}
