//! Per-source health, matching the TS source-health panel's source list.
//!
//! The panel used `pending | loading | ok | error`; a desktop client also
//! needs to distinguish a stale snapshot and a missing API key, so those are
//! separate states here. `last_updated_ms` is epoch milliseconds.

use std::{collections::HashMap, sync::Mutex, time::Duration};

use serde::{Deserialize, Serialize};

use crate::{error::Error, time::now_millis};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    BusStops,
    BusArrivals,
    BusRoutes,
    Cameras,
    Incidents,
    Carparks,
    EvChargers,
    Weather,
    News,
    Flights,
    TrainAlerts,
    BusRouter,
    Feedback,
}

impl Source {
    pub fn key(self) -> &'static str {
        match self {
            Source::BusStops => "bus stops",
            Source::BusArrivals => "bus arrivals",
            Source::BusRoutes => "bus routes",
            Source::Cameras => "cameras",
            Source::Incidents => "incidents",
            Source::Carparks => "carparks",
            Source::EvChargers => "ev chargers",
            Source::Weather => "weather",
            Source::News => "news",
            Source::Flights => "flights",
            Source::TrainAlerts => "train alerts",
            Source::BusRouter => "busrouter",
            Source::Feedback => "feedback",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Source::BusStops => "LTA Bus Network",
            Source::BusArrivals => "Bus Arrivals",
            Source::BusRoutes => "Bus Routes",
            Source::Cameras => "Traffic Cameras",
            Source::Incidents => "Road Incidents",
            Source::Carparks => "Carpark Availability",
            Source::EvChargers => "EV Charging Points",
            Source::Weather => "Weather Grid",
            Source::News => "OSINT Stream",
            Source::Flights => "Airspace Feed",
            Source::TrainAlerts => "Train Service Alerts",
            Source::BusRouter => "BusRouter Geometry",
            Source::Feedback => "Visitor Feedback",
        }
    }

    /// How long a successful snapshot is considered fresh.
    pub fn ttl(self) -> Duration {
        match self {
            Source::BusStops | Source::BusRoutes => Duration::from_secs(24 * 60 * 60),
            Source::BusArrivals => Duration::from_secs(15),
            Source::Cameras | Source::Incidents | Source::Carparks | Source::TrainAlerts => {
                Duration::from_secs(60)
            }
            Source::EvChargers | Source::Weather => Duration::from_secs(5 * 60),
            Source::News => Duration::from_secs(15 * 60),
            Source::Flights => Duration::from_secs(15),
            Source::BusRouter => Duration::from_secs(24 * 60 * 60),
            Source::Feedback => Duration::from_secs(60),
        }
    }

    /// Every source the dashboard panel shows (flights included — the TS
    /// build showed it as "disabled" when the feature flag was off).
    pub const ALL: &'static [Source] = &[
        Source::BusStops,
        Source::Cameras,
        Source::Incidents,
        Source::Carparks,
        Source::EvChargers,
        Source::Weather,
        Source::News,
        Source::TrainAlerts,
        Source::Flights,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Ok,
    Stale,
    Error,
    Unconfigured,
    Loading,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceHealth {
    pub source: Source,
    pub label: &'static str,
    pub status: HealthStatus,
    pub last_updated_ms: Option<i64>,
    pub message: Option<String>,
}

#[derive(Default)]
pub struct HealthRegistry {
    entries: Mutex<HashMap<Source, SourceHealth>>,
}

impl HealthRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    fn status_for(
        source: Source,
        status: HealthStatus,
        message: Option<String>,
        last: Option<i64>,
    ) -> SourceHealth {
        let status = if status == HealthStatus::Ok {
            match last {
                Some(ts)
                    if now_millis().saturating_sub(ts) > source.ttl().as_millis() as i64 * 2 =>
                {
                    HealthStatus::Stale
                }
                _ => HealthStatus::Ok,
            }
        } else {
            status
        };
        SourceHealth {
            source,
            label: source.label(),
            status,
            last_updated_ms: last,
            message,
        }
    }

    pub fn mark_ok(&self, source: Source) {
        let now = now_millis();
        let entry = Self::status_for(source, HealthStatus::Ok, None, Some(now));
        self.entries.lock().unwrap().insert(source, entry);
    }

    /// Record a success with an explanatory note (e.g. "primary key missing,
    /// using fallback").
    pub fn mark_ok_with_message(&self, source: Source, message: impl Into<String>) {
        let now = now_millis();
        let entry = Self::status_for(source, HealthStatus::Ok, Some(message.into()), Some(now));
        self.entries.lock().unwrap().insert(source, entry);
    }

    pub fn mark_error(&self, source: Source, error: &Error) {
        let status = if matches!(error, Error::NotConfigured { .. }) {
            HealthStatus::Unconfigured
        } else {
            HealthStatus::Error
        };
        let entry = Self::status_for(source, status, Some(error.message().to_string()), None);
        self.entries.lock().unwrap().insert(source, entry);
    }

    pub fn mark_unconfigured(&self, source: Source, message: impl Into<String>) {
        let entry = Self::status_for(
            source,
            HealthStatus::Unconfigured,
            Some(message.into()),
            None,
        );
        self.entries.lock().unwrap().insert(source, entry);
    }

    pub fn get(&self, source: Source) -> SourceHealth {
        let existing = self.entries.lock().unwrap().get(&source).cloned();
        match existing {
            Some(entry) => {
                Self::status_for(source, entry.status, entry.message, entry.last_updated_ms)
            }
            None => SourceHealth {
                source,
                label: source.label(),
                status: HealthStatus::Loading,
                last_updated_ms: None,
                message: None,
            },
        }
    }

    /// Snapshot of all known sources, in panel order.
    pub fn list(&self) -> Vec<SourceHealth> {
        Source::ALL.iter().map(|s| self.get(*s)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok_error_unconfigured_and_stale() {
        let reg = HealthRegistry::new();
        assert_eq!(reg.get(Source::Weather).status, HealthStatus::Loading);
        reg.mark_ok(Source::Weather);
        assert_eq!(reg.get(Source::Weather).status, HealthStatus::Ok);
        reg.mark_error(Source::Weather, &Error::external("x", 503, "down"));
        let h = reg.get(Source::Weather);
        assert_eq!(h.status, HealthStatus::Error);
        assert_eq!(h.message.as_deref(), Some("down"));
        reg.mark_unconfigured(Source::Flights, "no key");
        assert_eq!(reg.get(Source::Flights).status, HealthStatus::Unconfigured);

        // An old successful snapshot is reported stale without another fetch.
        reg.entries.lock().unwrap().insert(
            Source::Flights,
            SourceHealth {
                source: Source::Flights,
                label: Source::Flights.label(),
                status: HealthStatus::Ok,
                last_updated_ms: Some(now_millis() - Source::Flights.ttl().as_millis() as i64 * 3),
                message: None,
            },
        );
        assert_eq!(reg.get(Source::Flights).status, HealthStatus::Stale);
    }
}
