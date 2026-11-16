//! Generic, domain-agnostic overlays: points, polylines, polygons and a radius
//! circle. The app maps its own data (POIs, MRT lines, parks, geo filter) onto
//! these types later; `argus-map` deliberately has no dependency on `argus-core`.
//!
//! Point drawing is GPU-friendly: points are culled to the viewport and, at low
//! zoom, collapsed into screen-space grid clusters so thousands of POIs stay
//! cheap. Hit-testing returns the generic [`OverlayId`] to the app.

use crate::camera::MapCamera;
use crate::projection::{LngLat, lat_to_world_y, lon_to_world_x};

/// Opaque id the app assigns to an overlay feature; returned on click.
pub type OverlayId = u64;

/// Straight-alpha sRGB colour, independent of Rune so this module stays pure.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub const TRANSPARENT: Self = Self::new(0.0, 0.0, 0.0, 0.0);

    pub fn to_rune(self) -> rune::Color {
        rune::Color { r: self.r, g: self.g, b: self.b, a: self.a }
    }
}

/// Point marker shape. `Pin` is a small teardrop approximation (a filled circle
/// with a stem); the rest are drawn as filled/stroked polygons in the canvas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Circle,
    Pin,
    Square,
    Diamond,
    Cross,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PointStyle {
    pub radius: f32,
    pub color: Rgba,
    pub stroke: Option<Rgba>,
    pub icon: Icon,
    pub label: Option<String>,
}

impl Default for PointStyle {
    fn default() -> Self {
        Self {
            radius: 5.0,
            color: Rgba::new(0.20, 0.90, 0.65, 1.0),
            stroke: Some(Rgba::new(0.04, 0.05, 0.06, 1.0)),
            icon: Icon::Circle,
            label: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MapPoint {
    pub id: OverlayId,
    pub position: LngLat,
    pub style: PointStyle,
}

impl MapPoint {
    pub fn new(id: OverlayId, position: LngLat, style: PointStyle) -> Self {
        Self { id, position, style }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Polyline {
    pub id: OverlayId,
    pub points: Vec<LngLat>,
    pub color: Rgba,
    pub width: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Polygon {
    pub id: OverlayId,
    pub points: Vec<LngLat>,
    pub fill: Rgba,
    pub stroke: Option<(Rgba, f32)>,
}

/// A radius circle (the geo filter), defined in metres on the ground.
#[derive(Clone, Debug, PartialEq)]
pub struct RadiusCircle {
    pub id: OverlayId,
    pub center: LngLat,
    pub radius_m: f64,
    pub stroke: Rgba,
    pub fill: Rgba,
}

/// All overlay geometry the app wants drawn. Cheap to rebuild each frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Overlays {
    pub points: Vec<MapPoint>,
    pub lines: Vec<Polyline>,
    pub polygons: Vec<Polygon>,
    pub circles: Vec<RadiusCircle>,
}

impl Overlays {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_point(mut self, p: MapPoint) -> Self {
        self.points.push(p);
        self
    }

    pub fn with_line(mut self, l: Polyline) -> Self {
        self.lines.push(l);
        self
    }

    pub fn with_polygon(mut self, p: Polygon) -> Self {
        self.polygons.push(p);
        self
    }

    pub fn with_circle(mut self, c: RadiusCircle) -> Self {
        self.circles.push(c);
        self
    }

    /// Compute the screen-space point clusters to draw for `camera`.
    pub fn drawn_points(&self, camera: &MapCamera, opts: ClusterOptions) -> Vec<DrawnPoint> {
        let cell = opts.cell_px.max(1.0);
        let margin = opts.margin_px.max(0.0);
        let (vw, vh) = (camera.viewport.0, camera.viewport.1);
        let mut buckets: std::collections::HashMap<(i64, i64), Vec<VisiblePoint<'_>>> =
            std::collections::HashMap::new();

        for p in &self.points {
            let (sx, sy) = camera.lnglat_to_screen(p.position);
            if sx < -margin || sy < -margin || sx > vw + margin || sy > vh + margin {
                continue;
            }
            let cluster = camera.zoom < opts.max_cluster_zoom;
            let key = if cluster {
                ((sx / cell).floor() as i64, (sy / cell).floor() as i64)
            } else {
                // Unique key: never group when clustering is off.
                (i64::MIN ^ p.id as i64, p.id as i64)
            };
            buckets.entry(key).or_default().push(VisiblePoint { point: p, sx, sy });
        }

        let mut out = Vec::with_capacity(buckets.len());
        for members in buckets.values() {
            if members.len() == 1 {
                let v = &members[0];
                out.push(DrawnPoint {
                    screen: (v.sx, v.sy),
                    ids: vec![v.point.id],
                    style: v.point.style.clone(),
                    count: 1,
                });
            } else {
                let n = members.len() as f32;
                let sx = members.iter().map(|m| m.sx).sum::<f32>() / n;
                let sy = members.iter().map(|m| m.sy).sum::<f32>() / n;
                let base = &members[0].point.style;
                out.push(DrawnPoint {
                    screen: (sx, sy),
                    ids: members.iter().map(|m| m.point.id).collect(),
                    style: PointStyle {
                        radius: base.radius + (n.ln() * 1.4).min(11.0),
                        color: base.color,
                        stroke: base.stroke,
                        icon: Icon::Circle,
                        label: Some(format!("{n}")),
                    },
                    count: members.len(),
                });
            }
        }
        out
    }

    /// Nearest overlay point to a screen position within `tolerance` points.
    /// Returns the app's id so it can open a popup / select a feature.
    pub fn hit_test(&self, camera: &MapCamera, sx: f32, sy: f32, tolerance: f32) -> Option<OverlayId> {
        let mut best: Option<(f32, OverlayId)> = None;
        let tol2 = tolerance * tolerance;
        for p in &self.points {
            let (px, py) = camera.lnglat_to_screen(p.position);
            let dx = px - sx;
            let dy = py - sy;
            let d2 = dx * dx + dy * dy;
            if d2 <= tol2 && best.map_or(true, |(b, _)| d2 < b) {
                best = Some((d2, p.id));
            }
        }
        best.map(|(_, id)| id)
    }

    /// Point-in-polygon hit test (ray casting), for park / area selection.
    pub fn hit_test_polygon(&self, camera: &MapCamera, sx: f32, sy: f32) -> Option<OverlayId> {
        for poly in &self.polygons {
            let pts: Vec<(f32, f32)> = poly.points.iter().map(|p| camera.lnglat_to_screen(*p)).collect();
            if point_in_polygon(sx, sy, &pts) {
                return Some(poly.id);
            }
        }
        None
    }
}

/// Tuning for screen-space point clustering / culling.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClusterOptions {
    /// Grid cell size in points; points in one cell merge into a cluster.
    pub cell_px: f32,
    /// Above this zoom points are never clustered.
    pub max_cluster_zoom: f64,
    /// Extra pixels outside the viewport kept so markers pop in smoothly.
    pub margin_px: f32,
}

impl Default for ClusterOptions {
    fn default() -> Self {
        Self { cell_px: 44.0, max_cluster_zoom: 12.0, margin_px: 24.0 }
    }
}

#[derive(Clone, Debug)]
struct VisiblePoint<'a> {
    point: &'a MapPoint,
    sx: f32,
    sy: f32,
}

/// One point (or cluster) ready to draw.
#[derive(Clone, Debug, PartialEq)]
pub struct DrawnPoint {
    pub screen: (f32, f32),
    pub ids: Vec<OverlayId>,
    pub style: PointStyle,
    pub count: usize,
}

/// Sample a circle on the ground into a lng/lat ring for drawing with `Shape`.
/// Uses the equirectangular approximation, which is accurate over Singapore's
/// small radius circle scales (a few km).
pub fn radius_circle_ring(center: LngLat, radius_m: f64, segments: usize) -> Vec<LngLat> {
    let segments = segments.max(8);
    let lat_rad = center.lat.to_radians();
    let d_lat = radius_m / 111_320.0;
    let d_lon = radius_m / (111_320.0 * lat_rad.cos().max(1e-6));
    (0..segments)
        .map(|i| {
            let a = 2.0 * std::f64::consts::PI * i as f64 / segments as f64;
            LngLat::new(center.lon + d_lon * a.cos(), center.lat + d_lat * a.sin())
        })
        .collect()
}

/// Ray-casting point-in-polygon test on screen coordinates.
pub fn point_in_polygon(x: f32, y: f32, poly: &[(f32, f32)]) -> bool {
    let mut inside = false;
    let mut j = poly.len().saturating_sub(1);
    for i in 0..poly.len() {
        let (xi, yi) = poly[i];
        let (xj, yj) = poly[j];
        if ((yi > y) != (yj > y)) && (x < (xj - xi) * (y - yi) / (yj - yi) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Distance in points from a screen point to a polyline (used for line hit-test).
pub fn distance_to_polyline(x: f32, y: f32, pts: &[(f32, f32)]) -> f32 {
    let mut best = f32::INFINITY;
    for w in pts.windows(2) {
        let (ax, ay) = w[0];
        let (bx, by) = w[1];
        let vx = bx - ax;
        let vy = by - ay;
        let len2 = vx * vx + vy * vy;
        let t = if len2 <= 0.0 {
            0.0
        } else {
            (((x - ax) * vx + (y - ay) * vy) / len2).clamp(0.0, 1.0)
        };
        let dx = ax + t * vx - x;
        let dy = ay + t * vy - y;
        best = best.min(dx * dx + dy * dy);
    }
    best.sqrt()
}

/// Convert a lng/lat to world pixels (re-exported helper for the app).
pub fn lnglat_to_world(p: LngLat, zoom: f64) -> (f64, f64) {
    (lon_to_world_x(p.lon, zoom), lat_to_world_y(p.lat, zoom))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(id: OverlayId, lon: f64, lat: f64) -> MapPoint {
        MapPoint::new(id, LngLat::new(lon, lat), PointStyle::default())
    }

    fn camera(zoom: f64) -> MapCamera {
        MapCamera::new(LngLat::new(103.8198, 1.3521), zoom, (400.0, 300.0))
    }

    #[test]
    fn clustering_groups_nearby_points_at_low_zoom() {
        let mut ov = Overlays::new();
        // Five points within a few pixels of each other at low zoom.
        for i in 0..5 {
            ov = ov.with_point(point(i, 103.8198 + i as f64 * 0.0001, 1.3521));
        }
        let drawn = ov.drawn_points(&camera(10.0), ClusterOptions::default());
        assert_eq!(drawn.len(), 1, "nearby points merge into one cluster");
        assert_eq!(drawn[0].count, 5);
        assert_eq!(drawn[0].ids.len(), 5);
        assert_eq!(drawn[0].style.label.as_deref(), Some("5"));
    }

    #[test]
    fn clustering_splits_far_points_and_stops_at_high_zoom() {
        let mut ov = Overlays::new();
        ov = ov.with_point(point(1, 103.82, 1.35));
        ov = ov.with_point(point(2, 103.90, 1.42)); // far away
        let drawn = ov.drawn_points(&camera(10.0), ClusterOptions::default());
        assert_eq!(drawn.len(), 2, "far points stay separate");
        // At high zoom the same near points are never merged.
        let mut near = Overlays::new();
        near = near.with_point(point(1, 103.8198, 1.3521));
        near = near.with_point(point(2, 103.81981, 1.35211));
        let drawn = near.drawn_points(&camera(14.0), ClusterOptions::default());
        assert_eq!(drawn.len(), 2, "no clustering above max_cluster_zoom");
    }

    #[test]
    fn culling_drops_points_outside_viewport() {
        let mut ov = Overlays::new();
        ov = ov.with_point(point(1, 103.8198, 1.3521)); // centre -> visible
        ov = ov.with_point(point(2, 100.0, 1.3521)); // far west -> culled
        let drawn = ov.drawn_points(&camera(11.0), ClusterOptions { margin_px: 0.0, ..Default::default() });
        assert_eq!(drawn.len(), 1);
        assert_eq!(drawn[0].ids, vec![1]);
    }

    #[test]
    fn hit_test_returns_nearest_id() {
        let mut ov = Overlays::new();
        ov = ov.with_point(point(7, 103.8198, 1.3521));
        ov = ov.with_point(point(8, 103.8198 + 0.02, 1.3521));
        let c = camera(12.0);
        let (sx, sy) = c.lnglat_to_screen(LngLat::new(103.8198, 1.3521));
        assert_eq!(ov.hit_test(&c, sx, sy, 12.0), Some(7));
        assert_eq!(ov.hit_test(&c, sx + 40.0, sy, 6.0), None);
    }

    #[test]
    fn polygon_and_line_hit_tests() {
        let mut ov = Overlays::new();
        let ring = vec![
            LngLat::new(103.81, 1.34),
            LngLat::new(103.83, 1.34),
            LngLat::new(103.83, 1.36),
            LngLat::new(103.81, 1.36),
        ];
        ov.polygons.push(Polygon {
            id: 42,
            points: ring,
            fill: Rgba::TRANSPARENT,
            stroke: None,
        });
        let c = camera(12.0);
        let (sx, sy) = c.lnglat_to_screen(LngLat::new(103.82, 1.35));
        assert_eq!(ov.hit_test_polygon(&c, sx, sy), Some(42));
        let (ox, oy) = c.lnglat_to_screen(LngLat::new(103.85, 1.39));
        assert_eq!(ov.hit_test_polygon(&c, ox, oy), None);
        let square = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        assert!(point_in_polygon(5.0, 5.0, &square));
        assert!(!point_in_polygon(20.0, 5.0, &square));
        assert_eq!(distance_to_polyline(5.0, 2.0, &[(0.0, 0.0), (10.0, 0.0)]), 2.0);
    }

    #[test]
    fn radius_circle_ring_has_right_size() {
        let ring = radius_circle_ring(LngLat::new(103.82, 1.35), 1000.0, 64);
        assert_eq!(ring.len(), 64);
        // 1km at 1.35N is ~0.009 degrees latitude.
        let lat_span = ring.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.lat), b.max(p.lat)));
        assert!((lat_span.1 - lat_span.0 - 2.0 * 1000.0 / 111_320.0).abs() < 1e-6);
    }
}
