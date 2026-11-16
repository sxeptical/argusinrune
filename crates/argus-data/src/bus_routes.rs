//! Bus routes: LTA's ordered stop index joined with BusRouter / sgbusdata
//! road-following polylines. Ports `lib/api-clients/bus-routes.ts`.
//!
//! Everything geometric (polyline decoding, pattern scoring, direction
//! selection) is pure and public; only `get_bus_route` touches the network.

use std::{collections::HashMap, time::Duration};

use argus_core::{
    api_types::LtaBusRouteRow,
    types::{BusRouteDirection, BusRouteResponse, BusRouteStop, LngLat},
};
use serde::{Deserialize, Serialize};

use crate::{Client, Result, error::Error, health::Source};

const BUSROUTER_ROUTES_URL: &str = "https://data.busrouter.sg/v1/routes.min.json";
const BUSROUTER_ROUTES_FALLBACK_URL: &str =
    "https://cdn.jsdelivr.net/gh/cheeaun/sgbusdata@master/data/v1/routes.min.json";
const BUS_ROUTES_TIMEOUT_MS: u64 = 90_000;
const BUS_ROUTES_CACHE_TTL_MS: Duration = Duration::from_secs(24 * 60 * 60);
const BUSROUTER_GEOMETRY_CACHE_TTL_MS: Duration = Duration::from_secs(24 * 60 * 60);
const BUSROUTER_GEOMETRY_NEGATIVE_TTL_MS: Duration = Duration::from_secs(5 * 60);
const BUSROUTER_GEOMETRY_TIMEOUT_MS: u64 = 15_000;
const MAX_BUSROUTER_PATH_POINTS: usize = 2_500;
const MAX_PATTERN_MATCH_DISTANCE_M: f64 = 200.0;
const STOP_ON_PATH_TOLERANCE_M: f64 = 60.0;
const MAX_STOP_DISTANCE_PENALTY_M: f64 = 5_000.0;
const LOOP_TERMINUS_RADIUS_M: f64 = 300.0;
const CLOSED_PATH_RADIUS_M: f64 = 200.0;
const BUS_ROUTES_PAGE_SIZE: usize = 500;
const BUS_ROUTES_MAX_PAGES: usize = 128;

fn finite_number(value: &argus_core::api_types::NumberOrString) -> Option<f64> {
    value.as_f64().filter(|v| v.is_finite())
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IndexedBusRouteStop {
    pub bus_stop_code: String,
    pub stop_sequence: f64,
}

/// ServiceNo → Direction → ordered stop codes (pre-join).
pub type BusRoutesIndex = HashMap<String, HashMap<i64, Vec<IndexedBusRouteStop>>>;

pub fn build_bus_routes_index(rows: &[LtaBusRouteRow]) -> BusRoutesIndex {
    let mut raw: BusRoutesIndex = HashMap::new();
    for row in rows {
        let service = row.service_no.trim();
        let code = row.bus_stop_code.trim();
        let (Some(direction), Some(seq)) = (
            finite_number(&row.direction),
            finite_number(&row.stop_sequence),
        ) else {
            continue;
        };
        if service.is_empty() || code.is_empty() {
            continue;
        }
        raw.entry(service.to_string())
            .or_default()
            .entry(direction as i64)
            .or_default()
            .push(IndexedBusRouteStop {
                bus_stop_code: code.to_string(),
                stop_sequence: seq,
            });
    }
    for by_direction in raw.values_mut() {
        for stops in by_direction.values_mut() {
            stops.sort_by(|a, b| {
                a.stop_sequence
                    .partial_cmp(&b.stop_sequence)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
    }
    raw
}

/// Decode a Google-encoded polyline into `[lng, lat]` pairs.
pub fn decode_google_polyline(encoded: &str) -> Vec<LngLat> {
    let bytes = encoded.as_bytes();
    let mut coords = Vec::new();
    let mut index = 0usize;
    let mut lat = 0i64;
    let mut lng = 0i64;
    while index < bytes.len() {
        let mut result = 0i64;
        let mut shift = 0u32;
        loop {
            if index >= bytes.len() {
                return coords;
            }
            let byte = (bytes[index] as i64) - 63;
            index += 1;
            result |= (byte & 0x1f) << shift;
            shift += 5;
            if byte < 0x20 {
                break;
            }
        }
        let dlat = if result & 1 == 1 {
            !(result >> 1)
        } else {
            result >> 1
        };
        lat += dlat;

        result = 0;
        shift = 0;
        loop {
            if index >= bytes.len() {
                return coords;
            }
            let byte = (bytes[index] as i64) - 63;
            index += 1;
            result |= (byte & 0x1f) << shift;
            shift += 5;
            if byte < 0x20 {
                break;
            }
        }
        let dlng = if result & 1 == 1 {
            !(result >> 1)
        } else {
            result >> 1
        };
        lng += dlng;
        coords.push([lng as f64 / 1e5, lat as f64 / 1e5]);
    }
    coords
}

fn downsample_path(path: Vec<LngLat>, max_points: usize) -> Vec<LngLat> {
    if path.len() <= max_points || max_points < 2 {
        return path;
    }
    let last = path.len() - 1;
    (0..max_points)
        .map(|i| path[(i * last) / (max_points - 1)])
        .collect()
}

/// Local planar distance to a segment, accurate at road-snapping scale.
fn distance_to_segment_meters(lng: f64, lat: f64, a: LngLat, b: LngLat) -> f64 {
    let m_per_lng = (lat * std::f64::consts::PI / 180.0).cos() * 111_320.0;
    let m_per_lat = 110_540.0;
    let ax = (a[0] - lng) * m_per_lng;
    let ay = (a[1] - lat) * m_per_lat;
    let dx = (b[0] - lng) * m_per_lng - ax;
    let dy = (b[1] - lat) * m_per_lat - ay;
    let length_sq = dx * dx + dy * dy;
    let t = if length_sq == 0.0 {
        0.0
    } else {
        (-(ax * dx + ay * dy) / length_sq).clamp(0.0, 1.0)
    };
    (ax + t * dx).hypot(ay + t * dy)
}

fn distance_meters(a: LngLat, b: LngLat) -> f64 {
    distance_to_segment_meters(a[0], a[1], b, b)
}

fn slide_to_nearest_segment(path: &[LngLat], stop: &BusRouteStop, start: usize) -> (usize, f64) {
    let mut segment = start;
    let mut distance = distance_to_segment_meters(
        stop.longitude,
        stop.latitude,
        path[segment],
        path[segment + 1],
    );
    while segment < path.len() - 2 {
        let next = distance_to_segment_meters(
            stop.longitude,
            stop.latitude,
            path[segment + 1],
            path[segment + 2],
        );
        if next > distance {
            break;
        }
        segment += 1;
        distance = next;
    }
    (segment, distance)
}

/// Project stops in route order so repeated roads / loopbacks select the
/// correct pass.
fn project_stops_onto_path(path: &[LngLat], stops: &[BusRouteStop]) -> Vec<(usize, f64)> {
    let mut projections = Vec::new();
    let mut cursor = 0usize;
    for stop in stops {
        let mut nearest = (cursor, f64::INFINITY);
        let mut matched = None;
        for segment in cursor..path.len() - 1 {
            let distance = distance_to_segment_meters(
                stop.longitude,
                stop.latitude,
                path[segment],
                path[segment + 1],
            );
            if distance < nearest.1 {
                nearest = (segment, distance);
            }
            if distance <= STOP_ON_PATH_TOLERANCE_M {
                matched = Some(slide_to_nearest_segment(path, stop, segment));
                break;
            }
        }
        let (segment, distance) = matched.unwrap_or(nearest);
        if matched.is_some() {
            cursor = segment;
        }
        projections.push((segment, distance));
    }
    projections
}

fn start_loop_at_first_stop(path: Vec<LngLat>, stops: &[BusRouteStop]) -> Vec<LngLat> {
    let (Some(first), Some(last)) = (stops.first(), stops.last()) else {
        return path;
    };
    if path.len() < 3
        || distance_meters(
            [first.longitude, first.latitude],
            [last.longitude, last.latitude],
        ) > LOOP_TERMINUS_RADIUS_M
        || distance_meters(path[0], path[path.len() - 1]) > CLOSED_PATH_RADIUS_M
    {
        return path;
    }
    for segment in 0..path.len() - 1 {
        let distance = distance_to_segment_meters(
            first.longitude,
            first.latitude,
            path[segment],
            path[segment + 1],
        );
        if distance <= STOP_ON_PATH_TOLERANCE_M {
            if segment == 0 {
                return path;
            }
            let start = slide_to_nearest_segment(&path, first, segment).0;
            let mut rotated = path[start..].to_vec();
            rotated.extend_from_slice(&path[1..start + 1]);
            return rotated;
        }
    }
    path
}

#[derive(Clone, Debug, PartialEq)]
pub struct PatternFit {
    pub path: Vec<LngLat>,
    /// Per stop: index of the path segment the stop sits on.
    pub stop_segments: Vec<usize>,
    /// Mean in-order stop distance in metres. Lower is better.
    pub score: f64,
}

fn fit_pattern(pattern: Vec<LngLat>, stops: &[BusRouteStop]) -> PatternFit {
    let path = start_loop_at_first_stop(pattern, stops);
    let projections = project_stops_onto_path(&path, stops);
    let total: f64 = projections
        .iter()
        .map(|(_, d)| d.min(MAX_STOP_DISTANCE_PENALTY_M))
        .sum();
    let score = if projections.is_empty() {
        f64::INFINITY
    } else {
        total / projections.len() as f64
    };
    PatternFit {
        path,
        stop_segments: projections.into_iter().map(|(s, _)| s).collect(),
        score,
    }
}

fn best_fit(patterns: &[Vec<LngLat>], stops: &[BusRouteStop]) -> Option<PatternFit> {
    let mut best: Option<PatternFit> = None;
    for pattern in patterns {
        if pattern.len() < 2 {
            continue;
        }
        let fit = fit_pattern(pattern.clone(), stops);
        if best.as_ref().is_none_or(|b| fit.score < b.score) {
            best = Some(fit);
        }
    }
    best.filter(|f| f.score < MAX_PATTERN_MATCH_DISTANCE_M)
}

pub struct DirectionStops<'a> {
    pub direction: i64,
    pub stops: &'a [BusRouteStop],
}

/// Pick each direction's road polyline, falling back to a reversed pattern
/// (then straight stop chords) when only one direction has geometry.
pub fn match_patterns_to_directions(
    patterns: &[Vec<LngLat>],
    directions: &[DirectionStops<'_>],
) -> HashMap<i64, PatternFit> {
    let mut result = HashMap::new();
    if patterns.is_empty() {
        return result;
    }
    for dir in directions {
        let reversed: Vec<Vec<LngLat>> = patterns
            .iter()
            .map(|p| p.iter().rev().copied().collect())
            .collect();
        let fit = best_fit(patterns, dir.stops).or_else(|| best_fit(&reversed, dir.stops));
        if let Some(fit) = fit {
            result.insert(dir.direction, fit);
        }
    }
    result
}

/// Implemented by any direction-like value so `directions_leaving_stop` works
/// for both the internal draft and UI-facing direction types.
pub trait SelectedStop {
    fn selected_stop_index(&self) -> Option<i64>;
    fn stop_count(&self) -> usize;
}

/// A bus at the selected stop only travels one way: keep the direction(s)
/// that continue on from it; a direction the stop only ends is dropped.
pub fn directions_leaving_stop<D: SelectedStop>(directions: &[D]) -> Vec<&D> {
    let serving: Vec<&D> = directions
        .iter()
        .filter(|d| d.selected_stop_index().is_some())
        .collect();
    let departing: Vec<&D> = serving
        .iter()
        .copied()
        .filter(|d| d.selected_stop_index().unwrap() < d.stop_count() as i64 - 1)
        .collect();
    if !departing.is_empty() {
        departing
    } else if !serving.is_empty() {
        serving
    } else {
        directions.iter().collect()
    }
}

/// ServiceNo → decoded pattern polylines (one per direction pattern).
pub type BusRouterGeometryIndex = HashMap<String, Vec<Vec<LngLat>>>;

pub fn decode_geometry_index(body: &serde_json::Value) -> BusRouterGeometryIndex {
    let mut index = BusRouterGeometryIndex::new();
    let Some(object) = body.as_object() else {
        return index;
    };
    for (service_no, patterns) in object {
        let Some(patterns) = patterns.as_array() else {
            continue;
        };
        let mut decoded = Vec::new();
        for pattern in patterns {
            let Some(encoded) = pattern.as_str().filter(|s| s.len() >= 4) else {
                continue;
            };
            let path = downsample_path(decode_google_polyline(encoded), MAX_BUSROUTER_PATH_POINTS);
            if path.len() >= 2 {
                decoded.push(path);
            }
        }
        if !decoded.is_empty() {
            index.insert(service_no.clone(), decoded);
        }
    }
    index
}

fn find_case_insensitive_key<V>(index: &HashMap<String, V>, service_no: &str) -> Option<String> {
    let lower = service_no.to_ascii_lowercase();
    index
        .keys()
        .find(|k| k.to_ascii_lowercase() == lower)
        .cloned()
}

#[derive(Clone, Debug)]
struct Draft {
    direction: i64,
    selected_stop_index: Option<i64>,
    origin_code: String,
    destination_code: String,
    stops: Vec<BusRouteStop>,
}

impl SelectedStop for Draft {
    fn selected_stop_index(&self) -> Option<i64> {
        self.selected_stop_index
    }
    fn stop_count(&self) -> usize {
        self.stops.len()
    }
}

impl Client {
    fn get_bus_routes_index(&self) -> Result<BusRoutesIndex> {
        self.cache
            .get("bus-routes-index", BUS_ROUTES_CACHE_TTL_MS, || {
                let rows = self.collect_lta_endpoint::<LtaBusRouteRow>(
                    "/BusRoutes",
                    BUS_ROUTES_PAGE_SIZE,
                    BUS_ROUTES_MAX_PAGES,
                    BUS_ROUTES_TIMEOUT_MS,
                )?;
                Ok(build_bus_routes_index(&rows))
            })
    }

    fn fetch_busrouter_geometry(&self) -> Result<BusRouterGeometryIndex> {
        for url in [BUSROUTER_ROUTES_URL, BUSROUTER_ROUTES_FALLBACK_URL] {
            let decoded = self
                .http
                .get_json::<serde_json::Value>(
                    "busrouter",
                    url,
                    &[("Accept", "application/json")],
                    BUSROUTER_GEOMETRY_TIMEOUT_MS,
                )
                .map(|body| decode_geometry_index(&body));
            if let Ok(index) = decoded
                && !index.is_empty()
            {
                return Ok(index);
            }
        }
        Err(Error::external(
            "busrouter",
            502,
            "No BusRouter geometry source is currently reachable",
        ))
    }

    /// Fall back to stop chords and briefly cache geometry-source failures.
    fn get_busrouter_geometry_index(&self) -> BusRouterGeometryIndex {
        if let Some(empty) = self.cache.peek::<BusRouterGeometryIndex>(
            "busrouter-geometry-unavailable",
            BUSROUTER_GEOMETRY_NEGATIVE_TTL_MS,
        ) {
            return empty;
        }
        match self.cache.get(
            "busrouter-geometry-index",
            BUSROUTER_GEOMETRY_CACHE_TTL_MS,
            || self.fetch_busrouter_geometry(),
        ) {
            Ok(index) => index,
            Err(_) => {
                let empty = BusRouterGeometryIndex::new();
                self.cache.set_with_ttl(
                    "busrouter-geometry-unavailable",
                    &empty,
                    Some(BUSROUTER_GEOMETRY_NEGATIVE_TTL_MS),
                );
                empty
            }
        }
    }

    pub fn get_bus_route(
        &self,
        service_no: &str,
        stop_id: Option<&str>,
    ) -> Result<BusRouteResponse> {
        let result = self.build_bus_route(service_no, stop_id);
        self.finish(Source::BusRoutes, result)
    }

    fn build_bus_route(&self, service_no: &str, stop_id: Option<&str>) -> Result<BusRouteResponse> {
        crate::validate_service_no(service_no)?;
        if let Some(id) = stop_id {
            crate::validate_bus_stop_id(id)?;
        }
        let normalized = service_no.trim();
        let index = self.get_bus_routes_index()?;
        let mut service_key = normalized.to_string();
        let mut directions_map = index.get(normalized).cloned();
        if directions_map.is_none()
            && let Some(matched) = find_case_insensitive_key(&index, normalized)
        {
            service_key = matched.clone();
            directions_map = index.get(&matched).cloned();
        }
        let directions_map = directions_map.filter(|m| !m.is_empty()).ok_or_else(|| {
            Error::external(
                "lta",
                404,
                format!("Bus service {normalized} was not found"),
            )
        })?;

        let stops_catalog = self.get_bus_stops()?;
        let by_code: HashMap<&str, &argus_core::types::BusStop> = stops_catalog
            .iter()
            .map(|s| (s.bus_stop_code.as_str(), s))
            .collect();
        let context_stop = stop_id.map(str::trim).filter(|s| !s.is_empty());

        let mut drafts: Vec<Draft> = Vec::new();
        let mut sorted_dirs: Vec<(&i64, &Vec<IndexedBusRouteStop>)> =
            directions_map.iter().collect();
        sorted_dirs.sort_by_key(|(d, _)| **d);
        for (direction, indexed) in sorted_dirs {
            let mut joined = Vec::new();
            for entry in indexed {
                let Some(stop) = by_code.get(entry.bus_stop_code.as_str()) else {
                    continue;
                };
                if !stop.latitude.is_finite() || !stop.longitude.is_finite() {
                    continue;
                }
                joined.push(BusRouteStop {
                    bus_stop_code: entry.bus_stop_code.clone(),
                    description: stop.description.clone(),
                    latitude: stop.latitude,
                    longitude: stop.longitude,
                    stop_sequence: entry.stop_sequence as i64,
                });
            }
            if joined.len() < 2 {
                continue;
            }
            let selected = context_stop
                .and_then(|id| joined.iter().position(|s| s.bus_stop_code == id))
                .map(|i| i as i64);
            let origin = joined
                .first()
                .map(|s| s.bus_stop_code.clone())
                .or_else(|| indexed.first().map(|s| s.bus_stop_code.clone()))
                .unwrap_or_default();
            let destination = joined
                .last()
                .map(|s| s.bus_stop_code.clone())
                .or_else(|| indexed.last().map(|s| s.bus_stop_code.clone()))
                .unwrap_or_default();
            drafts.push(Draft {
                direction: *direction,
                selected_stop_index: selected,
                origin_code: origin,
                destination_code: destination,
                stops: joined,
            });
        }
        if drafts.is_empty() {
            return Err(Error::external(
                "lta",
                404,
                format!("Bus service {service_key} has no mappable stops"),
            ));
        }

        let shown = directions_leaving_stop(&drafts);
        let matching: Vec<i64> = shown
            .iter()
            .filter(|d| d.selected_stop_index.is_some())
            .map(|d| d.direction)
            .collect();
        let preferred = if matching.len() == 1 { matching[0] } else { -1 };

        let geometry = self.get_busrouter_geometry_index();
        let patterns = geometry
            .get(&service_key)
            .or_else(|| geometry.get(normalized))
            .or_else(|| {
                find_case_insensitive_key(&geometry, &service_key).and_then(|k| geometry.get(&k))
            })
            .cloned()
            .unwrap_or_default();
        let ref_dirs: Vec<DirectionStops<'_>> = shown
            .iter()
            .map(|d| DirectionStops {
                direction: d.direction,
                stops: &d.stops,
            })
            .collect();
        let fits = match_patterns_to_directions(&patterns, &ref_dirs);

        let directions = shown
            .into_iter()
            .map(|d| {
                let fit = fits.get(&d.direction);
                let selected_path_index = fit.and_then(|f| {
                    d.selected_stop_index
                        .and_then(|idx| f.stop_segments.get(idx as usize).copied())
                });
                BusRouteDirection {
                    direction: d.direction,
                    preferred: preferred == d.direction,
                    selected_stop_index: d.selected_stop_index,
                    origin_code: d.origin_code.clone(),
                    destination_code: d.destination_code.clone(),
                    stops: d.stops.clone(),
                    path: fit.map(|f| f.path.clone()).unwrap_or_default(),
                    selected_path_index: selected_path_index.map(|i| i as i64),
                }
            })
            .collect();
        Ok(BusRouteResponse {
            service_no: service_key,
            directions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stops_at(points: &[LngLat]) -> Vec<BusRouteStop> {
        points
            .iter()
            .enumerate()
            .map(|(i, p)| BusRouteStop {
                bus_stop_code: format!("{}", 10_000 + i),
                description: format!("Stop {i}"),
                latitude: p[1],
                longitude: p[0],
                stop_sequence: (i + 1) as i64,
            })
            .collect()
    }

    fn road(from: f64, to: f64, lat: f64) -> Vec<LngLat> {
        let steps = ((to - from).abs() / 0.001).round() as usize;
        (0..=steps)
            .map(|i| [from + (to - from) * i as f64 / steps as f64, lat])
            .collect()
    }

    #[test]
    fn polyline_decode_and_downsample() {
        // "_p~iF~ps|U_ulLnnqC" decodes to the classic (lng,lat) pair sequence.
        let path = decode_google_polyline("_p~iF~ps|U_ulLnnqC");
        assert_eq!(path.len(), 2);
        assert!((path[0][0] - (-120.2)).abs() < 1e-4);
        let short = downsample_path((0..100).map(|i| [i as f64, 0.0]).collect(), 10);
        assert_eq!(short.len(), 10);
    }

    #[test]
    fn directions_leaving_stop_filters_and_keeps_all() {
        struct Dir(i64, Option<i64>, usize);
        impl SelectedStop for Dir {
            fn selected_stop_index(&self) -> Option<i64> {
                self.1
            }
            fn stop_count(&self) -> usize {
                self.2
            }
        }
        let dirs = [Dir(1, Some(1), 3), Dir(2, None, 3), Dir(3, Some(2), 3)];
        let shown = directions_leaving_stop(&dirs);
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].0, 1);
        assert_eq!(directions_leaving_stop(&[Dir(1, None, 3)]).len(), 1);
    }

    #[test]
    fn pattern_matching_uses_stop_order_and_reverses() {
        let east = road(103.8, 103.83, 1.3);
        let west = road(103.83, 103.8, 1.3002);
        let east_stops = stops_at(&[
            [103.801, 1.3001],
            [103.81, 1.3001],
            [103.82, 1.3001],
            [103.829, 1.3001],
        ]);
        let west_stops = stops_at(
            &east_stops
                .iter()
                .rev()
                .map(|s| [s.longitude, s.latitude])
                .collect::<Vec<_>>(),
        );
        let patterns = vec![west.clone(), east.clone()];
        let dirs = [
            DirectionStops {
                direction: 1,
                stops: &east_stops,
            },
            DirectionStops {
                direction: 2,
                stops: &west_stops,
            },
        ];
        let matched = match_patterns_to_directions(&patterns, &dirs);
        assert_eq!(matched.get(&1).unwrap().path, east);
        assert_eq!(matched.get(&2).unwrap().path, west);

        // Only one pattern: the other direction gets it reversed.
        let one = match_patterns_to_directions(
            std::slice::from_ref(&east),
            &[DirectionStops {
                direction: 2,
                stops: &west_stops,
            }],
        );
        let reversed: Vec<LngLat> = east.iter().rev().copied().collect();
        assert_eq!(one.get(&2).unwrap().path, reversed);
    }

    #[test]
    fn route_index_sorts_stops_by_sequence() {
        use argus_core::api_types::NumberOrString;
        let rows = vec![
            LtaBusRouteRow {
                service_no: "12e".into(),
                direction: NumberOrString::Number(1.0),
                stop_sequence: NumberOrString::Number(2.0),
                bus_stop_code: "B".into(),
            },
            LtaBusRouteRow {
                service_no: "12e".into(),
                direction: NumberOrString::Number(1.0),
                stop_sequence: NumberOrString::Number(1.0),
                bus_stop_code: "A".into(),
            },
        ];
        let index = build_bus_routes_index(&rows);
        let stops = &index["12e"][&1];
        assert_eq!(stops[0].bus_stop_code, "A");
        assert_eq!(stops[1].bus_stop_code, "B");
    }
}
