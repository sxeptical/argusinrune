//! Radius filtering using a local flat projection scaled by WGS84 curvature.
//! Ported from `lib/geo-filter.ts`.

use crate::types::LngLat;

const WGS84_A: f64 = 6_378_137.0;
const WGS84_E2: f64 = 0.006_694_379_990_14;
const DEG_TO_RAD: f64 = std::f64::consts::PI / 180.0;

pub const GEO_FILTER_MIN_RADIUS_M: f64 = 250.0;
pub const GEO_FILTER_MAX_RADIUS_M: f64 = 5_000.0;
pub const GEO_FILTER_RADIUS_STEP_M: f64 = 250.0;
pub const GEO_FILTER_DEFAULT_RADIUS_M: f64 = 1_000.0;

/// `GeoFilter` from lib/geo-filter.ts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeoFilter {
    /// [lng, lat]
    pub center: LngLat,
    pub radius_m: f64,
}

/// Metres per radian of longitude (east) and latitude (north) at `lat`.
fn metres_per_radian(lat: f64) -> (f64, f64) {
    let phi = lat * DEG_TO_RAD;
    let w = 1.0 - WGS84_E2 * phi.sin().powi(2);
    let prime_vertical = WGS84_A / w.sqrt();
    let meridional = (WGS84_A * (1.0 - WGS84_E2)) / w.powf(1.5);
    (prime_vertical * phi.cos(), meridional)
}

/// Project [lng, lat] to metres east/north of `origin`.
fn to_local_metres(origin: LngLat, point: LngLat) -> (f64, f64) {
    let (east, north) = metres_per_radian(origin[1]);
    (
        (point[0] - origin[0]) * DEG_TO_RAD * east,
        (point[1] - origin[1]) * DEG_TO_RAD * north,
    )
}

/// Inverse of `to_local_metres`.
fn from_local_metres(origin: LngLat, xy: (f64, f64)) -> LngLat {
    let (east, north) = metres_per_radian(origin[1]);
    [
        origin[0] + xy.0 / east / DEG_TO_RAD,
        origin[1] + xy.1 / north / DEG_TO_RAD,
    ]
}

pub fn distance_m(from: LngLat, to: LngLat) -> f64 {
    let (x, y) = to_local_metres(from, to);
    x.hypot(y)
}

/// Distance from the local origin (0, 0) to segment a→b, in metres.
fn origin_to_segment_m(a: (f64, f64), b: (f64, f64)) -> f64 {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let length_sq = dx * dx + dy * dy;
    let t = if length_sq == 0.0 {
        0.0
    } else {
        (-(a.0 * dx + a.1 * dy) / length_sq).clamp(0.0, 1.0)
    };
    (a.0 + t * dx).hypot(a.1 + t * dy)
}

fn line_touches(filter: &GeoFilter, line: &[LngLat]) -> bool {
    if line.is_empty() {
        return false;
    }
    let projected: Vec<(f64, f64)> = line
        .iter()
        .map(|p| to_local_metres(filter.center, *p))
        .collect();
    if projected.len() == 1 {
        return projected[0].0.hypot(projected[0].1) <= filter.radius_m;
    }
    for pair in projected.windows(2) {
        if origin_to_segment_m(pair[0], pair[1]) <= filter.radius_m {
            return true;
        }
    }
    false
}

/// Ray-cast test: is the filter centre inside this ring?
fn center_in_ring(filter: &GeoFilter, ring: &[LngLat]) -> bool {
    let (x, y) = (filter.center[0], filter.center[1]);
    let mut inside = false;
    let n = ring.len();
    if n == 0 {
        return false;
    }
    for i in 0..n {
        let j = (i + n - 1) % n;
        let (xi, yi) = (ring[i][0], ring[i][1]);
        let (xj, yj) = (ring[j][0], ring[j][1]);
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
    }
    inside
}

fn polygon_touches(filter: &GeoFilter, rings: &[Vec<LngLat>]) -> bool {
    if rings.is_empty() {
        return false;
    }
    if rings.iter().any(|ring| line_touches(filter, ring)) {
        return true;
    }
    // Circle entirely inside the polygon (and not inside a hole).
    center_in_ring(filter, &rings[0]) && !rings[1..].iter().any(|hole| center_in_ring(filter, hole))
}

/// Geometry type of a `FilterableGeometry` (GeoJSON-like).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum FilterableGeometry {
    Point(LngLat),
    LineString(Vec<LngLat>),
    MultiLineString(Vec<Vec<LngLat>>),
    Polygon(Vec<Vec<LngLat>>),
    MultiPolygon(Vec<Vec<Vec<LngLat>>>),
}

impl FilterableGeometry {
    pub fn geometry_type(&self) -> &'static str {
        match self {
            FilterableGeometry::Point(_) => "Point",
            FilterableGeometry::LineString(_) => "LineString",
            FilterableGeometry::MultiLineString(_) => "MultiLineString",
            FilterableGeometry::Polygon(_) => "Polygon",
            FilterableGeometry::MultiPolygon(_) => "MultiPolygon",
        }
    }
}

use serde::{Deserialize, Serialize};

/// True when any part of the geometry lies within the radius.
pub fn geometry_touches_radius(filter: &GeoFilter, geometry: &FilterableGeometry) -> bool {
    match geometry {
        FilterableGeometry::Point(p) => distance_m(filter.center, *p) <= filter.radius_m,
        FilterableGeometry::LineString(line) => line_touches(filter, line),
        FilterableGeometry::MultiLineString(lines) => {
            lines.iter().any(|line| line_touches(filter, line))
        }
        FilterableGeometry::Polygon(rings) => polygon_touches(filter, rings),
        FilterableGeometry::MultiPolygon(polygons) => {
            polygons.iter().any(|rings| polygon_touches(filter, rings))
        }
    }
}

/// Parameter range [t0, t1] ⊆ [0, 1] of segment a→b inside the circle around
/// the local origin, or None when the segment stays outside.
fn segment_inside_range(a: (f64, f64), b: (f64, f64), radius_m: f64) -> Option<(f64, f64)> {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let qa = dx * dx + dy * dy;
    let qb = 2.0 * (a.0 * dx + a.1 * dy);
    let qc = a.0 * a.0 + a.1 * a.1 - radius_m * radius_m;
    if qa == 0.0 {
        return if qc <= 0.0 { Some((0.0, 1.0)) } else { None };
    }
    let discriminant = qb * qb - 4.0 * qa * qc;
    if discriminant < 0.0 {
        return None;
    }
    let root = discriminant.sqrt();
    let t0 = ((-qb - root) / (2.0 * qa)).max(0.0);
    let t1 = ((-qb + root) / (2.0 * qa)).min(1.0);
    if t0 < t1 { Some((t0, t1)) } else { None }
}

/// The parts of a line inside the radius, cut exactly at the circle. A line
/// that leaves and re-enters the circle yields several pieces. Pieces are
/// tracked by index so a piece can be extended after it is pushed.
pub fn clip_line_to_radius(filter: &GeoFilter, line: &[LngLat]) -> Vec<Vec<LngLat>> {
    let projected: Vec<(f64, f64)> = line
        .iter()
        .map(|p| to_local_metres(filter.center, *p))
        .collect();
    let mut pieces: Vec<Vec<LngLat>> = Vec::new();
    let mut current_idx: Option<usize> = None;
    for pair in projected.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let range = match segment_inside_range(a, b, filter.radius_m) {
            Some(r) => r,
            None => {
                current_idx = None;
                continue;
            }
        };
        let at = |t: f64| -> LngLat {
            from_local_metres(
                filter.center,
                (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1)),
            )
        };
        if current_idx.is_none() || range.0 > 0.0 {
            pieces.push(vec![at(range.0)]);
            current_idx = Some(pieces.len() - 1);
        }
        pieces[current_idx.unwrap()].push(at(range.1));
        if range.1 < 1.0 {
            current_idx = None;
        }
    }
    pieces
}

/// Regular polygon approximating the circle, in local metres (CCW).
fn circle_ring_local(radius_m: f64, steps: usize) -> Vec<(f64, f64)> {
    (0..steps)
        .map(|i| {
            let angle = (i as f64 / steps as f64) * 2.0 * std::f64::consts::PI;
            (radius_m * angle.cos(), radius_m * angle.sin())
        })
        .collect()
}

/// Where segment p→q crosses the infinite line a→b.
fn segment_line_intersection(
    p: (f64, f64),
    q: (f64, f64),
    a: (f64, f64),
    b: (f64, f64),
) -> (f64, f64) {
    let rx = q.0 - p.0;
    let ry = q.1 - p.1;
    let sx = b.0 - a.0;
    let sy = b.1 - a.1;
    let denominator = rx * sy - ry * sx;
    if denominator.abs() < 1e-12 {
        return p;
    }
    let t = ((a.0 - p.0) * sy - (a.1 - p.1) * sx) / denominator;
    (p.0 + t * rx, p.1 + t * ry)
}

/// Sutherland–Hodgman clip of a ring against a convex CCW clip polygon.
fn clip_ring_to_convex(subject: &[(f64, f64)], clip: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut output = subject.to_vec();
    for i in 0..clip.len() {
        if output.is_empty() {
            break;
        }
        let a = clip[i];
        let b = clip[(i + 1) % clip.len()];
        let input = output;
        output = Vec::new();
        let inside = |p: &(f64, f64)| (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0) >= 0.0;
        let n = input.len();
        for j in 0..n {
            let current = input[j];
            let previous = input[(j + n - 1) % n];
            let current_in = inside(&current);
            let previous_in = inside(&previous);
            if current_in {
                if !previous_in {
                    output.push(segment_line_intersection(previous, current, a, b));
                }
                output.push(current);
            } else if previous_in {
                output.push(segment_line_intersection(previous, current, a, b));
            }
        }
    }
    output
}

/// Clip one ring to the circle, keeping the original when wholly inside.
fn clip_ring_to_radius(filter: &GeoFilter, ring: &[LngLat], circle: &[(f64, f64)]) -> Vec<LngLat> {
    let local: Vec<(f64, f64)> = ring
        .iter()
        .map(|p| to_local_metres(filter.center, *p))
        .collect();
    let radius_sq = filter.radius_m * filter.radius_m;
    if local.iter().all(|p| p.0 * p.0 + p.1 * p.1 <= radius_sq) {
        return ring.to_vec();
    }
    clip_ring_to_convex(&local, circle)
        .into_iter()
        .map(|p| from_local_metres(filter.center, p))
        .collect()
}

/// Clip polygon rings to the circle. The first ring is the outer boundary.
fn clip_polygon_rings(filter: &GeoFilter, rings: &[Vec<LngLat>]) -> Option<Vec<Vec<LngLat>>> {
    if rings.is_empty() {
        return None;
    }
    let circle = circle_ring_local(filter.radius_m, 96);
    let outer = clip_ring_to_radius(filter, &rings[0], &circle);
    if outer.len() < 3 {
        return None;
    }
    let mut clipped = vec![outer];
    for hole in &rings[1..] {
        let clipped_hole = clip_ring_to_radius(filter, hole, &circle);
        if clipped_hole.len() >= 3 {
            clipped.push(clipped_hole);
        }
    }
    Some(clipped)
}

/// Geometry cut exactly at the radius, or None when nothing lies inside.
pub fn clip_geometry_to_radius(
    filter: &GeoFilter,
    geometry: &FilterableGeometry,
) -> Option<FilterableGeometry> {
    match geometry {
        FilterableGeometry::Point(p) => {
            if distance_m(filter.center, *p) <= filter.radius_m {
                Some(geometry.clone())
            } else {
                None
            }
        }
        FilterableGeometry::LineString(_) | FilterableGeometry::MultiLineString(_) => {
            let lines: Vec<&Vec<LngLat>> = match geometry {
                FilterableGeometry::LineString(line) => vec![line],
                FilterableGeometry::MultiLineString(lines) => lines.iter().collect(),
                _ => unreachable!(),
            };
            let pieces: Vec<Vec<LngLat>> = lines
                .iter()
                .flat_map(|line| clip_line_to_radius(filter, line))
                .collect();
            if pieces.is_empty() {
                None
            } else if pieces.len() == 1 {
                Some(FilterableGeometry::LineString(pieces[0].clone()))
            } else {
                Some(FilterableGeometry::MultiLineString(pieces))
            }
        }
        FilterableGeometry::Polygon(rings) => {
            clip_polygon_rings(filter, rings).map(FilterableGeometry::Polygon)
        }
        FilterableGeometry::MultiPolygon(polygons) => {
            let clipped: Vec<Vec<Vec<LngLat>>> = polygons
                .iter()
                .filter_map(|rings| clip_polygon_rings(filter, rings))
                .collect();
            if clipped.is_empty() {
                None
            } else if clipped.len() == 1 {
                Some(FilterableGeometry::Polygon(clipped[0].clone()))
            } else {
                Some(FilterableGeometry::MultiPolygon(clipped))
            }
        }
    }
}

/// Keep items whose position is within the radius; `None` filter keeps all.
/// Returns owned items so the caller does not need lifetime juggling.
pub fn within_radius<T, F>(items: Vec<T>, filter: Option<&GeoFilter>, to_lng_lat: F) -> Vec<T>
where
    F: Fn(&T) -> LngLat,
{
    match filter {
        None => items,
        Some(filter) => items
            .into_iter()
            .filter(|item| {
                let point = to_lng_lat(item);
                point[0].is_finite()
                    && point[1].is_finite()
                    && distance_m(filter.center, point) <= filter.radius_m
            })
            .collect(),
    }
}

/// Feature-collection variant of `within_radius` for static GeoJSON layers.
pub fn features_touching_radius<T, F>(
    features: Vec<T>,
    filter: Option<&GeoFilter>,
    to_geometry: F,
) -> Vec<T>
where
    F: Fn(&T) -> &FilterableGeometry,
{
    match filter {
        None => features,
        Some(filter) => features
            .into_iter()
            .filter(|feature| geometry_touches_radius(filter, to_geometry(feature)))
            .collect(),
    }
}

/// Closed polygon approximating the filter circle, for drawing on the map.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GeoFilterCircleGeoJson {
    pub r#type: String,
    pub features: Vec<GeoFilterCircleFeature>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GeoFilterCircleFeature {
    pub r#type: String,
    pub properties: serde_json::Map<String, serde_json::Value>,
    pub geometry: CirclePolygonGeometry,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CirclePolygonGeometry {
    pub r#type: String,
    pub coordinates: Vec<Vec<LngLat>>,
}

fn empty_circle_geojson() -> GeoFilterCircleGeoJson {
    GeoFilterCircleGeoJson {
        r#type: "FeatureCollection".into(),
        features: Vec::new(),
    }
}

pub fn build_geo_filter_circle_geojson(
    filter: Option<&GeoFilter>,
    steps: usize,
) -> GeoFilterCircleGeoJson {
    let filter = match filter {
        Some(f) => f,
        None => return empty_circle_geojson(),
    };
    let (lng, lat) = (filter.center[0], filter.center[1]);
    let (east, north) = metres_per_radian(lat);
    let east_deg = filter.radius_m / east / DEG_TO_RAD;
    let north_deg = filter.radius_m / north / DEG_TO_RAD;
    let mut ring: Vec<LngLat> = Vec::with_capacity(steps + 1);
    for i in 0..=steps {
        let theta = ((i % steps) as f64 / steps as f64) * 2.0 * std::f64::consts::PI;
        ring.push([lng + east_deg * theta.sin(), lat + north_deg * theta.cos()]);
    }
    GeoFilterCircleGeoJson {
        r#type: "FeatureCollection".into(),
        features: vec![GeoFilterCircleFeature {
            r#type: "Feature".into(),
            properties: serde_json::Map::new(),
            geometry: CirclePolygonGeometry {
                r#type: "Polygon".into(),
                coordinates: vec![ring],
            },
        }],
    }
}

/// World polygon with the filter circle cut out to dim exterior areas.
pub fn build_geo_filter_mask_geojson(filter: Option<&GeoFilter>) -> GeoFilterCircleGeoJson {
    let circle = build_geo_filter_circle_geojson(filter, 96);
    let Some(first) = circle.features.first() else {
        return empty_circle_geojson();
    };
    let world: Vec<LngLat> = vec![
        [-180.0, -85.0],
        [180.0, -85.0],
        [180.0, 85.0],
        [-180.0, 85.0],
        [-180.0, -85.0],
    ];
    let mut hole = first.geometry.coordinates[0].clone();
    hole.reverse();
    GeoFilterCircleGeoJson {
        r#type: "FeatureCollection".into(),
        features: vec![GeoFilterCircleFeature {
            r#type: "Feature".into(),
            properties: serde_json::Map::new(),
            geometry: CirclePolygonGeometry {
                r#type: "Polygon".into(),
                coordinates: vec![world, hole],
            },
        }],
    }
}

pub fn format_radius(radius_m: f64) -> String {
    if radius_m >= 1_000.0 {
        // toFixed(radiusM % 1000 === 0 ? 0 : 2).replace(/0$/, "")
        let decimals = if (radius_m % 1_000.0) == 0.0 { 0 } else { 2 };
        let mut s = format!("{:.*}", decimals, radius_m / 1_000.0);
        if decimals > 0 && s.ends_with('0') {
            s.pop();
        }
        format!("{} km", s)
    } else {
        format!("{} m", radius_m as i64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CENTER: LngLat = [103.8515, 1.2840];
    fn filter() -> GeoFilter {
        GeoFilter {
            center: CENTER,
            radius_m: 1_000.0,
        }
    }
    const DEG_PER_KM: f64 = 1.0 / 111.0;

    #[test]
    fn distance_matches_wgs84_geodesic() {
        for (point, expected) in [
            ([CENTER[0], CENTER[1] + 0.045], 4_975.868),
            ([CENTER[0] + 0.045, CENTER[1]], 5_008.128),
            ([CENTER[0] + 0.03, CENTER[1] - 0.03], 4_706.538),
        ] {
            assert!((distance_m(CENTER, point) - expected).abs() < 0.05);
        }
    }

    #[test]
    fn north_south_degrees_are_shorter_near_equator() {
        let north = distance_m(CENTER, [CENTER[0], CENTER[1] + 0.01]);
        let east = distance_m(CENTER, [CENTER[0] + 0.01, CENTER[1]]);
        assert!(north < east);
    }

    #[test]
    fn within_radius_keeps_only_points_inside() {
        let points = vec![
            ("near", CENTER[0], CENTER[1] + 0.5 * DEG_PER_KM),
            ("far", CENTER[0], CENTER[1] + 1.5 * DEG_PER_KM),
            ("bad", f64::NAN, CENTER[1]),
        ];
        let kept = within_radius(points.clone(), None, |p| [p.1, p.2]);
        assert_eq!(kept.len(), 3);
        let kept = within_radius(points, Some(&filter()), |p| [p.1, p.2]);
        assert_eq!(kept.iter().map(|p| p.0).collect::<Vec<_>>(), ["near"]);
    }

    #[test]
    fn geometry_touching_lines_and_polygons() {
        let f = filter();
        assert!(geometry_touches_radius(
            &f,
            &FilterableGeometry::LineString(vec![
                [CENTER[0] - 3.0 * DEG_PER_KM, CENTER[1]],
                [CENTER[0] + 3.0 * DEG_PER_KM, CENTER[1]],
            ])
        ));
        assert!(!geometry_touches_radius(
            &f,
            &FilterableGeometry::MultiLineString(vec![vec![
                [CENTER[0] - 3.0 * DEG_PER_KM, CENTER[1] + 2.0 * DEG_PER_KM],
                [CENTER[0] + 3.0 * DEG_PER_KM, CENTER[1] + 2.0 * DEG_PER_KM],
            ]])
        ));
        let d = 5.0 * DEG_PER_KM;
        assert!(geometry_touches_radius(
            &f,
            &FilterableGeometry::Polygon(vec![vec![
                [CENTER[0] - d, CENTER[1] - d],
                [CENTER[0] + d, CENTER[1] - d],
                [CENTER[0] + d, CENTER[1] + d],
                [CENTER[0] - d, CENTER[1] + d],
                [CENTER[0] - d, CENTER[1] - d],
            ]])
        ));
    }

    #[test]
    fn clip_line_cuts_at_edges_splits_and_drops() {
        let f = filter();
        let east = |km: f64| [CENTER[0] + km * DEG_PER_KM, CENTER[1]];
        let north = |km: f64| [CENTER[0], CENTER[1] + km * DEG_PER_KM];
        let on_circle = |p: LngLat| (distance_m(CENTER, p) - f.radius_m).abs() < 0.01;
        let pieces = clip_line_to_radius(&f, &[east(-3.0), east(3.0)]);
        assert_eq!(pieces.len(), 1);
        assert_eq!(pieces[0].len(), 2);
        assert!(on_circle(pieces[0][0]));
        assert!(on_circle(pieces[0][1]));
        let pieces = clip_line_to_radius(&f, &[east(-3.0), east(-0.2), north(0.3), east(3.0)]);
        assert_eq!(pieces.len(), 1);
        assert_eq!(pieces[0].len(), 4);
        let pieces = clip_line_to_radius(&f, &[east(-0.5), north(3.0), east(0.5)]);
        assert_eq!(pieces.len(), 2);
        assert!(clip_line_to_radius(&f, &[north(2.0), east(2.0)]).is_empty());
        let pieces = clip_line_to_radius(&f, &[east(-0.5), east(0.5)]);
        assert_eq!(pieces.len(), 1);
    }

    #[test]
    fn clip_geometry_polygon_and_multipolygon() {
        let f = filter();
        let east = |km: f64| [CENTER[0] + km * DEG_PER_KM, CENTER[1]];
        let north = |km: f64| [CENTER[0], CENTER[1] + km * DEG_PER_KM];
        let result = clip_geometry_to_radius(
            &f,
            &FilterableGeometry::Polygon(vec![vec![
                east(-3.0),
                north(-3.0),
                north(3.0),
                east(3.0),
                east(-3.0),
            ]]),
        );
        assert_eq!(result.as_ref().map(|g| g.geometry_type()), Some("Polygon"));
        let inside = vec![vec![
            east(-0.1),
            north(-0.1),
            north(0.1),
            east(0.1),
            east(-0.1),
        ]];
        let result = clip_geometry_to_radius(&f, &FilterableGeometry::Polygon(inside.clone()));
        assert_eq!(result, Some(FilterableGeometry::Polygon(inside)));
        let outside = vec![vec![
            east(3.0),
            north(3.0),
            north(4.0),
            east(4.0),
            east(3.0),
        ]];
        assert_eq!(
            clip_geometry_to_radius(&f, &FilterableGeometry::Polygon(outside)),
            None
        );
    }

    #[test]
    fn circle_and_mask_geojson() {
        assert!(
            build_geo_filter_circle_geojson(None, 96)
                .features
                .is_empty()
        );
        let ring = &build_geo_filter_circle_geojson(Some(&filter()), 32).features[0]
            .geometry
            .coordinates[0];
        assert_eq!(ring.len(), 33);
        assert_eq!(ring[0], ring[32]);
        for p in ring {
            assert!((distance_m(CENTER, *p) - 1_000.0).abs() < 0.5);
        }
        let mask = build_geo_filter_mask_geojson(Some(&filter()));
        let coords = &mask.features[0].geometry.coordinates;
        assert_eq!(coords[0][0], [-180.0, -85.0]);
        assert!((distance_m(CENTER, coords[1][0]) - 1_000.0).abs() < 0.5);
        assert!(build_geo_filter_mask_geojson(None).features.is_empty());
    }

    #[test]
    fn format_radius_formats_metres_and_kilometres() {
        assert_eq!(format_radius(250.0), "250 m");
        assert_eq!(format_radius(1_000.0), "1 km");
        assert_eq!(format_radius(1_250.0), "1.25 km");
        assert_eq!(format_radius(2_500.0), "2.5 km");
    }
}
