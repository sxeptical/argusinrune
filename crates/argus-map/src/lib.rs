//! `argus-map` — a slippy-map widget for [Rune](https://github.com/anomalyco/rune).
//!
//! # Design
//!
//! * **Raster basemap, not vector.** Rune can draw arbitrary raster images
//!   (`ImageCache::load` → `Image::pixels` → `El::pixels`) and has absolute
//!   positioning (`El::absolute`), so this crate fetches dark raster tiles
//!   rather than decoding MVT vector tiles itself. The default source is
//!   Esri World Dark Gray Canvas (keyless); CARTO `dark_all` /
//!   `dark_nolabels` are supported with an API key (`ARGUS_CARTO_KEY`).
//!   Overlays (points, lines, polygons, radius circle) are drawn GPU-side
//!   with `Shape`/`canvas`. Attribution is always shown.
//! * **Web Mercator** projection with a fractional-zoom camera and a
//!   Singapore bounds clamp, matching the original `Map.tsx` defaults
//!   (centre `103.8198, 1.3521`, zoom `10.8`).
//! * **Background tiles** via `rune::Latest` on worker threads using
//!   `reqwest::blocking`, with an in-memory LRU, an on-disk cache under the
//!   platform cache dir, and scaled parent-tile fallback while children load.
//! * **Domain-agnostic overlays** — the app maps its POIs / MRT lines / parks
//!   onto [`overlay::Overlays`] and consumes [`state::MapMsg`] events. This
//!   crate deliberately has no dependency on `argus-core`.
//!
//! # Usage
//!
//! ```ignore
//! use argus_map::{MapState, Overlays, map_view};
//!
//! struct App { map: MapState, overlays: Overlays }
//! // init: MapState::new(...), set viewport to the map element's size
//! fn view(s: &App, cx: &mut rune::Cx<App>) -> rune::El {
//!     let mut pane = argus_map::map_view_for(&s.map, &s.overlays, cx, |a| &mut a.map);
//!     pane = pane.grow(1.0).min_h(0.0); // size the returned element
//!     pane
//! }
//! // setup: s.map.setup(waker); then s.map.preload_sync() for headless snaps
//! // poll:  let changed = s.map.poll(); for msg in s.map.drain_messages() { ... }
//! ```
//!
//! See `examples/map_demo.rs` for a complete runnable app (including a named
//! scene for `--snap`) and `RUST_REWRITE.md` → "argus-map API".

pub mod camera;
pub mod overlay;
pub mod projection;
pub mod state;
pub mod tile;
pub mod view;

pub use camera::{DEFAULT_CENTER, DEFAULT_ZOOM, LatLngBounds, LngLat, MapCamera, MAX_ZOOM, MIN_ZOOM, SINGAPORE_BOUNDS};
pub use overlay::{
    ClusterOptions, DrawnPoint, Icon, MapPoint, OverlayId, Overlays, PointStyle, Polygon, Polyline, RadiusCircle, Rgba,
    point_in_polygon, radius_circle_ring,
};
pub use projection::{TileCoord, TILE_SIZE, tile_zoom};
pub use state::{MapMsg, MapState};
pub use tile::{ATTRIBUTION, CARTO_ATTRIBUTION, TileSource, TileStore, USER_AGENT, cache_dir};
pub use view::{map_view, map_view_for};
