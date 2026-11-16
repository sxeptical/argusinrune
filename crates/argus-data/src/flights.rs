//! AviationStack flights with OpenSky and last-good snapshot fallbacks.
//! Ports `lib/api-clients/flights.ts`.

use std::time::Duration;

use argus_core::{
    api_types::{AviationStackFlight, AviationStackResponse, OpenSkyResponse, OpenSkyState},
    types::{FlightDirection, FlightState},
};

use crate::{Client, Result, error::Error, health::Source};

const AVIATIONSTACK_BASE_URL: &str = "https://api.aviationstack.com/v1";
const OPENSKY_BASE_URL: &str = "https://opensky-network.org/api";
const FLIGHT_TIMEOUT_MS: u64 = 6_000;
const FLIGHTS_TTL: Duration = Duration::from_secs(15);
const FALLBACK_KEY: &str = "flights-sg-fallback";
const FALLBACK_MAX_AGE: Duration = Duration::from_secs(10 * 60);
const MAX_FLIGHTS: usize = 120;
const SG_BOUNDS: (f64, f64, f64, f64) = (1.15, 103.45, 1.5, 104.15); // lamin, lomin, lamax, lomax
const CHANGI: (f64, f64) = (1.3644, 103.9915);

fn normalize_code(code: Option<&str>) -> &str {
    code.unwrap_or("").trim()
}

fn is_singapore_airport(icao: Option<&str>, iata: Option<&str>) -> bool {
    let icao = normalize_code(icao).to_ascii_uppercase();
    let iata = normalize_code(iata).to_ascii_uppercase();
    ["WSSS", "WSSL"].contains(&icao.as_str()) || ["SIN", "XSP"].contains(&iata.as_str())
}

pub fn bearing_degrees(from_lat: f64, from_lon: f64, to_lat: f64, to_lon: f64) -> f64 {
    let (from_lat, from_lon, to_lat, to_lon) = (
        from_lat.to_radians(),
        from_lon.to_radians(),
        to_lat.to_radians(),
        to_lon.to_radians(),
    );
    let d_lon = to_lon - from_lon;
    let y = d_lon.sin() * to_lat.cos();
    let x = from_lat.cos() * to_lat.sin() - from_lat.sin() * to_lat.cos() * d_lon.cos();
    (y.atan2(x).to_degrees() + 360.0) % 360.0
}

/// Inbound/outbound from Singapore airports, else by track toward Changi.
pub fn classify_flight_direction(
    latitude: f64,
    longitude: f64,
    track: Option<f64>,
) -> FlightDirection {
    let Some(track) = track.filter(|t| t.is_finite()) else {
        return FlightDirection::Transit;
    };
    let heading = bearing_degrees(latitude, longitude, CHANGI.0, CHANGI.1);
    let diff = (track - heading).abs();
    let normalized = if diff > 180.0 { 360.0 - diff } else { diff };
    if normalized <= 55.0 {
        FlightDirection::Inbound
    } else if normalized >= 125.0 {
        FlightDirection::Outbound
    } else {
        FlightDirection::Transit
    }
}

fn finite(value: Option<f64>) -> Option<f64> {
    value.filter(|v| v.is_finite())
}

pub fn from_aviation_stack(item: &AviationStackFlight) -> Option<FlightState> {
    let live = item.live.as_ref()?;
    let latitude = finite(live.latitude)?;
    let longitude = finite(live.longitude)?;
    let on_ground = live.is_ground.unwrap_or(false);
    if on_ground {
        return None;
    }
    let track = finite(live.direction);
    let dep = item.departure.as_ref();
    let arr = item.arrival.as_ref();
    let dep_sg = is_singapore_airport(
        dep.and_then(|d| d.icao.as_deref()),
        dep.and_then(|d| d.iata.as_deref()),
    );
    let arr_sg = is_singapore_airport(
        arr.and_then(|a| a.icao.as_deref()),
        arr.and_then(|a| a.iata.as_deref()),
    );
    let direction = if arr_sg && !dep_sg {
        FlightDirection::Inbound
    } else if dep_sg && !arr_sg {
        FlightDirection::Outbound
    } else {
        classify_flight_direction(latitude, longitude, track)
    };

    let flight = item.flight.as_ref();
    let airline = item.airline.as_ref();
    let aircraft = item.aircraft.as_ref();
    let flight_code = {
        let icao = normalize_code(flight.and_then(|f| f.icao.as_deref())).trim();
        let iata = normalize_code(flight.and_then(|f| f.iata.as_deref())).trim();
        let composed = format!(
            "{}{}",
            normalize_code(airline.and_then(|a| a.iata.as_deref())).trim(),
            normalize_code(flight.and_then(|f| f.number.as_deref())).trim()
        );
        if !icao.is_empty() {
            icao.to_string()
        } else if !iata.is_empty() {
            iata.to_string()
        } else {
            composed
        }
    };
    let icao24 = {
        let reg = normalize_code(aircraft.and_then(|a| a.icao24.as_deref())).trim();
        let tail = normalize_code(aircraft.and_then(|a| a.registration.as_deref())).trim();
        if !reg.is_empty() {
            reg.to_string()
        } else if !tail.is_empty() {
            tail.to_string()
        } else if !flight_code.is_empty() {
            flight_code.clone()
        } else {
            "unknown".to_string()
        }
    };
    let callsign = if flight_code.is_empty() {
        icao24.to_ascii_uppercase()
    } else {
        flight_code
    };
    let last_contact = live
        .updated
        .as_deref()
        .and_then(crate::time::parse_timestamp)
        .map(|ms| (ms / 1000) as f64);

    Some(FlightState {
        id: icao24.clone(),
        icao24,
        callsign,
        origin_country: dep
            .and_then(|d| d.airport.as_deref())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .or_else(|| {
                airline
                    .and_then(|a| a.name.as_deref())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "Unknown".to_string()),
        latitude,
        longitude,
        altitude: finite(live.altitude),
        velocity: finite(live.speed_horizontal).map(|v| v / 3.6),
        track,
        vertical_rate: finite(live.speed_vertical).map(|v| v / 3.6),
        on_ground,
        direction,
        last_contact,
    })
}

pub fn from_open_sky(row: &OpenSkyState) -> Option<FlightState> {
    let icao24 = normalize_code(row.icao24.as_deref()).trim().to_string();
    let latitude = finite(row.latitude)?;
    let longitude = finite(row.longitude)?;
    if icao24.is_empty() || row.on_ground.unwrap_or(false) {
        return None;
    }
    let track = finite(row.true_track);
    let callsign = normalize_code(row.callsign.as_deref()).trim();
    Some(FlightState {
        id: icao24.clone(),
        icao24,
        callsign: if callsign.is_empty() {
            "UNKN".into()
        } else {
            callsign.into()
        },
        origin_country: normalize_code(row.origin_country.as_deref())
            .trim()
            .to_string(),
        latitude,
        longitude,
        altitude: finite(row.baro_altitude),
        velocity: finite(row.velocity),
        track,
        vertical_rate: finite(row.vertical_rate),
        on_ground: false,
        direction: classify_flight_direction(latitude, longitude, track),
        last_contact: finite(row.last_contact),
    })
}

/// Dedupe by icao24 keeping the newest `last_contact`, then sort fastest
/// first and cap at 120 (matches the TS client).
pub fn dedup_and_sort(flights: Vec<FlightState>) -> Vec<FlightState> {
    let mut by_id: std::collections::HashMap<String, FlightState> =
        std::collections::HashMap::new();
    for flight in flights {
        let replace = by_id.get(&flight.icao24).is_none_or(|old| {
            flight.last_contact.unwrap_or(0.0) >= old.last_contact.unwrap_or(0.0)
        });
        if replace {
            by_id.insert(flight.icao24.clone(), flight);
        }
    }
    let mut sorted: Vec<FlightState> = by_id.into_values().collect();
    sorted.sort_by(|a, b| {
        b.velocity
            .unwrap_or(0.0)
            .partial_cmp(&a.velocity.unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    sorted.truncate(MAX_FLIGHTS);
    sorted
}

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

impl Client {
    fn fetch_aviationstack_endpoint(
        &self,
        key: &str,
        filter: &str,
    ) -> Result<Vec<AviationStackFlight>> {
        let url = format!(
            "{AVIATIONSTACK_BASE_URL}/flights?access_key={}&limit=100&{filter}",
            urlencode(key)
        );
        let response = self.http.get_json::<AviationStackResponse>(
            "aviationstack",
            url,
            &[("Accept", "application/json")],
            FLIGHT_TIMEOUT_MS,
        )?;
        if let Some(error) = response.error {
            return Err(Error::external(
                "aviationstack",
                502,
                format!(
                    "Aviationstack error {}: {}",
                    error
                        .code
                        .as_ref()
                        .and_then(|c| c.as_f64())
                        .map(|v| v.to_string())
                        .unwrap_or_default(),
                    error
                        .info
                        .or(error.type_field)
                        .unwrap_or_else(|| "unknown".into())
                ),
            ));
        }
        Ok(response.data.unwrap_or_default())
    }

    fn fetch_flights_from_aviationstack(&self) -> Result<Vec<FlightState>> {
        let key = self.config.aviationstack_api_key()?.to_string();
        let mut all = Vec::new();
        let mut succeeded = 0usize;
        for filter in [
            "dep_icao=WSSS",
            "arr_icao=WSSS",
            "dep_icao=WSSL",
            "arr_icao=WSSL",
        ] {
            match self.fetch_aviationstack_endpoint(&key, filter) {
                Ok(rows) => {
                    succeeded += 1;
                    all.extend(rows);
                }
                Err(_) => continue,
            }
        }
        if succeeded == 0 {
            return Err(Error::external(
                "aviationstack",
                502,
                "All AviationStack endpoints failed",
            ));
        }
        Ok(dedup_and_sort(
            all.iter().filter_map(from_aviation_stack).collect(),
        ))
    }

    fn fetch_flights_from_open_sky(&self) -> Result<Vec<FlightState>> {
        let (lamin, lomin, lamax, lomax) = SG_BOUNDS;
        let url = format!(
            "{OPENSKY_BASE_URL}/states/all?lamin={lamin}&lomin={lomin}&lamax={lamax}&lomax={lomax}"
        );
        let response = self.http.get_json::<OpenSkyResponse>(
            "opensky",
            url,
            &[("Accept", "application/json")],
            FLIGHT_TIMEOUT_MS,
        )?;
        Ok(response
            .states
            .unwrap_or_default()
            .iter()
            .filter_map(from_open_sky)
            .collect())
    }

    pub fn get_flights(&self) -> Result<Vec<FlightState>> {
        let result = self.cache.get("flights-sg", FLIGHTS_TTL, || {
            let mut flights = self.fetch_flights_from_aviationstack().unwrap_or_default();
            if flights.is_empty() {
                flights = self.fetch_flights_from_open_sky().unwrap_or_default();
            }
            let sorted = dedup_and_sort(flights);
            if !sorted.is_empty() {
                self.cache.set(FALLBACK_KEY, &sorted);
                return Ok(sorted);
            }
            // Read the last-good snapshot without replacing it on failure.
            Ok(self
                .cache
                .peek::<Vec<FlightState>>(FALLBACK_KEY, FALLBACK_MAX_AGE)
                .unwrap_or_default())
        });
        let result = self.finish(Source::Flights, result);
        if self.config.aviationstack_api_key.is_none() {
            self.health
                .mark_ok_with_message(Source::Flights, "AVIATIONSTACK_API_KEY not configured");
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_core::api_types::{
        AviationAircraft, AviationAirline, AviationEndpoint, AviationFlightInfo, AviationLive,
    };

    fn flight(icao: &str, arr_icao: Option<&str>) -> AviationStackFlight {
        AviationStackFlight {
            departure: Some(AviationEndpoint {
                airport: Some("Bangkok".into()),
                iata: Some("BKK".into()),
                icao: Some("VTBS".into()),
            }),
            arrival: Some(AviationEndpoint {
                airport: Some("Changi".into()),
                iata: arr_icao.map(|_| "SIN".to_string()),
                icao: arr_icao.map(str::to_string),
            }),
            airline: Some(AviationAirline {
                name: Some("Singapore Airlines".into()),
                iata: Some("SQ".into()),
            }),
            flight: Some(AviationFlightInfo {
                number: Some("5".into()),
                iata: Some("SQ5".into()),
                icao: Some("SIA5".into()),
            }),
            aircraft: Some(AviationAircraft {
                registration: Some("9V-SMP".into()),
                icao24: Some(icao.into()),
            }),
            live: Some(AviationLive {
                updated: Some("2026-09-03T00:00:00Z".into()),
                latitude: Some(1.3644),
                longitude: Some(103.9915),
                altitude: Some(10_000.0),
                direction: Some(180.0),
                speed_horizontal: Some(800.0),
                speed_vertical: Some(0.0),
                is_ground: Some(false),
            }),
        }
    }

    #[test]
    fn aviationstack_normalization_and_direction() {
        let state = from_aviation_stack(&flight("76C0A1", Some("WSSS"))).unwrap();
        assert_eq!(state.direction, FlightDirection::Inbound);
        assert!((state.velocity.unwrap() - 800.0 / 3.6).abs() < 1e-9);
        assert_eq!(
            state.last_contact,
            Some((crate::time::parse_timestamp("2026-09-03T00:00:00Z").unwrap() / 1000) as f64)
        );
        assert_eq!(state.origin_country, "Bangkok");
    }

    #[test]
    fn track_classification() {
        let lat = 1.3;
        let lng = 103.8;
        let heading = bearing_degrees(lat, lng, CHANGI.0, CHANGI.1);
        assert_eq!(
            classify_flight_direction(lat, lng, Some(heading)),
            FlightDirection::Inbound
        );
        assert_eq!(
            classify_flight_direction(lat, lng, Some((heading + 180.0) % 360.0)),
            FlightDirection::Outbound
        );
        assert_eq!(
            classify_flight_direction(lat, lng, None),
            FlightDirection::Transit
        );
    }

    #[test]
    fn open_sky_row_and_dedup_cap() {
        let row = OpenSkyState {
            icao24: Some("76C0A1".into()),
            callsign: Some("SIA321 ".into()),
            origin_country: Some("Singapore".into()),
            last_contact: Some(60.0),
            longitude: Some(103.9915),
            latitude: Some(1.3644),
            baro_altitude: Some(3000.0),
            on_ground: Some(false),
            velocity: Some(250.0),
            true_track: None,
            vertical_rate: Some(0.0),
            ..Default::default()
        };
        let state = from_open_sky(&row).unwrap();
        assert_eq!(state.callsign, "SIA321");
        assert_eq!(state.velocity, Some(250.0));

        let many = (0..150)
            .map(|i| FlightState {
                id: format!("{i}"),
                icao24: format!("{i}"),
                velocity: Some(i as f64),
                ..state.clone()
            })
            .collect();
        let sorted = dedup_and_sort(many);
        assert_eq!(sorted.len(), 120);
        assert_eq!(sorted[0].velocity, Some(149.0));
    }
}
