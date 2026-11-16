//! Map the app's domain data onto `argus-map` overlays.
//!
//! Everything in here is pure (no I/O): the view passes `state.overlays` to
//! `map_view_for`, and the id→target table resolves clicks in `msg.rs`.

use std::collections::HashMap;

use argus_core::amenity_layers::visible_places;
use argus_core::geo::within_radius;
use argus_core::types::{
    AmenityType, LngLat, ParkFeatureKind, ParkGeometry, TrafficCamera, TrainAlertStatus,
};
use argus_map::{
    Icon, LngLat as MapLngLat, MapPoint, OverlayId, Overlays, PointStyle, Polygon, Polyline,
    RadiusCircle, Rgba,
};

use crate::state::{AppState, Selection, Target};
use crate::theme::hex_color;

fn ll(p: LngLat) -> MapLngLat {
    MapLngLat::new(p[0], p[1])
}

fn rgba(hex: &str) -> Rgba {
    let c = hex_color(hex);
    Rgba::new(c.r, c.g, c.b, 1.0)
}

const CAMERA_COLOR: Rgba = Rgba::new(0.68, 0.68, 0.68, 1.0);
const BUS_COLOR: Rgba = Rgba::new(0.50, 0.94, 0.68, 1.0);
const INCIDENT_COLOR: Rgba = Rgba::new(0.91, 0.40, 0.28, 1.0);
const CARPARK_COLOR: Rgba = Rgba::new(0.70, 0.57, 0.95, 1.0);
const EV_COLOR: Rgba = Rgba::new(0.42, 0.85, 0.92, 1.0);
const PARK_COLOR: Rgba = Rgba::new(0.33, 0.84, 0.55, 1.0);
const FLIGHT_COLORS: [Rgba; 3] = [
    Rgba::new(0.33, 0.84, 0.55, 1.0),
    Rgba::new(0.91, 0.40, 0.28, 1.0),
    Rgba::new(0.46, 0.74, 0.93, 1.0),
];

fn visible_on(s: &AppState, key: &str) -> bool {
    s.visibility.get(key).copied().unwrap_or(false)
}

/// Parse the geometry variants present in `data/parks-geometry.json`.
fn park_paths(geometry: &ParkGeometry) -> Vec<Vec<LngLat>> {
    let coords = &geometry.coordinates;
    match geometry.r#type.as_str() {
        "Polygon" => serde_json::from_value::<Vec<Vec<LngLat>>>(coords.clone())
            .ok()
            .and_then(|rings| rings.into_iter().next())
            .into_iter()
            .collect(),
        "MultiPolygon" => serde_json::from_value::<Vec<Vec<Vec<LngLat>>>>(coords.clone())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|poly| poly.into_iter().next())
            .collect(),
        "LineString" => serde_json::from_value::<Vec<LngLat>>(coords.clone())
            .ok()
            .into_iter()
            .collect(),
        "MultiLineString" => {
            serde_json::from_value::<Vec<Vec<LngLat>>>(coords.clone()).unwrap_or_default()
        }
        _ => Vec::new(),
    }
}

// Small internal helper: the arguments are independent map-marker attributes.
#[allow(clippy::too_many_arguments)]
fn add_point(
    ov: &mut Overlays,
    next_id: &mut OverlayId,
    targets: &mut HashMap<OverlayId, Target>,
    position: LngLat,
    color: Rgba,
    icon: Icon,
    radius: f32,
    target: Target,
) {
    let id = *next_id;
    *next_id += 1;
    ov.points.push(MapPoint::new(
        id,
        ll(position),
        PointStyle {
            radius,
            color,
            stroke: Some(Rgba::new(0.03, 0.04, 0.05, 0.9)),
            icon,
            label: None,
        },
    ));
    targets.insert(id, target);
}

pub fn build(s: &AppState) -> (Overlays, HashMap<OverlayId, Target>) {
    let mut ov = Overlays::new();
    let mut targets = HashMap::new();
    let mut next_id: OverlayId = 1;

    let filter = s.geo_filter();
    let snap = s.data.snapshot();

    // Parks: filled polygons for parks, thin lines for connectors/trails.
    if visible_on(s, "parks")
        && let Some(data) = s.static_data.as_ref()
    {
        for feature in &data.parks_geojson.features {
            for path in park_paths(&feature.geometry) {
                if path.len() < 2 {
                    continue;
                }
                let target = Target::Park {
                    name: feature.properties.name.clone(),
                    kind: feature.properties.kind,
                };
                let is_area = feature.properties.kind == ParkFeatureKind::Park;
                let id = next_id;
                next_id += 1;
                let pts: Vec<MapLngLat> = path.iter().map(|p| ll(*p)).collect();
                if is_area {
                    ov.polygons.push(Polygon {
                        id,
                        points: pts,
                        fill: Rgba::new(PARK_COLOR.r, PARK_COLOR.g, PARK_COLOR.b, 0.18),
                        stroke: Some((PARK_COLOR, 1.0)),
                    });
                } else {
                    ov.lines.push(Polyline {
                        id,
                        points: pts,
                        color: PARK_COLOR,
                        width: 1.6,
                    });
                }
                targets.insert(id, target);
            }
        }
    }

    // MRT / LRT lines and stations. Disrupted lines are drawn in warning amber.
    let mut station_positions: HashMap<String, LngLat> = HashMap::new();
    if visible_on(s, "mrt")
        && let Some(data) = s.static_data.as_ref()
    {
        let disrupted: Vec<String> = snap
            .train_alerts
            .iter()
            .filter(|a| a.status == TrainAlertStatus::Disrupted)
            .flat_map(|a| a.affected_lines.iter().cloned())
            .collect();
        for feature in &data.mrt_lines.features {
            let line = &feature.properties.name;
            let colour = if disrupted
                .iter()
                .any(|d| line.contains(d) || d.contains(line))
            {
                Rgba::new(0.92, 0.72, 0.32, 1.0)
            } else {
                rgba(&feature.properties.color)
            };
            let width = if feature.properties.mode.as_deref() == Some("lrt") {
                2.0
            } else {
                3.2
            };
            let id = next_id;
            next_id += 1;
            ov.lines.push(Polyline {
                id,
                points: feature
                    .geometry
                    .coordinates
                    .iter()
                    .map(|p| ll(*p))
                    .collect(),
                color: colour,
                width,
            });
            targets.insert(id, Target::MrtStation(line.clone()));
        }
        let stations = argus_core::geometry::build_mrt_station_geojson(&data.mrt_lines);
        for station in stations {
            let p = [
                station.geometry.coordinates[0],
                station.geometry.coordinates[1],
            ];
            station_positions
                .entry(station.properties.name.clone())
                .or_insert(p);
            add_point(
                &mut ov,
                &mut next_id,
                &mut targets,
                p,
                rgba(&station.properties.color),
                Icon::Circle,
                3.0,
                Target::MrtStation(station.properties.name.clone()),
            );
        }
    }

    // Planned MRT route, drawn on top of the network.
    if let Some(plan) = s.route.plan.as_ref() {
        for segment in &plan.segments {
            let pts: Vec<MapLngLat> = segment
                .stations
                .iter()
                .filter_map(|name| station_positions.get(name).copied())
                .map(ll)
                .collect();
            if pts.len() >= 2 {
                ov.lines.push(Polyline {
                    id: next_id,
                    points: pts,
                    color: Rgba::new(0.98, 0.98, 0.98, 0.95),
                    width: 5.0,
                });
                next_id += 1;
            }
        }
    }

    // Bus route geometry from argus-data (road-following polylines).
    if s.bus_route.shown
        && let Some(route) = snap.bus_routes.as_ref()
    {
        for dir in &route.directions {
            if dir.path.len() < 2 {
                continue;
            }
            ov.lines.push(Polyline {
                id: next_id,
                points: dir.path.iter().map(|p| ll(*p)).collect(),
                color: Rgba::new(0.50, 0.94, 0.68, if dir.preferred { 0.95 } else { 0.45 }),
                width: if dir.preferred { 4.0 } else { 2.0 },
            });
            next_id += 1;
        }
    }

    // Amenity POIs from the static bundles, honouring sub-filters.
    if let Some(registry) = s.registry.as_ref() {
        for layer in &registry.layers {
            if !visible_on(s, &layer.layer_key) {
                continue;
            }
            let places = visible_places(layer, &s.amenity_filters);
            let colour = rgba(&layer.color);
            for place in within_radius(places, filter.as_ref(), |p| [p.lng(), p.lat()]) {
                let point = registry.display_point(layer.amenity_type, place);
                add_point(
                    &mut ov,
                    &mut next_id,
                    &mut targets,
                    [point.lng, point.lat],
                    colour,
                    Icon::Pin,
                    5.0,
                    Target::Amenity {
                        kind: layer.amenity_type,
                        id: place.id().to_string(),
                    },
                );
            }
        }
    }

    // Live feeds.
    if visible_on(s, "busStops") {
        for stop in within_radius(snap.bus_stops.clone(), filter.as_ref(), |b| {
            [b.longitude, b.latitude]
        }) {
            add_point(
                &mut ov,
                &mut next_id,
                &mut targets,
                [stop.longitude, stop.latitude],
                BUS_COLOR,
                Icon::Square,
                4.0,
                Target::BusStop(stop.bus_stop_code.clone()),
            );
        }
    }
    if visible_on(s, "cameras") {
        for cam in within_radius(
            snap.cameras.clone(),
            filter.as_ref(),
            |c: &TrafficCamera| [c.longitude, c.latitude],
        ) {
            add_point(
                &mut ov,
                &mut next_id,
                &mut targets,
                [cam.longitude, cam.latitude],
                CAMERA_COLOR,
                Icon::Cross,
                3.5,
                Target::Camera(cam.camera_id.clone()),
            );
        }
    }
    if visible_on(s, "incidents") {
        for inc in within_radius(snap.incidents.clone(), filter.as_ref(), |i| [i.lng, i.lat]) {
            add_point(
                &mut ov,
                &mut next_id,
                &mut targets,
                [inc.lng, inc.lat],
                INCIDENT_COLOR,
                Icon::Diamond,
                6.0,
                Target::Incident(inc.id.clone()),
            );
        }
    }
    if visible_on(s, "carparks") {
        for cp in within_radius(snap.carparks.clone(), filter.as_ref(), |c| [c.lng, c.lat]) {
            add_point(
                &mut ov,
                &mut next_id,
                &mut targets,
                [cp.lng, cp.lat],
                CARPARK_COLOR,
                Icon::Square,
                4.5,
                Target::Carpark(cp.id.clone()),
            );
        }
    }
    if visible_on(s, "evChargers") {
        for ev in within_radius(snap.ev_chargers.clone(), filter.as_ref(), |e| {
            [e.lng, e.lat]
        }) {
            add_point(
                &mut ov,
                &mut next_id,
                &mut targets,
                [ev.lng, ev.lat],
                EV_COLOR,
                Icon::Diamond,
                4.5,
                Target::Ev(ev.id.clone()),
            );
        }
    }
    if visible_on(s, "flights") {
        for flight in within_radius(snap.flights.clone(), filter.as_ref(), |f| {
            [f.longitude, f.latitude]
        }) {
            let idx = match flight.direction {
                argus_core::types::FlightDirection::Inbound => 0,
                argus_core::types::FlightDirection::Outbound => 1,
                argus_core::types::FlightDirection::Transit => 2,
            };
            add_point(
                &mut ov,
                &mut next_id,
                &mut targets,
                [flight.longitude, flight.latitude],
                FLIGHT_COLORS[idx],
                Icon::Cross,
                4.5,
                Target::Flight(flight.id.clone()),
            );
        }
    }

    // Radius circle (drawn above the basemap, below the selected marker).
    if let Some(filter) = filter.as_ref() {
        ov.circles.push(RadiusCircle {
            id: next_id,
            center: ll(filter.center),
            radius_m: filter.radius_m,
            stroke: Rgba::new(0.46, 0.74, 0.93, 0.9),
            fill: Rgba::new(0.46, 0.74, 0.93, 0.07),
        });
        next_id += 1;
    }

    // The selected POI gets a ring so it stays visible among dense markers.
    if let Some(sel) = selected_position(s) {
        let id = next_id;
        ov.points.push(MapPoint::new(
            id,
            ll(sel),
            PointStyle {
                radius: 12.0,
                color: Rgba::TRANSPARENT,
                stroke: Some(Rgba::new(1.0, 1.0, 1.0, 0.95)),
                icon: Icon::Circle,
                label: None,
            },
        ));
        if let Some(selection) = s.selection.as_ref() {
            targets.insert(id, target_for_selection(selection));
        }
    }

    (ov, targets)
}

fn target_for_selection(sel: &Selection) -> Target {
    match sel {
        Selection::Amenity { kind, id } => Target::Amenity {
            kind: *kind,
            id: id.clone(),
        },
        Selection::Park { name, kind } => Target::Park {
            name: name.clone(),
            kind: *kind,
        },
        Selection::MrtStation(name) => Target::MrtStation(name.clone()),
        Selection::Flight(id) => Target::Flight(id.clone()),
        Selection::Camera(id) => Target::Camera(id.clone()),
        Selection::BusStop(id) => Target::BusStop(id.clone()),
        Selection::Carpark(id) => Target::Carpark(id.clone()),
        Selection::Ev(id) => Target::Ev(id.clone()),
        Selection::Incident(id) => Target::Incident(id.clone()),
    }
}

fn selected_position(s: &AppState) -> Option<LngLat> {
    let snap = s.data.snapshot();
    match s.selection.as_ref()? {
        Selection::Amenity { kind, id } => {
            let layer = s
                .registry
                .as_ref()?
                .layers
                .iter()
                .find(|l| l.amenity_type == *kind)?;
            let place = layer.places.get(*layer.by_id.get(id)?)?;
            let p = s.registry.as_ref()?.display_point(*kind, place);
            Some([p.lng, p.lat])
        }
        Selection::Park { name, .. } => {
            let data = s.static_data.as_ref()?;
            let feature = data
                .parks_geojson
                .features
                .iter()
                .find(|f| &f.properties.name == name)?;
            let path = park_paths(&feature.geometry).into_iter().next()?;
            path.first().copied()
        }
        Selection::MrtStation(name) => {
            let data = s.static_data.as_ref()?;
            let stations = argus_core::geometry::build_mrt_station_geojson(&data.mrt_lines);
            let st = stations
                .into_iter()
                .find(|st| &st.properties.name == name)?;
            Some([st.geometry.coordinates[0], st.geometry.coordinates[1]])
        }
        Selection::Flight(id) => snap
            .flights
            .iter()
            .find(|f| &f.id == id)
            .map(|f| [f.longitude, f.latitude]),
        Selection::Camera(id) => snap
            .cameras
            .iter()
            .find(|c| &c.camera_id == id)
            .map(|c| [c.longitude, c.latitude]),
        Selection::BusStop(code) => snap
            .bus_stops
            .iter()
            .find(|b| &b.bus_stop_code == code)
            .map(|b| [b.longitude, b.latitude]),
        Selection::Carpark(id) => snap
            .carparks
            .iter()
            .find(|c| &c.id == id)
            .map(|c| [c.lng, c.lat]),
        Selection::Ev(id) => snap
            .ev_chargers
            .iter()
            .find(|e| &e.id == id)
            .map(|e| [e.lng, e.lat]),
        Selection::Incident(id) => snap
            .incidents
            .iter()
            .find(|i| &i.id == id)
            .map(|i| [i.lng, i.lat]),
    }
}

/// Convenience for the POI detail dialog: a short label per amenity type.
pub fn amenity_label(kind: AmenityType) -> &'static str {
    match kind {
        AmenityType::Mall => "Shopping Mall",
        AmenityType::Hawker => "Hawker Centre",
        AmenityType::Gym => "Gym",
        AmenityType::CommunityClub => "Community Club",
        AmenityType::Pharmacy => "Pharmacy",
        AmenityType::Hotel => "Hotel",
        AmenityType::Heritage => "Heritage Site",
        AmenityType::Healthcare => "Healthcare",
        AmenityType::Library => "Public Library",
        AmenityType::Supermarket => "Supermarket",
        AmenityType::School => "School",
    }
}
