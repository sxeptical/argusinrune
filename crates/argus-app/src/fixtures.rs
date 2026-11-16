//! Deterministic fixture snapshot used by the `panels` / `poi-dialog` scenes
//! and by reducer tests. This is *not* live data: the default app state has
//! no fetchers installed yet (see `live.rs`).

use argus_core::types::*;

use crate::live::{DataSnapshot, FeedKind, SourceState, SourceStatus, def, default_weather};

fn ok_sources() -> Vec<SourceState> {
    crate::live::FEED_DEFS
        .iter()
        .map(|d: &crate::live::FeedDef| SourceState {
            key: d.kind,
            label: d.label,
            status: SourceStatus::Ok,
        })
        .collect()
}

pub fn demo_snapshot() -> DataSnapshot {
    let bus_stop = |code: &str, desc: &str, road: &str, lat: f64, lon: f64| BusStop {
        bus_stop_code: code.into(),
        description: desc.into(),
        road_name: road.into(),
        latitude: lat,
        longitude: lon,
    };
    let camera = |id: &str, loc: &str, lat: f64, lon: f64| TrafficCamera {
        camera_id: id.into(),
        latitude: lat,
        longitude: lon,
        image_link: String::new(),
        location: loc.into(),
    };
    let flight =
        |id: &str, call: &str, lat: f64, lon: f64, dir: FlightDirection, alt: f64, vel: f64| {
            FlightState {
                id: id.into(),
                icao24: format!("7{a}{b}", a = &id[0..1], b = &id[1..2]),
                callsign: call.into(),
                origin_country: "Singapore".into(),
                latitude: lat,
                longitude: lon,
                altitude: Some(alt),
                velocity: Some(vel),
                track: Some(122.0),
                vertical_rate: None,
                on_ground: false,
                direction: dir,
                last_contact: None,
            }
        };
    let mut weather = default_weather();
    weather.temperature = Some(30.2);
    weather.humidity = Some(78.0);
    weather.psi = Some(58.0);
    weather.psi_status = PsiStatus::Moderate;
    weather.pm25_one_hour = Some(24.0);
    weather.pm25_one_hour_band = Pm25Band::Normal;
    weather.uv = Some(7.0);
    weather.uv_status = UvStatus::High;
    weather.forecast = "Warm with scattered afternoon showers.".into();
    weather.last_updated = "2026-11-17T08:45:00+08:00".into();

    DataSnapshot {
        bus_stops: vec![
            bus_stop(
                "01012",
                "Bukit Merah Interchange",
                "Bukit Merah Central",
                1.2786,
                103.8242,
            ),
            bus_stop(
                "02031",
                "Bras Basah Complex",
                "Victoria St",
                1.2939,
                103.8536,
            ),
            bus_stop(
                "03111",
                "Tanah Merah Stn Exit A",
                "New Upper Changi Rd",
                1.3277,
                103.9456,
            ),
        ],
        cameras: vec![
            camera("1701", "PIE / Changi South Ave 3", 1.3354, 103.9579),
            camera("1702", "Ayer Rajah Expwy / Keppel", 1.2754, 103.8431),
            camera("1703", "Bukit Timah Expwy / Farrer", 1.3210, 103.8200),
            camera("1704", "Central Expwy / Chin Swee", 1.2874, 103.8417),
        ],
        carparks: vec![
            CarparkAvailability {
                id: "P001".into(),
                name: "Bukit Merah Car Park".into(),
                area: "Bukit Merah".into(),
                agency: "HDB".into(),
                lat: 1.2791,
                lng: 103.8248,
                car_lots: Some(42),
                motorcycle_lots: Some(6),
                heavy_lots: None,
            },
            CarparkAvailability {
                id: "P002".into(),
                name: "Raffles Place Car Park".into(),
                area: "Raffles Place".into(),
                agency: "URA".into(),
                lat: 1.2845,
                lng: 103.8512,
                car_lots: Some(9),
                motorcycle_lots: None,
                heavy_lots: None,
            },
        ],
        ev_chargers: vec![EvChargingLocation {
            id: "EV01".into(),
            name: "VivoCity EV Hub".into(),
            address: "1 HarbourFront Walk".into(),
            lat: 1.2644,
            lng: 103.8222,
            available: 3,
            occupied: 5,
            total: 8,
            max_power_kw: Some(50.0),
            has_dc: true,
            operators: vec!["Shell Recharge".into()],
            plug_types: vec!["CCS2".into()],
        }],
        incidents: vec![
            TrafficIncident {
                id: "INC-401".into(),
                incident_type: "Accident".into(),
                lat: 1.3041,
                lng: 103.8791,
                message: "Accident on PIE eastbound after Exit 22. Lane 2 blocked.".into(),
            },
            TrafficIncident {
                id: "INC-402".into(),
                incident_type: "Heavy Traffic".into(),
                lat: 1.2701,
                lng: 103.8360,
                message: "Slow-moving traffic on Ayer Rajah Expressway towards CBD.".into(),
            },
        ],
        train_alerts: vec![TrainServiceAlert {
            status: TrainAlertStatus::Disrupted,
            message: "Minor delay on the East West Line due to a track fault.".into(),
            affected_lines: vec!["EWL".into()],
            affected_stations: vec!["Jurong East".into(), "Boon Lay".into()],
            start_time: Some("2026-11-17T07:12:00+08:00".into()),
            end_time: None,
        }],
        flights: vec![
            flight(
                "41",
                "SQ402",
                1.3701,
                103.9102,
                FlightDirection::Inbound,
                5400.0,
                245.0,
            ),
            flight(
                "42",
                "SQ318",
                1.3444,
                103.9021,
                FlightDirection::Outbound,
                7200.0,
                260.0,
            ),
            flight(
                "43",
                "QF052",
                1.2901,
                103.8820,
                FlightDirection::Transit,
                9800.0,
                280.0,
            ),
            flight(
                "44",
                "TG414",
                1.3140,
                103.9400,
                FlightDirection::Inbound,
                3900.0,
                210.0,
            ),
        ],
        news: vec![
            NewsItem {
                title: "LTA expands bus service coverage in Tampines".into(),
                source: "Channel NewsAsia".into(),
                url: "#".into(),
                published_at: "2026-11-17T08:10:00+08:00".into(),
            },
            NewsItem {
                title: "Singapore records higher afternoon humidity".into(),
                source: "Today Online".into(),
                url: "#".into(),
                published_at: "2026-11-17T07:35:00+08:00".into(),
            },
            NewsItem {
                title: "Changi Airport traffic recovers for seventh straight month".into(),
                source: "The Straits Times".into(),
                url: "#".into(),
                published_at: "2026-11-17T06:50:00+08:00".into(),
            },
        ],
        bus_arrivals: vec![],
        bus_routes: None,
        feedback_status: None,
        weather,
        sources: ok_sources(),
    }
}

/// Mark one source offline in a fixture snapshot (source-health scene).
#[allow(dead_code)]
pub fn with_failed_source(mut snap: DataSnapshot, kind: FeedKind) -> DataSnapshot {
    if let Some(source) = snap.sources.iter_mut().find(|s| s.key == kind) {
        source.status = SourceStatus::Error("request timed out".into());
    }
    snap
}

/// Keep the compiler honest that the feed table is used by fixtures too.
#[allow(dead_code)]
pub fn feed_label(kind: FeedKind) -> &'static str {
    def(kind).label
}
