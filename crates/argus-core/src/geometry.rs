//! Framework-free GeoJSON builders for map overlays.
//! Ported from `lib/map-geometry.ts` and `lib/map-features.ts`.

use serde::{Deserialize, Serialize};

use crate::mrt_network::display_stations;
use crate::mrt_routing::{MrtRouteSegment, is_routeable_mrt_station};
use crate::types::{BusRouteStop, FlightState, LngLat, MrtGeoJson, TrafficCamera};

// ---------------------------------------------------------------------------
// Bus route overlays
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct BusRouteOverlay {
    pub service_no: String,
    pub direction: i64,
    pub stops: Vec<BusRouteStop>,
    pub selected_stop_index: Option<i64>,
    /// Road-following polyline when available; empty falls back to stop chords.
    pub path: Vec<LngLat>,
    pub selected_path_index: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BusRouteLineGeoJson {
    pub r#type: String,
    pub features: Vec<BusRouteLineFeature>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BusRouteLineFeature {
    pub r#type: String,
    pub properties: BusRouteLineProps,
    pub geometry: LineStringGeo,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BusRouteLineProps {
    pub kind: String,
    #[serde(rename = "serviceNo")]
    pub service_no: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LineStringGeo {
    pub r#type: String,
    pub coordinates: Vec<LngLat>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BusRouteStopsGeoJson {
    pub r#type: String,
    pub features: Vec<BusRouteStopFeature>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BusRouteStopFeature {
    pub r#type: String,
    pub properties: BusRouteStopProps,
    pub geometry: PointGeo,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BusRouteStopProps {
    #[serde(rename = "BusStopCode")]
    pub bus_stop_code: String,
    #[serde(rename = "Description")]
    pub description: String,
    pub role: String,
    pub sequence: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PointGeo {
    pub r#type: String,
    pub coordinates: LngLat,
}

fn empty_line_geojson() -> BusRouteLineGeoJson {
    BusRouteLineGeoJson {
        r#type: "FeatureCollection".into(),
        features: Vec::new(),
    }
}
fn empty_stops_geojson() -> BusRouteStopsGeoJson {
    BusRouteStopsGeoJson {
        r#type: "FeatureCollection".into(),
        features: Vec::new(),
    }
}

/// Resolve the full route, or only the selected stop's remaining segment.
pub fn resolve_bus_route_draw_path(overlay: &BusRouteOverlay) -> Vec<LngLat> {
    let full_coords: Vec<LngLat> = if overlay.path.len() >= 2 {
        overlay.path.clone()
    } else {
        overlay
            .stops
            .iter()
            .map(|stop| [stop.longitude, stop.latitude])
            .collect()
    };
    if full_coords.len() < 2 {
        return Vec::new();
    }

    let start_idx = if overlay.path.len() >= 2 {
        overlay.selected_path_index
    } else {
        overlay.selected_stop_index
    };
    let at_terminus = overlay.selected_stop_index == Some(overlay.stops.len() as i64 - 1);
    if !at_terminus
        && let Some(idx) = start_idx
        && idx >= 0
        && (idx as usize) < full_coords.len() - 1
    {
        let stop = overlay
            .selected_stop_index
            .and_then(|i| overlay.stops.get(i as usize));
        if overlay.path.len() < 2 || stop.is_none() {
            return full_coords[idx as usize..].to_vec();
        }
        let stop = stop.unwrap();
        let start = closest_point_on_segment(
            [stop.longitude, stop.latitude],
            full_coords[idx as usize],
            full_coords[idx as usize + 1],
        );
        let mut result = vec![start];
        result.extend_from_slice(&full_coords[(idx as usize) + 1..]);
        return result;
    }
    full_coords
}

/// Closest point to `point` on segment a–b (planar; fine at street scale).
fn closest_point_on_segment(point: LngLat, a: LngLat, b: LngLat) -> LngLat {
    let k = (point[1] * std::f64::consts::PI / 180.0).cos();
    let dx = (b[0] - a[0]) * k;
    let dy = b[1] - a[1];
    let length_sq = dx * dx + dy * dy;
    let t = if length_sq == 0.0 {
        0.0
    } else {
        (((point[0] - a[0]) * k * dx + (point[1] - a[1]) * dy) / length_sq).clamp(0.0, 1.0)
    };
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

/// Stops on the remaining path (selected stop → terminus), inclusive.
pub fn resolve_remaining_route_stops(overlay: &BusRouteOverlay) -> Vec<BusRouteStop> {
    if overlay.stops.is_empty() {
        return Vec::new();
    }
    let start = match overlay.selected_stop_index {
        Some(idx) if idx >= 0 && (idx as usize) < overlay.stops.len() - 1 => idx as usize,
        _ => 0,
    };
    overlay.stops[start..].to_vec()
}

pub fn build_bus_route_geojson(overlay: Option<&BusRouteOverlay>) -> BusRouteLineGeoJson {
    let overlay = match overlay {
        Some(o) if o.path.len() >= 2 || o.stops.len() >= 2 => o,
        _ => return empty_line_geojson(),
    };
    let draw = resolve_bus_route_draw_path(overlay);
    if draw.len() < 2 {
        return empty_line_geojson();
    }
    BusRouteLineGeoJson {
        r#type: "FeatureCollection".into(),
        features: vec![BusRouteLineFeature {
            r#type: "Feature".into(),
            properties: BusRouteLineProps {
                kind: "remaining".into(),
                service_no: overlay.service_no.clone(),
            },
            geometry: LineStringGeo {
                r#type: "LineString".into(),
                coordinates: draw,
            },
        }],
    }
}

pub fn build_bus_route_stops_geojson(overlay: Option<&BusRouteOverlay>) -> BusRouteStopsGeoJson {
    let overlay = match overlay {
        Some(o) => o,
        None => return empty_stops_geojson(),
    };
    let remaining = resolve_remaining_route_stops(overlay);
    if remaining.is_empty() {
        return empty_stops_geojson();
    }
    let selected = overlay
        .selected_stop_index
        .filter(|i| *i >= 0 && (*i as usize) < overlay.stops.len())
        .map(|i| i as usize);
    let at_terminus = selected == Some(overlay.stops.len() - 1);
    let start_index = match selected {
        Some(index) if !at_terminus => index,
        _ => 0,
    };
    let last_idx = remaining.len() - 1;
    let mut features = Vec::new();
    for (index, stop) in remaining.iter().enumerate() {
        if !stop.latitude.is_finite() || !stop.longitude.is_finite() {
            continue;
        }
        let is_selected = selected == Some(start_index + index);
        let role = if is_selected {
            "selected"
        } else if index == last_idx {
            "terminus"
        } else {
            "stop"
        };
        features.push(BusRouteStopFeature {
            r#type: "Feature".into(),
            properties: BusRouteStopProps {
                bus_stop_code: stop.bus_stop_code.clone(),
                description: stop.description.clone(),
                role: role.into(),
                sequence: stop.stop_sequence,
            },
            geometry: PointGeo {
                r#type: "Point".into(),
                coordinates: [stop.longitude, stop.latitude],
            },
        });
    }
    BusRouteStopsGeoJson {
        r#type: "FeatureCollection".into(),
        features,
    }
}

// ---------------------------------------------------------------------------
// MRT station interpolation + route geometry
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MrtStationFeature {
    pub r#type: String,
    pub properties: MrtStationProps,
    pub geometry: PointGeo,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MrtStationProps {
    pub name: String,
    pub label: String,
    pub line: String,
    pub color: String,
    pub routeable: bool,
}

/// Use line vertices as station positions when counts match; otherwise
/// distribute stations evenly along the line. Positions are approximate.
pub fn interpolate_stations(
    coordinates: &[LngLat],
    station_names: &[String],
    line: &str,
    color: &str,
) -> Vec<MrtStationFeature> {
    if coordinates.len() < 2 || station_names.is_empty() {
        return Vec::new();
    }
    let make = |name: &str, coord: LngLat| MrtStationFeature {
        r#type: "Feature".into(),
        properties: MrtStationProps {
            name: name.into(),
            label: name.into(),
            line: line.into(),
            color: color.into(),
            routeable: is_routeable_mrt_station(name),
        },
        geometry: PointGeo {
            r#type: "Point".into(),
            coordinates: coord,
        },
    };
    if coordinates.len() == station_names.len() {
        return station_names
            .iter()
            .zip(coordinates.iter())
            .map(|(name, coord)| make(name, *coord))
            .collect();
    }
    if station_names.len() == 1 {
        return vec![make(&station_names[0], coordinates[0])];
    }

    let segment_lengths: Vec<f64> = coordinates
        .windows(2)
        .map(|p| (p[1][0] - p[0][0]).hypot(p[1][1] - p[0][1]))
        .collect();
    let total: f64 = segment_lengths.iter().sum();
    if total == 0.0 {
        return Vec::new();
    }
    station_names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let target = total * index as f64 / (station_names.len() - 1) as f64;
            let mut traversed = 0.0;
            let mut segment_index = 0;
            while segment_index < segment_lengths.len() - 1
                && traversed + segment_lengths[segment_index] < target
            {
                traversed += segment_lengths[segment_index];
                segment_index += 1;
            }
            let current_len = segment_lengths[segment_index];
            let ratio = if current_len > 0.0 {
                (target - traversed) / current_len
            } else {
                0.0
            };
            let start = coordinates[segment_index];
            let end = coordinates[segment_index + 1];
            let lng = start[0] + (end[0] - start[0]) * ratio;
            let lat = start[1] + (end[1] - start[1]) * ratio;
            make(name, [lng, lat])
        })
        .collect()
}

/// `line|station` keys for the stations a route rides.
pub fn route_station_keys(route_segments: &[MrtRouteSegment]) -> Vec<String> {
    route_segments
        .iter()
        .flat_map(|segment| {
            segment
                .stations
                .iter()
                .map(|station| format!("{}|{}", segment.line, station))
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MrtRouteGeoJson {
    pub r#type: String,
    pub features: Vec<MrtRouteFeature>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MrtRouteFeature {
    pub r#type: String,
    pub properties: MrtRouteProps,
    pub geometry: LineStringGeo,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MrtRouteProps {
    pub line: String,
    pub color: String,
}

/// Build GeoJSON for the ridden segments of an MRT route.
///
/// `station_lookup` maps a line name to its ordered station names (the TS uses
/// `MRT_DISPLAY_LINE_STATIONS`). Pass `display_line_stations()` for the full
/// display network, or a `HashMap::new()` for the empty case.
pub fn build_route_geojson(
    mrt_lines: &MrtGeoJson,
    route_segments: &[MrtRouteSegment],
    station_lookup: &std::collections::HashMap<String, Vec<String>>,
) -> MrtRouteGeoJson {
    let mut features = Vec::new();
    let by_name: std::collections::HashMap<&str, &crate::types::MrtLineFeature> = mrt_lines
        .features
        .iter()
        .map(|f| (f.properties.name.as_str(), f))
        .collect();

    for segment in route_segments {
        if segment.stops <= 0 {
            continue;
        }
        let Some(line_feature) = by_name.get(segment.line.as_str()) else {
            continue;
        };
        let station_names = match station_lookup.get(&segment.line) {
            Some(names) if !names.is_empty() => names,
            _ => continue,
        };
        let station_features = interpolate_stations(
            &line_feature.geometry.coordinates,
            station_names,
            &segment.line,
            &line_feature.properties.color,
        );
        if station_features.len() != station_names.len() {
            continue;
        }
        let positions: std::collections::HashMap<&str, LngLat> = station_features
            .iter()
            .map(|f| (f.properties.name.as_str(), f.geometry.coordinates))
            .collect();
        let ridden: Vec<&String> = if segment.stations.len() >= 2 {
            segment.stations.iter().collect()
        } else {
            vec![&segment.from, &segment.to]
        };
        let coordinates: Vec<LngLat> = ridden
            .iter()
            .filter_map(|station| positions.get(station.as_str()).copied())
            .collect();
        if coordinates.len() != ridden.len() || coordinates.len() < 2 {
            continue;
        }
        features.push(MrtRouteFeature {
            r#type: "Feature".into(),
            properties: MrtRouteProps {
                line: segment.line.clone(),
                color: line_feature.properties.color.clone(),
            },
            geometry: LineStringGeo {
                r#type: "LineString".into(),
                coordinates,
            },
        });
    }
    MrtRouteGeoJson {
        r#type: "FeatureCollection".into(),
        features,
    }
}

// ---------------------------------------------------------------------------
// Map feature builders (lib/map-features.ts)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PointFeatureCollection<P> {
    pub r#type: String,
    pub features: Vec<PointFeature<P>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PointFeature<P> {
    pub r#type: String,
    pub properties: P,
    pub geometry: PointGeo,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BusStopProps {
    #[serde(rename = "BusStopCode")]
    pub bus_stop_code: String,
    #[serde(rename = "Description")]
    pub description: String,
}

pub fn build_bus_stop_geojson<'a, I>(stops: I) -> PointFeatureCollection<BusStopProps>
where
    I: IntoIterator<Item = &'a crate::types::BusStop>,
{
    let features = stops
        .into_iter()
        .filter(|stop| stop.latitude.is_finite() && stop.longitude.is_finite())
        .map(|stop| PointFeature {
            r#type: "Feature".into(),
            properties: BusStopProps {
                bus_stop_code: stop.bus_stop_code.clone(),
                description: stop.description.clone(),
            },
            geometry: PointGeo {
                r#type: "Point".into(),
                coordinates: [stop.longitude, stop.latitude],
            },
        })
        .collect();
    PointFeatureCollection {
        r#type: "FeatureCollection".into(),
        features,
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FlightFeatureProps {
    pub id: String,
    pub callsign: String,
    pub direction: crate::types::FlightDirection,
    pub track: f64,
}

pub fn build_flight_geojson<'a, I>(flights: I) -> PointFeatureCollection<FlightFeatureProps>
where
    I: IntoIterator<Item = &'a FlightState>,
{
    let features = flights
        .into_iter()
        .filter(|f| f.latitude.is_finite() && f.longitude.is_finite())
        .map(|f| PointFeature {
            r#type: "Feature".into(),
            properties: FlightFeatureProps {
                id: f.id.clone(),
                callsign: f.callsign.clone(),
                direction: f.direction,
                track: f.track.unwrap_or(0.0),
            },
            geometry: PointGeo {
                r#type: "Point".into(),
                coordinates: [f.longitude, f.latitude],
            },
        })
        .collect();
    PointFeatureCollection {
        r#type: "FeatureCollection".into(),
        features,
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CameraFeatureProps {
    #[serde(rename = "CameraID")]
    pub camera_id: String,
    pub location: String,
}

pub fn build_camera_geojson<'a, I>(cameras: I) -> PointFeatureCollection<CameraFeatureProps>
where
    I: IntoIterator<Item = &'a TrafficCamera>,
{
    let features = cameras
        .into_iter()
        .filter(|c| c.latitude.is_finite() && c.longitude.is_finite())
        .map(|c| PointFeature {
            r#type: "Feature".into(),
            properties: CameraFeatureProps {
                camera_id: c.camera_id.clone(),
                location: c.location.clone(),
            },
            geometry: PointGeo {
                r#type: "Point".into(),
                coordinates: [c.longitude, c.latitude],
            },
        })
        .collect();
    PointFeatureCollection {
        r#type: "FeatureCollection".into(),
        features,
    }
}

/// Build MRT station features from the MRT line GeoJSON and display station
/// lists. Labels only the first occurrence of each station (case-insensitive).
pub fn build_mrt_station_geojson(mrt_lines: &MrtGeoJson) -> Vec<MrtStationFeature> {
    let mut labelled: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut features = Vec::new();
    for line_feature in &mrt_lines.features {
        let line = &line_feature.properties.name;
        let names = display_stations(line).unwrap_or(&[]).to_vec();
        for mut feature in interpolate_stations(
            &line_feature.geometry.coordinates,
            &names,
            line,
            &line_feature.properties.color,
        ) {
            let key = feature.properties.name.to_lowercase();
            if labelled.contains(&key) {
                feature.properties.label.clear();
            } else {
                labelled.insert(key);
            }
            features.push(feature);
        }
    }
    features
}

/// Build a `HashMap` view of the display line station lists, for
/// `build_route_geojson`.
pub fn display_line_stations_map() -> std::collections::HashMap<String, Vec<String>> {
    crate::mrt_network::MRT_DISPLAY_LINE_STATIONS
        .iter()
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stop(n: i64, longitude: f64, latitude: f64) -> BusRouteStop {
        BusRouteStop {
            bus_stop_code: n.to_string(),
            description: n.to_string(),
            longitude,
            latitude,
            stop_sequence: n,
        }
    }

    #[test]
    fn resolve_bus_route_draw_path_remaining_and_terminus() {
        let stops = vec![
            stop(1, 103.0, 1.0),
            stop(2, 103.1, 1.1),
            stop(3, 103.2, 1.2),
        ];
        let overlay = BusRouteOverlay {
            service_no: "12".into(),
            direction: 1,
            stops: stops.clone(),
            selected_stop_index: Some(1),
            path: vec![[103.0, 1.0], [103.1, 1.1], [103.2, 1.2]],
            selected_path_index: Some(1),
        };
        assert_eq!(
            resolve_bus_route_draw_path(&overlay),
            vec![[103.1, 1.1], [103.2, 1.2]]
        );
        // Terminus draws the whole route.
        let overlay2 = BusRouteOverlay {
            selected_stop_index: Some(2),
            selected_path_index: Some(1),
            ..overlay.clone()
        };
        assert_eq!(resolve_bus_route_draw_path(&overlay2).len(), 3);
        // Start at the stop's point on a long road segment, not the segment start.
        let stops3 = vec![
            stop(1, 103.8, 1.3),
            stop(2, 103.809, 1.3002),
            stop(3, 103.82, 1.3),
        ];
        let overlay3 = BusRouteOverlay {
            service_no: "67".into(),
            direction: 2,
            stops: stops3,
            selected_stop_index: Some(1),
            path: vec![[103.8, 1.3], [103.81, 1.3], [103.82, 1.3]],
            selected_path_index: Some(0),
        };
        let path = resolve_bus_route_draw_path(&overlay3);
        assert!((path[0][0] - 103.809).abs() < 1e-6);
        assert_eq!(&path[1..], &[[103.81, 1.3], [103.82, 1.3]]);
    }

    #[test]
    fn stops_roles_and_null_overlay() {
        let stops: Vec<_> = (1..=4)
            .map(|n| stop(n, 103.0 + n as f64 / 10.0, 1.0 + n as f64 / 10.0))
            .collect();
        let overlay = |sel: Option<i64>| BusRouteOverlay {
            service_no: "12".into(),
            direction: 1,
            stops: stops.clone(),
            selected_stop_index: sel,
            path: stops.iter().map(|s| [s.longitude, s.latitude]).collect(),
            selected_path_index: sel,
        };
        let roles = |sel| -> Vec<String> {
            build_bus_route_stops_geojson(Some(&overlay(sel)))
                .features
                .into_iter()
                .map(|f| f.properties.role)
                .collect()
        };
        assert_eq!(roles(Some(1)), ["selected", "stop", "terminus"]);
        assert_eq!(roles(Some(0)), ["selected", "stop", "stop", "terminus"]);
        assert_eq!(roles(Some(3)), ["stop", "stop", "stop", "selected"]);
        assert_eq!(roles(None), ["stop", "stop", "stop", "terminus"]);
        assert!(build_bus_route_geojson(None).features.is_empty());
        assert!(build_bus_route_stops_geojson(None).features.is_empty());
    }

    #[test]
    fn interpolate_stations_vertices_and_even() {
        let names: Vec<String> = ["A", "B"].iter().map(|s| s.to_string()).collect();
        let features =
            interpolate_stations(&[[103.0, 1.0], [103.1, 1.1]], &names, "Test Line", "#fff");
        assert_eq!(features.len(), 2);
        assert_eq!(features[0].geometry.coordinates, [103.0, 1.0]);
        let names: Vec<String> = ["A", "B", "C"].iter().map(|s| s.to_string()).collect();
        let features =
            interpolate_stations(&[[0.0, 0.0], [10.0, 0.0]], &names, "Test Line", "#fff");
        assert_eq!(features.len(), 3);
        assert!((features[1].geometry.coordinates[0] - 5.0).abs() < 1e-5);
    }

    #[test]
    fn route_station_keys_and_empty_route() {
        assert!(route_station_keys(&[]).is_empty());
        let empty = MrtGeoJson {
            r#type: "FeatureCollection".into(),
            features: Vec::new(),
        };
        let route = build_route_geojson(&empty, &[], &std::collections::HashMap::new());
        assert!(route.features.is_empty());
    }

    #[test]
    fn map_feature_builders_reject_invalid_coords() {
        use crate::types::BusStop;
        let stop = BusStop {
            bus_stop_code: "01012".into(),
            road_name: "Victoria St".into(),
            description: "Bugis Stn".into(),
            latitude: 1.3,
            longitude: 103.85,
        };
        let bad = BusStop {
            bus_stop_code: "bad".into(),
            latitude: f64::NAN,
            ..stop.clone()
        };
        let result = build_bus_stop_geojson([&stop, &bad]);
        assert_eq!(result.features.len(), 1);
        assert_eq!(result.features[0].properties.bus_stop_code, "01012");
    }
}
