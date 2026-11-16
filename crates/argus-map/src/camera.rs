//! The map camera: fractional zoom, centre and viewport, with a Singapore clamp.

pub use crate::projection::LngLat;
use crate::projection::{
    clamp_lat, lat_to_world_y, lon_to_world_x, project, unproject, world_x_to_lon, world_y_to_lat,
};

/// Longitude/latitude rectangle. `south_west` / `north_east` are WGS84 degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LatLngBounds {
    pub min_lon: f64,
    pub min_lat: f64,
    pub max_lon: f64,
    pub max_lat: f64,
}

impl LatLngBounds {
    pub const fn new(min_lon: f64, min_lat: f64, max_lon: f64, max_lat: f64) -> Self {
        Self {
            min_lon,
            min_lat,
            max_lon,
            max_lat,
        }
    }

    pub fn contains(&self, p: LngLat) -> bool {
        p.lon >= self.min_lon
            && p.lon <= self.max_lon
            && p.lat >= self.min_lat
            && p.lat <= self.max_lat
    }

    pub fn center(&self) -> LngLat {
        LngLat::new(
            (self.min_lon + self.max_lon) / 2.0,
            (self.min_lat + self.max_lat) / 2.0,
        )
    }
}

/// Generous Singapore bounds (main island + outer islands), matching the
/// dashboard's area of interest. MapLibre itself has no `maxBounds`, so this
/// clamp is a deliberate restriction of the Rune widget.
pub const SINGAPORE_BOUNDS: LatLngBounds = LatLngBounds::new(103.55, 1.10, 104.15, 1.55);
pub const DEFAULT_CENTER: LngLat = LngLat::new(103.8198, 1.3521); // Map.tsx
pub const DEFAULT_ZOOM: f64 = 10.8; // Map.tsx
pub const MIN_ZOOM: f64 = 9.0;
pub const MAX_ZOOM: f64 = 17.0;

/// Camera state. All screen coordinates are in the map element's layout points,
/// measured from its top-left corner (Rune's `Drag`/`WheelEvent` convention).
#[derive(Clone, Debug, PartialEq)]
pub struct MapCamera {
    pub center: LngLat,
    pub zoom: f64,
    pub viewport: (f32, f32),
    pub bounds: LatLngBounds,
    pub min_zoom: f64,
    pub max_zoom: f64,
}

impl Default for MapCamera {
    fn default() -> Self {
        Self::new(DEFAULT_CENTER, DEFAULT_ZOOM, (960.0, 640.0))
    }
}

impl MapCamera {
    pub fn new(center: LngLat, zoom: f64, viewport: (f32, f32)) -> Self {
        let mut c = Self {
            center,
            zoom,
            viewport,
            bounds: SINGAPORE_BOUNDS,
            min_zoom: MIN_ZOOM,
            max_zoom: MAX_ZOOM,
        };
        c.clamp();
        c
    }

    /// Restrict the camera to Singapore and the supported zoom range. If the
    /// visible world is wider/taller than the bounds at this zoom, the centre
    /// is pinned to the bounds centre instead of jittering at the edges.
    pub fn clamp(&mut self) {
        self.zoom = self.zoom.clamp(self.min_zoom, self.max_zoom);
        let z = self.zoom;
        let (w, h) = (self.viewport.0 as f64, self.viewport.1 as f64);
        let west = lon_to_world_x(self.bounds.min_lon, z);
        let east = lon_to_world_x(self.bounds.max_lon, z);
        let north = lat_to_world_y(self.bounds.max_lat, z);
        let south = lat_to_world_y(self.bounds.min_lat, z);

        let mut cx = lon_to_world_x(self.center.lon, z);
        let mut cy = lat_to_world_y(self.center.lat, z);
        if east - west >= w {
            // Viewport fits inside the bounds: keep the whole view inside them.
            cx = cx.clamp(west + w / 2.0, east - w / 2.0);
        } else {
            // Viewport is wider than the bounds: just keep the *centre* inside
            // (pinning to the midpoint would move the Singapore default centre).
            cx = cx.clamp(west, east);
        }
        if south - north >= h {
            cy = cy.clamp(north + h / 2.0, south - h / 2.0);
        } else {
            cy = cy.clamp(north, south);
        }
        let mut center = LngLat::new(world_x_to_lon(cx, z), world_y_to_lat(cy, z));
        // Guard against float round-off at the projection/bounds boundary.
        center.lon = center.lon.clamp(self.bounds.min_lon, self.bounds.max_lon);
        center.lat = clamp_lat(center.lat).clamp(self.bounds.min_lat, self.bounds.max_lat);
        self.center = center;
    }

    /// Camera centre in world pixels at the current zoom.
    pub fn world_center(&self) -> (f64, f64) {
        project(self.center, self.zoom)
    }

    /// Convert a screen point (from the map element's top-left) to world pixels.
    pub fn screen_to_world(&self, sx: f32, sy: f32) -> (f64, f64) {
        let (cx, cy) = self.world_center();
        (
            cx + sx as f64 - self.viewport.0 as f64 / 2.0,
            cy + sy as f64 - self.viewport.1 as f64 / 2.0,
        )
    }

    /// Convert world pixels to a screen point.
    pub fn world_to_screen(&self, wx: f64, wy: f64) -> (f32, f32) {
        let (cx, cy) = self.world_center();
        (
            (wx - cx + self.viewport.0 as f64 / 2.0) as f32,
            (wy - cy + self.viewport.1 as f64 / 2.0) as f32,
        )
    }

    pub fn screen_to_lnglat(&self, sx: f32, sy: f32) -> LngLat {
        let (x, y) = self.screen_to_world(sx, sy);
        unproject(x, y, self.zoom)
    }

    pub fn lnglat_to_screen(&self, p: LngLat) -> (f32, f32) {
        let (x, y) = project(p, self.zoom);
        self.world_to_screen(x, y)
    }

    /// Drag the map: positive `dx` moves content right, so the centre moves left.
    pub fn pan_by_screen(&mut self, dx: f32, dy: f32) {
        let (mut cx, mut cy) = self.world_center();
        cx -= dx as f64;
        cy -= dy as f64;
        self.center = unproject(cx, cy, self.zoom);
        self.clamp();
    }

    /// Zoom by `delta` (log2), keeping the lng/lat under `(sx, sy)` fixed.
    pub fn zoom_at(&mut self, sx: f32, sy: f32, delta: f64) {
        let anchor = self.screen_to_lnglat(sx, sy);
        let new_zoom = (self.zoom + delta).clamp(self.min_zoom, self.max_zoom);
        if (new_zoom - self.zoom).abs() < 1e-9 {
            return;
        }
        self.zoom = new_zoom;
        // Put the anchor world pixel back under the cursor.
        let (ax, ay) = project(anchor, self.zoom);
        let (cx, cy) = self.world_center();
        let want_cx = ax + self.viewport.0 as f64 / 2.0 - sx as f64;
        let want_cy = ay + self.viewport.1 as f64 / 2.0 - sy as f64;
        let _ = (cx, cy);
        self.center = unproject(want_cx, want_cy, self.zoom);
        self.clamp();
    }

    /// One discrete zoom step (double-click / button), centred on a screen point.
    pub fn zoom_step(&mut self, sx: f32, sy: f32, direction: i32) {
        self.zoom_at(sx, sy, direction as f64);
    }

    /// Resize the viewport (then re-clamp the centre).
    pub fn set_viewport(&mut self, size: (f32, f32)) {
        self.viewport = (size.0.max(1.0), size.1.max(1.0));
        self.clamp();
    }

    /// True when a screen point is inside the viewport.
    pub fn contains_screen(&self, sx: f32, sy: f32) -> bool {
        sx >= 0.0 && sy >= 0.0 && sx <= self.viewport.0 && sy <= self.viewport.1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_map_tsx() {
        let c = MapCamera::default();
        assert!((c.center.lon - 103.8198).abs() < 1e-6);
        assert!((c.center.lat - 1.3521).abs() < 1e-6);
        assert!((c.zoom - 10.8).abs() < 1e-9);
    }

    #[test]
    fn screen_world_round_trip() {
        let c = MapCamera::default();
        for p in [(0.0, 0.0), (480.0, 320.0), (960.0, 640.0), (123.5, 77.25)] {
            let ll = c.screen_to_lnglat(p.0, p.1);
            let back = c.lnglat_to_screen(ll);
            assert!((back.0 - p.0).abs() < 1e-3, "{p:?} -> {back:?}");
            assert!((back.1 - p.1).abs() < 1e-3);
        }
    }

    #[test]
    fn zoom_at_keeps_anchor_fixed() {
        let mut c = MapCamera::default();
        let anchor = c.screen_to_lnglat(300.0, 200.0);
        c.zoom_at(300.0, 200.0, 1.0);
        let after = c.screen_to_lnglat(300.0, 200.0);
        assert!(
            (after.lon - anchor.lon).abs() < 1e-6,
            "{} vs {}",
            after.lon,
            anchor.lon
        );
        assert!((after.lat - anchor.lat).abs() < 1e-6);
        assert!((c.zoom - 11.8).abs() < 1e-9);
    }

    #[test]
    fn zoom_and_pan_clamp_to_singapore() {
        let mut c = MapCamera::default();
        for _ in 0..20 {
            c.zoom_step(480.0, 320.0, 1);
        }
        assert!(c.zoom <= MAX_ZOOM + 1e-9);
        for _ in 0..40 {
            c.zoom_step(480.0, 320.0, -1);
        }
        assert!(c.zoom >= MIN_ZOOM - 1e-9);
        for _ in 0..30 {
            c.pan_by_screen(500.0, 500.0); // push centre south-west
        }
        assert!(SINGAPORE_BOUNDS.contains(c.center));
        for _ in 0..60 {
            c.pan_by_screen(-500.0, -500.0); // north-east
        }
        assert!(SINGAPORE_BOUNDS.contains(c.center));
    }

    #[test]
    fn drag_moves_content_with_pointer() {
        let mut c = MapCamera::default();
        let before = c.screen_to_lnglat(480.0, 320.0);
        c.pan_by_screen(50.0, 0.0); // content moves right by 50px
        let after = c.screen_to_lnglat(480.0, 320.0);
        assert!(
            after.lon < before.lon,
            "dragging right reveals land to the west"
        );
    }
}
