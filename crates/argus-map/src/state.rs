//! `MapState` — everything the widget owns. The app owns a `MapState` (either
//! directly as its Rune state type or as a field) and drives it with `setup`,
//! `poll` and the handlers wired by [`map_view`](crate::view::map_view).

use std::sync::Arc;

use rune::Waker;

use crate::camera::{DEFAULT_CENTER, DEFAULT_ZOOM, LatLngBounds, LngLat, MapCamera, MIN_ZOOM, MAX_ZOOM, SINGAPORE_BOUNDS};
use crate::overlay::{ClusterOptions, OverlayId, Overlays};
use crate::projection::TileCoord;
use crate::tile::{TileSource, TileStore};

/// Messages emitted by the map for the app to consume in `poll` (or a handler).
#[derive(Clone, Debug, PartialEq)]
pub enum MapMsg {
    /// A point / polygon overlay was clicked (Rune overlay id).
    OverlayClicked(OverlayId),
    /// A drag / zoom gesture ended; the camera is now at `center`, `zoom`.
    CameraChanged { center: LngLat, zoom: f64 },
    /// While `geo_filter_picking`, a click sets the radius-filter centre.
    GeoFilterPicked(LngLat),
}

/// Widget state. Construct with [`MapState::new`], attach the waker in the app's
/// `AppSpec::setup`, and call [`MapState::poll`] from `AppSpec::poll`.
pub struct MapState {
    pub camera: MapCamera,
    /// Camera viewport in points. Set this to the map element's size; Rune's
    /// view runs before layout, so the size cannot be discovered from `Cx`.
    pub viewport: (f32, f32),
    /// Bounds / zoom limits (default: Singapore).
    pub bounds: LatLngBounds,
    /// Mirrored overlays used for hit-testing in handlers (no `Overlays` borrow
    /// can live inside a `'static` Rune handler, so we keep an Arc clone).
    overlays: Arc<Overlays>,
    /// Clustering / culling tuning for the current zoom.
    pub cluster: ClusterOptions,
    /// Id under the cursor, if any.
    pub hovered: Option<OverlayId>,
    /// Clicked overlay id, kept until the next click.
    pub selected: Option<OverlayId>,
    /// When true, the next click picks a radius-filter centre instead of a feature.
    pub geo_filter_picking: bool,
    /// Events for the app to drain.
    messages: Vec<MapMsg>,
    tiles: TileStore,
    last_drag: Option<(f32, f32)>,
    press_origin: Option<(f32, f32)>,
    last_pointer: (f32, f32),
    dragging: bool,
}

impl std::fmt::Debug for MapState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MapState")
            .field("camera", &self.camera)
            .field("hovered", &self.hovered)
            .field("selected", &self.selected)
            .field("messages", &self.messages.len())
            .field("tiles", &self.tiles)
            .finish()
    }
}

impl Default for MapState {
    fn default() -> Self {
        Self::new(DEFAULT_CENTER, DEFAULT_ZOOM, (960.0, 640.0))
    }
}

impl MapState {
    pub fn new(center: LngLat, zoom: f64, viewport: (f32, f32)) -> Self {
        let mut camera = MapCamera::new(center, zoom, viewport);
        camera.bounds = SINGAPORE_BOUNDS;
        camera.clamp();
        Self {
            camera,
            viewport,
            bounds: SINGAPORE_BOUNDS,
            overlays: Arc::new(Overlays::new()),
            cluster: ClusterOptions::default(),
            hovered: None,
            selected: None,
            geo_filter_picking: false,
            messages: Vec::new(),
            tiles: TileStore::default(),
            last_drag: None,
            press_origin: None,
            last_pointer: (viewport.0 / 2.0, viewport.1 / 2.0),
            dragging: false,
        }
    }

    /// Use a different raster source (e.g. `DarkNoLabels` under dense overlays).
    pub fn with_tile_source(mut self, source: TileSource) -> Self {
        self.tiles = TileStore::new(source, 256);
        self
    }

    /// Attach the app waker (from `AppSpec::setup`). Required for background
    /// tile fetching and image decoding to wake the window.
    pub fn setup(&mut self, waker: Waker) {
        self.tiles.setup(waker);
    }

    /// Replace the hit-test mirror. Call whenever the app rebuilds the overlays
    /// it passes to `map_view` (cheap: one `Overlays` clone into an Arc).
    pub fn set_overlays(&mut self, overlays: &Overlays) {
        self.overlays = Arc::new(overlays.clone());
    }

    pub fn overlays(&self) -> &Overlays {
        &self.overlays
    }

    /// Update the map element size (from `on_layout` or app config), re-clamping.
    pub fn set_viewport(&mut self, size: (f32, f32)) {
        self.viewport = size;
        self.camera.set_viewport(size);
    }

    pub fn bounds(mut self, b: LatLngBounds) -> Self {
        self.bounds = b;
        self.camera.bounds = b;
        self.camera.clamp();
        self
    }

    pub fn zoom_limits(mut self, min: f64, max: f64) -> Self {
        self.camera.min_zoom = min.clamp(MIN_ZOOM, MAX_ZOOM);
        self.camera.max_zoom = max.clamp(self.camera.min_zoom, MAX_ZOOM);
        self.camera.clamp();
        self
    }

    /// Tiles currently covering the viewport, for the view to draw.
    pub fn visible_tiles(&self) -> Vec<TileCoord> {
        let (cx, cy) = self.camera.world_center();
        crate::projection::tile_coverage((cx, cy), self.camera.zoom, self.viewport, 1).tiles
    }

    /// Ask the worker pool to fetch any visible tiles not already cached.
    /// Safe to call every frame (`TileStore` dedupes).
    pub fn refresh_tiles(&mut self) {
        let coords = self.visible_tiles();
        self.tiles.request(&coords);
    }

    /// Land fetched tiles / decoded images. Returns true when visible state changed.
    pub fn poll(&mut self) -> bool {
        let changed = self.tiles.poll();
        self.refresh_tiles();
        changed
    }

    /// Synchronously preload tiles for a deterministic headless snap. Call in
    /// `AppSpec::setup` only (it blocks while fetching/decoding).
    pub fn preload_sync(&mut self) -> usize {
        let coords = self.visible_tiles();
        self.tiles.preload_sync(&coords)
    }

    pub fn tile_store(&self) -> &TileStore {
        &self.tiles
    }

    /// Attribution string for the configured basemap (draw it on the map).
    pub fn attribution(&self) -> &'static str {
        self.tiles.attribution()
    }

    pub fn tile_store_mut(&mut self) -> &mut TileStore {
        &mut self.tiles
    }

    /// Drain queued messages (call from `poll` or a handler).
    pub fn drain_messages(&mut self) -> Vec<MapMsg> {
        std::mem::take(&mut self.messages)
    }

    pub fn push_message(&mut self, msg: MapMsg) {
        self.messages.push(msg);
    }

    // ---- handlers -------------------------------------------------------

    /// Pointer went down on the map. `clicks >= 2` is a double-click zoom.
    pub fn pointer_down(&mut self, x: f32, y: f32, clicks: u32) {
        self.last_pointer = (x, y);
        if clicks >= 2 {
            self.camera.zoom_step(x, y, 1);
            self.push_message(MapMsg::CameraChanged { center: self.camera.center, zoom: self.camera.zoom });
            self.dragging = false;
            self.last_drag = Some((x, y));
            return;
        }
        self.dragging = true;
        self.last_drag = Some((x, y));
        self.press_origin = Some((x, y));
    }

    /// Pointer moved while dragging: pan by the incremental screen delta.
    pub fn pointer_drag(&mut self, x: f32, y: f32) {
        self.last_pointer = (x, y);
        if !self.dragging {
            return;
        }
        if let Some((lx, ly)) = self.last_drag {
            self.camera.pan_by_screen(x - lx, y - ly);
        }
        self.last_drag = Some((x, y));
    }

    /// Pointer released. A near-stationary gesture is a click: pick a feature
    /// (or the geo-filter centre), not a pan.
    pub fn pointer_up(&mut self, x: f32, y: f32, cancelled: bool, clicks: u32) {
        self.last_pointer = (x, y);
        let start = self.press_origin.take();
        self.last_drag = None;
        self.dragging = false;
        if cancelled || clicks >= 2 {
            return;
        }
        let travelled = start.map_or(0.0, |(sx, sy)| ((x - sx).powi(2) + (y - sy).powi(2)).sqrt());
        if travelled > 4.0 {
            self.push_message(MapMsg::CameraChanged { center: self.camera.center, zoom: self.camera.zoom });
            return;
        }
        if self.geo_filter_picking {
            let ll = self.camera.screen_to_lnglat(x, y);
            self.push_message(MapMsg::GeoFilterPicked(ll));
            return;
        }
        let mut hit = self.overlays.hit_test(&self.camera, x, y, 12.0);
        if hit.is_none() {
            hit = self.overlays.hit_test_polygon(&self.camera, x, y);
        }
        if let Some(id) = hit {
            self.selected = Some(id);
            self.push_message(MapMsg::OverlayClicked(id));
        }
    }

    /// Wheel zoom around the cursor (negative `dy` zooms in).
    pub fn wheel(&mut self, x: f32, y: f32, dy: f32) {
        self.last_pointer = (x, y);
        let delta = (-dy * 0.002).clamp(-1.0, 1.0);
        self.camera.zoom_at(x, y, delta as f64);
        self.push_message(MapMsg::CameraChanged { center: self.camera.center, zoom: self.camera.zoom });
    }

    /// Trackpad pinch zoom around the cursor.
    pub fn magnify(&mut self, x: f32, y: f32, delta: f32) {
        self.last_pointer = (x, y);
        self.camera.zoom_at(x, y, (delta * 2.0) as f64);
        self.push_message(MapMsg::CameraChanged { center: self.camera.center, zoom: self.camera.zoom });
    }

    /// Hover hit-test. Returns true when the hovered id changed (so the view
    /// can be invalidated only when needed).
    pub fn hover(&mut self, x: f32, y: f32) -> bool {
        self.last_pointer = (x, y);
        let hit = self.overlays.hit_test(&self.camera, x, y, 10.0);
        if hit != self.hovered {
            self.hovered = hit;
            true
        } else {
            false
        }
    }

    /// Jump to a lng/lat and zoom (e.g. from a search result).
    pub fn fly_to(&mut self, center: LngLat, zoom: f64) {
        self.camera.center = center;
        self.camera.zoom = zoom;
        self.camera.clamp();
        self.push_message(MapMsg::CameraChanged { center: self.camera.center, zoom: self.camera.zoom });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::{MapPoint, PointStyle};

    #[test]
    fn click_hits_overlay_and_emits_message() {
        let mut s = MapState::new(DEFAULT_CENTER, 12.0, (400.0, 300.0));
        let p = MapPoint::new(11, DEFAULT_CENTER, PointStyle::default());
        s.set_overlays(&Overlays::new().with_point(p));
        let (x, y) = s.camera.lnglat_to_screen(DEFAULT_CENTER);
        s.pointer_down(x, y, 1);
        s.pointer_up(x, y, false, 1);
        assert_eq!(s.selected, Some(11));
        assert_eq!(s.drain_messages(), vec![MapMsg::OverlayClicked(11)]);
    }

    #[test]
    fn drag_does_not_click_and_moves_camera() {
        let mut s = MapState::new(DEFAULT_CENTER, 12.0, (400.0, 300.0));
        let p = MapPoint::new(11, DEFAULT_CENTER, PointStyle::default());
        s.set_overlays(&Overlays::new().with_point(p));
        let before = s.camera.center;
        s.pointer_down(200.0, 150.0, 1);
        s.pointer_drag(240.0, 150.0);
        s.pointer_up(240.0, 150.0, false, 1);
        assert_eq!(s.selected, None);
        assert!(s.camera.center.lon < before.lon);
        assert!(s.drain_messages().iter().any(|m| matches!(m, MapMsg::CameraChanged { .. })));
    }

    #[test]
    fn double_click_zooms_in() {
        let mut s = MapState::new(DEFAULT_CENTER, 12.0, (400.0, 300.0));
        s.pointer_down(200.0, 150.0, 2);
        assert!((s.camera.zoom - 13.0).abs() < 1e-9);
    }

    #[test]
    fn geo_filter_pick_emits_center() {
        let mut s = MapState::new(DEFAULT_CENTER, 12.0, (400.0, 300.0));
        s.geo_filter_picking = true;
        s.pointer_down(210.0, 160.0, 1);
        s.pointer_up(210.0, 160.0, false, 1);
        let msgs = s.drain_messages();
        match &msgs[0] {
            MapMsg::GeoFilterPicked(ll) => assert!(s.camera.bounds.contains(*ll)),
            other => panic!("unexpected {other:?}"),
        }
    }
}
