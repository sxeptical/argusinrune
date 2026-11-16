//! Intelligence signal digest and notification derivation.
//! Ported from `lib/signals.ts` and the pure parts of `lib/notifications.ts`.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::types::{TrafficIncident, TrainAlertStatus, TrainServiceAlert, WeatherData};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SignalCategory {
    Rail,
    Road,
    Weather,
}

impl SignalCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            SignalCategory::Rail => "rail",
            SignalCategory::Road => "road",
            SignalCategory::Weather => "weather",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SignalTone {
    Danger,
    Warning,
    Info,
    Muted,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignalRow {
    /// Stable across polls, so notifications can tell new signals from known ones.
    pub id: String,
    pub category: SignalCategory,
    pub tone: SignalTone,
    pub text: String,
}

use serde::{Deserialize, Serialize};

fn concise_message(message: &str) -> String {
    // fragment = message.trim().split(/[.!?](?:\s|$)/, 1)[0]?.trim()
    let trimmed = message.trim();
    let mut fragment = trimmed;
    let bytes = trimmed.as_bytes();
    let mut split_at = None;
    for (i, c) in bytes.iter().enumerate() {
        if matches!(c, b'.' | b'!' | b'?') {
            let next_is_boundary =
                i + 1 == bytes.len() || bytes.get(i + 1).is_some_and(|b| b.is_ascii_whitespace());
            if next_is_boundary {
                split_at = Some(i);
                break;
            }
        }
    }
    if let Some(i) = split_at {
        fragment = trimmed[..i].trim();
    }
    let chosen = if fragment.is_empty() {
        trimmed
    } else {
        fragment
    };
    chosen.chars().take(100).collect()
}

fn incident_tone(incident_type: &str) -> SignalTone {
    match incident_type {
        "Accident" => SignalTone::Danger,
        "Vehicle Breakdown" | "Heavy Traffic" => SignalTone::Warning,
        _ => SignalTone::Muted,
    }
}

/// Stable rail id: message is deliberately omitted because LTA rewords it.
fn rail_signal_id(alert: &TrainServiceAlert) -> String {
    let lines = if alert.affected_lines.is_empty() {
        "network".to_string()
    } else {
        let mut sorted = alert.affected_lines.clone();
        sorted.sort();
        sorted.join(",")
    };
    format!(
        "rail:{}:{}",
        lines,
        alert.start_time.as_deref().unwrap_or("unknown")
    )
}

pub struct DigestInput<'a> {
    pub train_alerts: &'a [TrainServiceAlert],
    pub incidents: &'a [TrafficIncident],
    pub weather: &'a WeatherData,
}

/// `buildSignalDigest` — rail, then road, then weather signals.
pub fn build_signal_digest(input: &DigestInput<'_>) -> Vec<SignalRow> {
    let mut rows = Vec::new();
    for alert in input
        .train_alerts
        .iter()
        .filter(|a| a.status == TrainAlertStatus::Disrupted)
    {
        let label = if alert.affected_lines.is_empty() {
            "MRT Network".to_string()
        } else {
            alert.affected_lines.join(", ")
        };
        rows.push(SignalRow {
            id: rail_signal_id(alert),
            category: SignalCategory::Rail,
            tone: SignalTone::Danger,
            text: format!("{}: {}", label, concise_message(&alert.message)),
        });
    }
    for incident in input.incidents {
        rows.push(SignalRow {
            id: format!("road:{}", incident.id),
            category: SignalCategory::Road,
            tone: incident_tone(&incident.incident_type),
            text: incident.message.clone(),
        });
    }
    if let Some(psi) = input.weather.psi
        && psi > 100.0
    {
        rows.push(SignalRow {
            id: "weather:psi-unhealthy".into(),
            category: SignalCategory::Weather,
            tone: SignalTone::Danger,
            text: format!(
                "PSI {} ({})",
                format_number(psi),
                input.weather.psi_status.as_str()
            ),
        });
    }
    rows
}

/// JS-style number formatting for integers (151 → "151", 151.5 → "151.5").
fn format_number(n: f64) -> String {
    if n.fract() == 0.0 {
        format!("{}", n as i64)
    } else {
        format!("{}", n)
    }
}

// ---------------------------------------------------------------------------
// Notification derivation (lib/notifications.ts)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct NotificationPrefs {
    pub rail: bool,
    pub road: bool,
    pub weather: bool,
    pub browser: bool,
}

impl Default for NotificationPrefs {
    fn default() -> Self {
        Self {
            rail: true,
            road: true,
            weather: true,
            browser: false,
        }
    }
}

/// Signal ids active at the last check. A missing category has no baseline yet.
pub type SeenSignals = HashMap<SignalCategory, Vec<String>>;

/// Whether each category's feed has returned data.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SignalsLoaded {
    pub rail: bool,
    pub road: bool,
    pub weather: bool,
}

impl SignalsLoaded {
    pub fn all_loaded() -> Self {
        Self {
            rail: true,
            road: true,
            weather: true,
        }
    }
    fn get(&self, category: SignalCategory) -> bool {
        match category {
            SignalCategory::Rail => self.rail,
            SignalCategory::Road => self.road,
            SignalCategory::Weather => self.weather,
        }
    }
}

/// Signal categories in priority order.
const SIGNAL_CATEGORIES: [SignalCategory; 3] = [
    SignalCategory::Rail,
    SignalCategory::Weather,
    SignalCategory::Road,
];

pub fn signal_title(category: SignalCategory) -> &'static str {
    match category {
        SignalCategory::Rail => "Rail disruption",
        SignalCategory::Road => "Road accident",
        SignalCategory::Weather => "Unhealthy air quality",
    }
}

const MAX_INDIVIDUAL_BROWSER_NOTIFICATIONS: usize = 3;

/// Compare current signals with the last seen ids. A category's first load only
/// records a baseline; a category that hasn't loaded is left alone.
pub fn diff_signals(
    seen: &SeenSignals,
    signals: &[SignalRow],
    loaded: &SignalsLoaded,
) -> (Vec<SignalRow>, SeenSignals) {
    let mut fresh = Vec::new();
    let mut next_seen = seen.clone();
    for category in SIGNAL_CATEGORIES {
        if !loaded.get(category) {
            continue;
        }
        // Preserve insertion order of ids in a BTreeMap-like fashion: the TS
        // builds a Map keyed by id, so later duplicates replace earlier ones.
        let mut rows: BTreeMap<String, SignalRow> = BTreeMap::new();
        for signal in signals.iter().filter(|s| s.category == category) {
            rows.insert(signal.id.clone(), signal.clone());
        }
        if let Some(previous) = seen.get(&category) {
            let known: BTreeSet<&str> = previous.iter().map(|s| s.as_str()).collect();
            for (id, signal) in &rows {
                if !known.contains(id.as_str()) {
                    fresh.push(signal.clone());
                }
            }
        }
        next_seen.insert(category, rows.keys().cloned().collect());
    }
    (fresh, next_seen)
}

/// Apply the user's choices to new signals. `scoped_ids` limits alerts without
/// affecting what counts as new.
pub fn select_notifiable(
    fresh: &[SignalRow],
    prefs: &NotificationPrefs,
    scoped_ids: &HashSet<String>,
) -> Vec<SignalRow> {
    fresh
        .iter()
        .filter(|signal| {
            let enabled = match signal.category {
                SignalCategory::Rail => prefs.rail,
                SignalCategory::Road => prefs.road,
                SignalCategory::Weather => prefs.weather,
            };
            enabled
                && scoped_ids.contains(&signal.id)
                && (signal.category != SignalCategory::Road || signal.tone == SignalTone::Danger)
        })
        .cloned()
        .collect()
}

/// Add new signals to a toast/alert list, drop cleared ones, refresh retained
/// rows from the latest feed. Returns `None` when nothing changed (so callers
/// can keep the same array reference).
pub fn merge_active_signals(
    current: &[SignalRow],
    fresh: &[SignalRow],
    active: &[SignalRow],
) -> Option<Vec<SignalRow>> {
    let latest: HashMap<&str, &SignalRow> = active.iter().map(|s| (s.id.as_str(), s)).collect();
    let mut kept: Vec<SignalRow> = Vec::new();
    let mut changed = false;
    for signal in current {
        match latest.get(signal.id.as_str()) {
            None => changed = true,
            Some(next) => {
                if next.category != signal.category
                    || next.tone != signal.tone
                    || next.text != signal.text
                {
                    kept.push((*next).clone());
                    changed = true;
                } else {
                    kept.push(signal.clone());
                }
            }
        }
    }
    let kept_ids: HashSet<&str> = kept.iter().map(|s| s.id.as_str()).collect();
    let added: Vec<SignalRow> = fresh
        .iter()
        .filter(|s| !kept_ids.contains(s.id.as_str()))
        .cloned()
        .collect();
    if !changed && added.is_empty() {
        None
    } else {
        kept.extend(added);
        Some(kept)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BrowserNotificationPayload {
    pub tag: String,
    pub title: String,
    pub body: String,
}

/// Collapse a burst into one summary rather than flooding the tray.
pub fn build_browser_notifications(signals: &[SignalRow]) -> Vec<BrowserNotificationPayload> {
    if signals.len() > MAX_INDIVIDUAL_BROWSER_NOTIFICATIONS {
        return vec![BrowserNotificationPayload {
            tag: "argus:signal-summary".into(),
            title: format!("{} new ARGUS signals", signals.len()),
            body: "Open the dashboard to review them.".into(),
        }];
    }
    signals
        .iter()
        .map(|signal| BrowserNotificationPayload {
            tag: signal.id.clone(),
            title: signal_title(signal.category).into(),
            body: signal.text.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{PsiStatus, RegionalAirReadings};

    fn signal(id: &str, category: SignalCategory, tone: SignalTone) -> SignalRow {
        SignalRow {
            id: id.into(),
            category,
            tone,
            text: format!("text {}", id),
        }
    }
    fn weather(psi_status: PsiStatus, psi: Option<f64>) -> WeatherData {
        WeatherData {
            temperature: None,
            humidity: None,
            psi,
            psi_status,
            psi_regions: RegionalAirReadings::default(),
            pm25_one_hour: None,
            pm25_one_hour_band: crate::types::Pm25Band::Unknown,
            pm25_one_hour_regions: RegionalAirReadings::default(),
            uv: None,
            uv_status: crate::types::UvStatus::Unknown,
            four_day: Vec::new(),
            forecast: String::new(),
            last_updated: String::new(),
        }
    }

    #[test]
    fn digest_orders_rail_road_weather() {
        let alerts = vec![TrainServiceAlert {
            status: TrainAlertStatus::Disrupted,
            message: "No service. Use buses.".into(),
            affected_lines: vec!["EWL".into(), "NSL".into()],
            affected_stations: Vec::new(),
            start_time: None,
            end_time: None,
        }];
        let incidents = vec![TrafficIncident {
            id: "1".into(),
            incident_type: "Accident".into(),
            lat: 1.0,
            lng: 103.0,
            message: "Road blocked".into(),
        }];
        let w = weather(PsiStatus::Unhealthy, Some(151.0));
        let digest = build_signal_digest(&DigestInput {
            train_alerts: &alerts,
            incidents: &incidents,
            weather: &w,
        });
        assert_eq!(
            digest.iter().map(|r| r.category).collect::<Vec<_>>(),
            [
                SignalCategory::Rail,
                SignalCategory::Road,
                SignalCategory::Weather
            ]
        );
        assert_eq!(digest[0].text, "EWL, NSL: No service");
        assert_eq!(digest[2].text, "PSI 151 (Unhealthy)");
        assert_eq!(digest[2].tone, SignalTone::Danger);
    }

    #[test]
    fn digest_tones_and_psi_threshold_and_stable_ids() {
        let incidents = vec![
            TrafficIncident {
                id: "1".into(),
                incident_type: "Accident".into(),
                lat: 1.0,
                lng: 103.0,
                message: "a".into(),
            },
            TrafficIncident {
                id: "2".into(),
                incident_type: "Vehicle Breakdown".into(),
                lat: 1.0,
                lng: 103.0,
                message: "b".into(),
            },
            TrafficIncident {
                id: "3".into(),
                incident_type: "Other".into(),
                lat: 1.0,
                lng: 103.0,
                message: "c".into(),
            },
        ];
        let good = weather(PsiStatus::Good, Some(42.0));
        let digest = build_signal_digest(&DigestInput {
            train_alerts: &[],
            incidents: &incidents,
            weather: &good,
        });
        assert_eq!(
            digest.iter().map(|r| r.tone).collect::<Vec<_>>(),
            [SignalTone::Danger, SignalTone::Warning, SignalTone::Muted]
        );
        let moderate = weather(PsiStatus::Moderate, Some(100.0));
        assert!(
            build_signal_digest(&DigestInput {
                train_alerts: &[],
                incidents: &[],
                weather: &moderate
            })
            .is_empty()
        );
    }

    #[test]
    fn diff_signals_baseline_fresh_and_recovery() {
        let a = signal("a", SignalCategory::Road, SignalTone::Danger);
        let b = signal("b", SignalCategory::Road, SignalTone::Danger);
        let all = SignalsLoaded::all_loaded();
        let (fresh, seen) = diff_signals(&SeenSignals::new(), &[a.clone(), b.clone()], &all);
        assert!(fresh.is_empty());
        assert_eq!(seen[&SignalCategory::Road], ["a", "b"]);
        let (fresh, _seen) = diff_signals(&seen, &[a.clone(), b.clone()], &all);
        assert!(fresh.is_empty());
        let mut baseline = SeenSignals::new();
        baseline.insert(SignalCategory::Road, vec!["a".into()]);
        let (fresh, _) = diff_signals(&baseline, &[a, b], &all);
        assert_eq!(
            fresh.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            ["b"]
        );
    }

    #[test]
    fn diff_signals_priority_order_and_unloaded() {
        let all = SignalsLoaded::all_loaded();
        let signals = vec![
            signal("a", SignalCategory::Road, SignalTone::Danger),
            signal("r", SignalCategory::Rail, SignalTone::Danger),
            signal("r2", SignalCategory::Rail, SignalTone::Danger),
            signal("p", SignalCategory::Weather, SignalTone::Danger),
        ];
        let mut baseline = SeenSignals::new();
        for cat in SIGNAL_CATEGORIES {
            baseline.insert(cat, Vec::new());
        }
        let (fresh, _) = diff_signals(&baseline, &signals, &all);
        assert_eq!(
            fresh.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            ["r", "r2", "p", "a"]
        );
        let partial = SignalsLoaded {
            rail: true,
            road: false,
            weather: false,
        };
        let (_, seen) = diff_signals(&SeenSignals::new(), &signals, &partial);
        assert!(!seen.contains_key(&SignalCategory::Road));
    }

    #[test]
    fn select_notifiable_filters_road_and_scope() {
        let fresh = vec![
            signal("crash", SignalCategory::Road, SignalTone::Danger),
            signal("breakdown", SignalCategory::Road, SignalTone::Warning),
        ];
        let scoped: HashSet<String> = ["crash", "breakdown"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let out = select_notifiable(&fresh, &NotificationPrefs::default(), &scoped);
        assert_eq!(
            out.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            ["crash"]
        );
    }

    #[test]
    fn merge_active_signals_and_browser_payloads() {
        let old = signal("old", SignalCategory::Road, SignalTone::Danger);
        let kept = signal("kept", SignalCategory::Road, SignalTone::Danger);
        let new = signal("new", SignalCategory::Road, SignalTone::Danger);
        let merged = merge_active_signals(
            &[old, kept.clone()],
            std::slice::from_ref(&new),
            &[kept.clone(), new.clone()],
        )
        .unwrap();
        assert_eq!(
            merged.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            ["kept", "new"]
        );
        let one = [kept];
        assert!(merge_active_signals(&one, &[], &one).is_none());
        let burst: Vec<_> = ["a", "b", "c", "d"]
            .iter()
            .map(|id| signal(id, SignalCategory::Road, SignalTone::Danger))
            .collect();
        let summary = build_browser_notifications(&burst);
        assert_eq!(summary.len(), 1);
        assert_eq!(summary[0].title, "4 new ARGUS signals");
    }
}
