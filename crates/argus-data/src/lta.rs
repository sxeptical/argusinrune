//! LTA DataMall clients: bus stops/arrivals/routes data, traffic cameras and
//! incidents, train service alerts, carpark availability, EV charging points.
//!
//! Pure normalizers are public so tests can exercise them against fixture JSON
//! without a network. The `Client` methods keep the TS cache keys, TTLs,
//! timeouts, `$skip` pagination and 404-as-empty behaviour.

use std::time::Duration;

use argus_core::{
    api_types::{
        LtaBusArrivalsResponse, LtaEvcBatchResponse, LtaTrafficIncidentsResponse,
        LtaTrainServiceAlertsResponse, NumberOrString, RawCarparkAvailability, RawEvLocation,
        RawTrafficIncident,
    },
    mrt_network::lta_line_display_name,
    types::{
        CarparkAvailability, EvChargingLocation, TrainAlertStatus, TrainServiceAlert,
        TrafficCamera, TrafficIncident,
    },
};
use serde::{Deserialize, de::DeserializeOwned};

use crate::{
    Client, Result,
    error::Error,
    health::Source,
    http::DEFAULT_TIMEOUT_MS,
    util::short_hash,
    wire::{WireCarparkResponse, WireEvBatchFile, WireTrafficImagesResponse},
};

const LTA_BASE_URL: &str = "https://datamall2.mytransport.sg/ltaodataservice";
const BUS_STOPS_TIMEOUT_MS: u64 = 35_000;
const CARPARKS_TIMEOUT_MS: u64 = 30_000;
const EV_BATCH_TIMEOUT_MS: u64 = 30_000;
const SAFE_CAMERA_HOSTS: &[&str] = &[
    "https://images.data.gov.sg/",
    "https://datamall2.mytransport.sg/",
    "https://dm-traffic-camera-itsc.s3.ap-southeast-1.amazonaws.com/",
];
const SAFE_EV_BATCH_PREFIX: &str =
    "https://dmprod-datasets.s3.ap-southeast-1.amazonaws.com/";

#[derive(Deserialize)]
struct ValuePage<A> {
    value: Vec<A>,
}

// ---------- pure normalizers ----------

/// Camera images must come from one of the known LTA hosts.
pub fn is_safe_camera_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    SAFE_CAMERA_HOSTS.iter().any(|h| lower.starts_with(h))
}

/// 404 from an LTA endpoint means "nothing here", not an outage.
pub fn is_trusted_ev_batch_link(link: &str) -> bool {
    link.to_ascii_lowercase().starts_with(SAFE_EV_BATCH_PREFIX)
}

pub fn normalize_traffic_incidents(rows: &[RawTrafficIncident]) -> Vec<TrafficIncident> {
    rows.iter()
        .filter(|i| i.latitude.is_finite() && i.longitude.is_finite())
        .map(|i| TrafficIncident {
            id: short_hash(&format!(
                "{}|{}|{}",
                i.message, i.latitude, i.longitude
            )),
            incident_type: i.incident_type.clone(),
            lat: i.latitude,
            lng: i.longitude,
            message: i.message.clone(),
        })
        .collect()
}

fn status_is_disrupted(status: &Option<NumberOrString>) -> bool {
    status.as_ref().and_then(|s| s.as_f64()) == Some(2.0)
}

fn normalize_alert_message(alert: &argus_core::api_types::LtaTrainAlert) -> String {
    alert
        .message
        .as_ref()
        .map(|rows| {
            rows.iter()
                .filter_map(|entry| entry.content.as_ref())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default()
}

/// Normalize either TrainServiceAlerts wrapper shape (array or all-clear).
pub fn normalize_train_service_alerts(
    payload: &LtaTrainServiceAlertsResponse,
) -> Vec<TrainServiceAlert> {
    payload
        .clone()
        .into_alerts()
        .into_iter()
        .map(|row| {
            let segments = row.affected_segments.clone().unwrap_or_default();
            let mut lines: Vec<String> = Vec::new();
            let mut stations: Vec<String> = Vec::new();
            for segment in &segments {
                if let Some(line) = segment.line.as_ref().map(|l| l.trim()).filter(|l| !l.is_empty()) {
                    let name = lta_line_display_name(line);
                    if !lines.contains(&name) {
                        lines.push(name);
                    }
                }
                if let Some(list) = &segment.stations {
                    for code in list.clone().into_vec() {
                        if !stations.contains(&code) {
                            stations.push(code);
                        }
                    }
                }
            }
            lines.sort();
            TrainServiceAlert {
                status: if status_is_disrupted(&row.status) {
                    TrainAlertStatus::Disrupted
                } else {
                    TrainAlertStatus::Normal
                },
                message: normalize_alert_message(&row),
                affected_lines: lines,
                affected_stations: stations,
                start_time: row.start_time,
                end_time: row.end_time,
            }
        })
        .collect()
}

/// Merge the per-lot-type DataMall rows into one entry per carpark.
pub fn normalize_carpark_availability(
    rows: &[RawCarparkAvailability],
) -> Vec<CarparkAvailability> {
    let mut result: Vec<CarparkAvailability> = Vec::new();
    let mut index: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for row in rows {
        let mut coords = row.location.split_whitespace();
        let lat = coords.next().and_then(|v| v.parse::<f64>().ok());
        let lng = coords.next().and_then(|v| v.parse::<f64>().ok());
        let (Some(lat), Some(lng)) = (lat, lng) else { continue };
        if !lat.is_finite() || !lng.is_finite() {
            continue;
        }
        let id = format!("{}:{}", row.agency, row.car_park_id);
        let idx = match index.get(&id) {
            Some(&i) => i,
            None => {
                let name = {
                    let trimmed = row.development.trim();
                    if trimmed.is_empty() {
                        format!("Carpark {}", row.car_park_id)
                    } else {
                        trimmed.to_string()
                    }
                };
                let idx = result.len();
                result.push(CarparkAvailability {
                    id: id.clone(),
                    name,
                    area: row.area.trim().to_string(),
                    agency: row.agency.clone(),
                    lat,
                    lng,
                    car_lots: None,
                    motorcycle_lots: None,
                    heavy_lots: None,
                });
                index.insert(id, idx);
                idx
            }
        };
        match row.lot_type.as_str() {
            "C" => result[idx].car_lots = Some(row.available_lots),
            "Y" => result[idx].motorcycle_lots = Some(row.available_lots),
            "H" => result[idx].heavy_lots = Some(row.available_lots),
            _ => {}
        }
    }
    result
}

/// Summarize each EV location's charging points into connector counts, peak
/// power and operator/plug lists.
pub fn normalize_ev_charging_locations(rows: &[RawEvLocation]) -> Vec<EvChargingLocation> {
    rows.iter()
        .filter_map(|row| {
            let lat = row.latitude?;
            let lng = row.effective_longitude()?;
            if !lat.is_finite() || !lng.is_finite() {
                return None;
            }
            let mut available = 0i64;
            let mut occupied = 0i64;
            let mut total = 0i64;
            let mut max_power_kw: Option<f64> = None;
            let mut has_dc = false;
            let mut operators: Vec<String> = Vec::new();
            let mut plug_types: Vec<String> = Vec::new();
            for point in &row.charging_points {
                if let Some(op) = point.operator.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
                    if !operators.iter().any(|o| o == op) {
                        operators.push(op.to_string());
                    }
                }
                for plug in &point.plug_types {
                    if let Some(pt) = plug.plug_type.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
                        if !plug_types.iter().any(|p| p == pt) {
                            plug_types.push(pt.to_string());
                        }
                    }
                    if plug.current.as_deref().is_some_and(|c| c.eq_ignore_ascii_case("DC")) {
                        has_dc = true;
                    }
                    if let Some(raw) = plug.power_rating.as_ref().filter(|s| !s.is_empty()) {
                        if let Ok(power) = raw.parse::<f64>() {
                            if power.is_finite() && power > max_power_kw.unwrap_or(0.0) {
                                max_power_kw = Some(power);
                            }
                        }
                    }
                    for charger in &plug.ev_ids {
                        total += 1;
                        match charger.status.as_deref() {
                            Some("1") => available += 1,
                            Some("0") => occupied += 1,
                            _ => {}
                        }
                    }
                }
            }
            Some(EvChargingLocation {
                id: short_hash(&format!("{}|{}|{}|{}", row.name, row.address, lat, lng)),
                name: row.name.trim().to_string(),
                address: row.address.trim().to_string(),
                lat,
                lng,
                available,
                occupied,
                total,
                max_power_kw,
                has_dc,
                operators,
                plug_types,
            })
        })
        .collect()
}

/// Fetch OData pages until a short/empty page ends the walk, failing at the
/// safety cap rather than returning a truncated dataset.
pub fn collect_lta_pages<A, F>(
    mut fetch_page: F,
    page_size: usize,
    max_pages: usize,
    service: &str,
) -> Result<Vec<A>>
where
    F: FnMut(usize) -> Result<Option<Vec<A>>>,
{
    let mut all = Vec::new();
    let mut pages = 0usize;
    let mut reached_end = false;
    while pages < max_pages && !reached_end {
        let page = fetch_page(pages * page_size)?;
        pages += 1;
        match page {
            None => reached_end = true,
            Some(rows) => {
                if rows.len() < page_size {
                    reached_end = true;
                }
                all.extend(rows);
            }
        }
    }
    if !reached_end {
        return Err(Error::external(
            service,
            502,
            format!(
                "{service} pagination hit the {max_pages}-page safety cap without an end marker; refusing to use a truncated dataset"
            ),
        ));
    }
    Ok(all)
}

// ---------- Client methods ----------

impl Client {
    fn lta_get<T: DeserializeOwned>(&self, endpoint: &str, timeout_ms: u64) -> Result<T> {
        let key = self.config.lta_api_key()?;
        self.http.get_json(
            "lta",
            format!("{LTA_BASE_URL}{endpoint}"),
            &[("AccountKey", key), ("Accept", "application/json")],
            timeout_ms,
        )
    }

    pub(crate) fn collect_lta_endpoint<A: DeserializeOwned>(
        &self,
        endpoint: &str,
        page_size: usize,
        max_pages: usize,
        timeout_ms: u64,
    ) -> Result<Vec<A>> {
        collect_lta_pages(
            |skip| match self.lta_get::<ValuePage<A>>(
                &format!("{endpoint}?$skip={skip}"),
                timeout_ms,
            ) {
                Ok(page) => Ok(Some(page.value)),
                Err(e) if e.is_not_found() => Ok(None),
                Err(e) => Err(e),
            },
            page_size,
            max_pages,
            "lta",
        )
    }

    pub fn get_bus_stops(&self) -> Result<Vec<argus_core::types::BusStop>> {
        let result = self.cache.get("bus-stops", Duration::from_secs(24 * 60 * 60), || {
            self.collect_lta_endpoint::<argus_core::api_types::LtaBusStopRow>(
                "/BusStops", 500, 20, BUS_STOPS_TIMEOUT_MS,
            )
            .map(|rows| {
                rows.into_iter()
                    .map(|r| argus_core::types::BusStop {
                        bus_stop_code: r.bus_stop_code,
                        road_name: r.road_name,
                        description: r.description,
                        latitude: r.latitude,
                        longitude: r.longitude,
                    })
                    .collect()
            })
        });
        self.finish(Source::BusStops, result)
    }

    pub fn get_bus_arrivals(&self, stop_id: &str) -> Result<Vec<argus_core::types::BusArrival>> {
        crate::validate_bus_stop_id(stop_id)?;
        let key = format!("bus-arrivals-{stop_id}");
        let result = self.cache.get(
            &key,
            Duration::from_secs(15),
            || {
                let response = self.lta_get::<LtaBusArrivalsResponse>(
                    &format!(
                        "/v3/BusArrival?BusStopCode={}",
                        urlencode(stop_id)
                    ),
                    DEFAULT_TIMEOUT_MS,
                )?;
                Ok(response.services)
            },
        );
        self.finish(Source::BusArrivals, result)
    }

    pub fn get_traffic_cameras(&self) -> Result<Vec<TrafficCamera>> {
        let result = self.cache.get("traffic-cameras", Duration::from_secs(60), || {
            let payload = match self.lta_get::<WireTrafficImagesResponse>(
                "/Traffic-Imagesv2",
                DEFAULT_TIMEOUT_MS,
            ) {
                Ok(p) => p,
                Err(e) if e.is_not_found() => return Ok(vec![]),
                Err(e) => return Err(e),
            };
            Ok(payload
                .into_cameras()
                .into_iter()
                .filter(|c| c.latitude.is_finite() && c.longitude.is_finite() && is_safe_camera_url(&c.image_link))
                .map(|c| TrafficCamera {
                    camera_id: c.camera_id.clone(),
                    latitude: c.latitude,
                    longitude: c.longitude,
                    image_link: c.image_link,
                    location: format!("Camera {}", c.camera_id),
                })
                .collect())
        });
        self.finish(Source::Cameras, result)
    }

    pub fn get_traffic_incidents(&self) -> Result<Vec<TrafficIncident>> {
        let result = self.cache.get("incidents", Duration::from_secs(60), || {
            let payload = self.lta_get::<LtaTrafficIncidentsResponse>(
                "/TrafficIncidents",
                DEFAULT_TIMEOUT_MS,
            )?;
            Ok(normalize_traffic_incidents(&payload.value))
        });
        self.finish(Source::Incidents, result)
    }

    pub fn get_train_service_alerts(&self) -> Result<Vec<TrainServiceAlert>> {
        let result = self
            .cache
            .get("train-service-alerts", Duration::from_secs(60), || {
                let payload = self.lta_get::<LtaTrainServiceAlertsResponse>(
                    "/TrainServiceAlerts",
                    DEFAULT_TIMEOUT_MS,
                )?;
                Ok(normalize_train_service_alerts(&payload))
            });
        self.finish(Source::TrainAlerts, result)
    }

    pub fn get_carpark_availability(&self) -> Result<Vec<CarparkAvailability>> {
        let result = self.cache.get("carpark-availability", Duration::from_secs(60), || {
            let rows = collect_lta_pages(
                |skip| {
                    match self.lta_get::<WireCarparkResponse>(
                        &format!("/CarParkAvailabilityv2?$skip={skip}"),
                        CARPARKS_TIMEOUT_MS,
                    ) {
                        Ok(page) => Ok(Some(
                            page.value
                                .into_iter()
                                .map(argus_core::api_types::RawCarparkAvailability::from)
                                .collect(),
                        )),
                        Err(e) if e.is_not_found() => Ok(None),
                        Err(e) => Err(e),
                    }
                },
                500,
                12,
                "lta",
            )?;
            Ok(normalize_carpark_availability(&rows))
        });
        self.finish(Source::Carparks, result)
    }

    pub fn get_ev_charging_locations(&self) -> Result<Vec<EvChargingLocation>> {
        let result = self.cache.get("ev-charging-locations", Duration::from_secs(5 * 60), || {
            let batch = self.lta_get::<LtaEvcBatchResponse>("/EVCBatch", DEFAULT_TIMEOUT_MS)?;
            let link = batch.value.first().map(|row| row.link.as_str());
            let link = link.filter(|l| is_trusted_ev_batch_link(l)).ok_or_else(|| {
                Error::external("lta", 502, "EVCBatch did not return a trusted snapshot link")
            })?;
            // The presigned link carries its own auth; never forward AccountKey.
            let file = self.http.get_json::<WireEvBatchFile>(
                "lta-ev-batch",
                link,
                &[("Accept", "application/json")],
                EV_BATCH_TIMEOUT_MS,
            )?;
            let file: argus_core::api_types::LtaEvBatchFile = file.into();
            Ok(normalize_ev_charging_locations(&file.ev_locations_data))
        });
        self.finish(Source::EvChargers, result)
    }
}

/// Minimal percent-encoding for a bus stop code (digits only in practice).
fn urlencode(value: &str) -> String {
    let mut out = String::new();
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_core::api_types::{
        EvChargingPoint, EvId, EvPlugType, LtaTrainAlert, MessageContent,
        StationList, TrainAlertSegment,
    };

    #[test]
    fn camera_url_allowlist() {
        assert!(is_safe_camera_url(
            "https://images.data.gov.sg/c1.jpg"
        ));
        assert!(is_safe_camera_url(
            "https://dm-traffic-camera-itsc.s3.ap-southeast-1.amazonaws.com/cam.jpg"
        ));
        assert!(!is_safe_camera_url("https://evil.example/cam.jpg"));
    }

    #[test]
    fn incident_normalization_hashes_and_filters() {
        let rows = vec![
            RawTrafficIncident {
                incident_type: "Accident".into(),
                latitude: 1.3,
                longitude: 103.8,
                message: "Crash on CTE".into(),
            },
            RawTrafficIncident {
                incident_type: "Obstacle".into(),
                latitude: f64::NAN,
                longitude: 103.81,
                message: "Bad row".into(),
            },
        ];
        let out = normalize_traffic_incidents(&rows);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id.len(), 16);
        assert_eq!(out[0].incident_type, "Accident");
    }

    #[test]
    fn carpark_rows_merge_by_lot_type() {
        let row = |id: &str, lot: &str, lots: i64| RawCarparkAvailability {
            car_park_id: id.into(),
            area: "".into(),
            development: "BLK 78B TOA PAYOH CENTRAL".into(),
            location: "1.33489 103.84857".into(),
            available_lots: lots,
            lot_type: lot.into(),
            agency: "HDB".into(),
        };
        let out = normalize_carpark_availability(&[
            row("TPMN", "C", 421),
            row("TPMN", "Y", 12),
            row("TPMN", "X", 99),
        ]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, "HDB:TPMN");
        assert_eq!(out[0].car_lots, Some(421));
        assert_eq!(out[0].motorcycle_lots, Some(12));
        assert_eq!(out[0].heavy_lots, None);
    }

    #[test]
    fn ev_normalization_counts_and_peak_power() {
        let loc = RawEvLocation {
            name: " CLARKE QUAY ".into(),
            address: "3E RIVER VALLEY ROAD".into(),
            latitude: Some(1.290314),
            longtitude: Some(103.846728),
            longitude: None,
            charging_points: vec![EvChargingPoint {
                operator: Some("SP MOBILITY".into()),
                plug_types: vec![EvPlugType {
                    plug_type: Some("Combo 2".into()),
                    current: Some("DC".into()),
                    power_rating: Some("50".into()),
                    ev_ids: vec![
                        EvId { status: Some("1".into()) },
                        EvId { status: Some("0".into()) },
                    ],
                }],
            }],
        };
        let out = normalize_ev_charging_locations(&[loc]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].available, 1);
        assert_eq!(out[0].occupied, 1);
        assert_eq!(out[0].total, 2);
        assert_eq!(out[0].max_power_kw, Some(50.0));
        assert!(out[0].has_dc);
        assert_eq!(out[0].name, "CLARKE QUAY");
    }

    #[test]
    fn train_alerts_map_codes_and_dedupe() {
        let payload = LtaTrainServiceAlertsResponse::Many {
            value: vec![LtaTrainAlert {
                status: Some(NumberOrString::Number(2.0)),
                message: Some(vec![
                    MessageContent { content: Some(" No service ".into()) },
                    MessageContent { content: None },
                ]),
                affected_segments: Some(vec![
                    TrainAlertSegment {
                        line: Some("EWL".into()),
                        stations: Some(StationList::Csv("EW21, EW22 ,,EW23 ".into())),
                    },
                    TrainAlertSegment {
                        line: Some("NEL".into()),
                        stations: Some(StationList::List(vec!["NE1".into(), "EW21".into()])),
                    },
                ]),
                start_time: Some("2026-09-09T08:15:00".into()),
                end_time: None,
            }],
        };
        let out = normalize_train_service_alerts(&payload);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].status, TrainAlertStatus::Disrupted);
        assert_eq!(out[0].message, "No service");
        assert_eq!(out[0].affected_lines, ["East West Line", "North East Line"]);
        assert_eq!(out[0].affected_stations, ["EW21", "EW22", "EW23", "NE1"]);
    }

    #[test]
    fn pagination_stops_at_short_page_and_caps() {
        let mut skips = vec![];
        let rows = collect_lta_pages(
            |skip| {
                skips.push(skip);
                Ok::<_, Error>(Some(vec![skip; if skip == 0 { 3 } else { 1 }]))
            },
            3,
            10,
            "lta",
        )
        .unwrap();
        assert_eq!(skips, [0, 3]);
        assert_eq!(rows.len(), 4);

        let err = collect_lta_pages::<i32, _>(|_| Ok(Some(vec![1; 3])), 3, 2, "lta").unwrap_err();
        assert_eq!(err.status(), 502);
        assert!(err.message().contains("safety cap"));
    }

    #[test]
    fn batch_link_allowlist() {
        assert!(is_trusted_ev_batch_link(
            "https://dmprod-datasets.s3.ap-southeast-1.amazonaws.com/ev-batch/EVBatch.json"
        ));
        assert!(!is_trusted_ev_batch_link("https://evil.example/EVBatch.json"));
    }
}
