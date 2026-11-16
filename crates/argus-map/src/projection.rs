//! Web Mercator projection and tile-coverage math.
//!
//! All functions here are pure (no Rune / network dependency) so they can be
//! unit-tested cheaply. Coordinates follow the slippy-map convention used by
//! MapLibre/OpenStreetMap:
//!
//! - longitude increases east, latitude increases north (WGS84 degrees),
//! - world pixel `(0, 0)` is the north-west corner at zoom 0,
//! - one tile is [`TILE_SIZE`] pixels square, world size is
//!   `TILE_SIZE * 2^zoom`.

use std::f64::consts::PI;

/// Raster tile edge length in device-independent pixels (the OSM standard).
pub const TILE_SIZE: f64 = 256.0;
/// Max latitude Web Mercator can represent; beyond this the projection blows up.
pub const MAX_MERCATOR_LAT: f64 = 85.0511287798066;

/// A longitude/latitude in WGS84 degrees. `lon` is `x`, `lat` is `y`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LngLat {
    pub lon: f64,
    pub lat: f64,
}

impl LngLat {
    pub const fn new(lon: f64, lat: f64) -> Self {
        Self { lon, lat }
    }
}

/// A slippy-map tile coordinate at integer zoom `z`. `x` grows east, `y` south.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TileCoord {
    pub z: u8,
    pub x: u64,
    pub y: u64,
}

impl TileCoord {
    pub const fn new(z: u8, x: u64, y: u64) -> Self {
        Self { z, x, y }
    }

    /// Number of tiles along each axis at this zoom.
    pub fn axis_len(&self) -> u64 {
        1u64 << self.z
    }

    /// Whether `self` is inside the world at its zoom.
    pub fn is_valid(&self) -> bool {
        let n = self.axis_len();
        self.x < n && self.y < n
    }

    /// The parent tile one zoom level out (`None` at zoom 0).
    pub fn parent(&self) -> Option<Self> {
        (self.z > 0).then(|| Self::new(self.z - 1, self.x / 2, self.y / 2))
    }

    /// All ancestors from the immediate parent up to zoom 0, nearest first.
    pub fn ancestors(&self) -> Vec<Self> {
        let mut out = Vec::with_capacity(self.z as usize);
        let mut cur = *self;
        while let Some(p) = cur.parent() {
            out.push(p);
            cur = p;
        }
        out
    }
}

/// It takes `2^zoom` world widths to cover the globe at a zoom level.
pub fn world_size(zoom: f64) -> f64 {
    TILE_SIZE * 2f64.powf(zoom)
}

/// Clamp a latitude to the range Web Mercator can represent.
pub fn clamp_lat(lat: f64) -> f64 {
    lat.clamp(-MAX_MERCATOR_LAT, MAX_MERCATOR_LAT)
}

/// Longitude -> world x pixel at `zoom` (no clamp: wraps across the antimeridian).
pub fn lon_to_world_x(lon: f64, zoom: f64) -> f64 {
    (lon + 180.0) / 360.0 * world_size(zoom)
}

/// Latitude -> world y pixel at `zoom`.
pub fn lat_to_world_y(lat: f64, zoom: f64) -> f64 {
    let lat = clamp_lat(lat).to_radians();
    let n = 1.0 - (lat.tan() + 1.0 / lat.cos()).ln() / PI;
    n / 2.0 * world_size(zoom)
}

/// World x pixel -> longitude.
pub fn world_x_to_lon(x: f64, zoom: f64) -> f64 {
    x / world_size(zoom) * 360.0 - 180.0
}

/// World y pixel -> latitude. Inverse of `lat_to_world_y`:
/// `φ = 2·atan(e^(π·n)) − π/2` where `n = 1 − 2y/W`.
pub fn world_y_to_lat(y: f64, zoom: f64) -> f64 {
    let n = 1.0 - 2.0 * y / world_size(zoom);
    (2.0 * (n * PI).exp().atan() - PI / 2.0) * 180.0 / PI
}

/// Project a lng/lat into fractional world pixels at `zoom`.
pub fn project(lnglat: LngLat, zoom: f64) -> (f64, f64) {
    (
        lon_to_world_x(lnglat.lon, zoom),
        lat_to_world_y(lnglat.lat, zoom),
    )
}

/// Unproject world pixels at `zoom` back into lng/lat.
pub fn unproject(x: f64, y: f64, zoom: f64) -> LngLat {
    LngLat::new(world_x_to_lon(x, zoom), world_y_to_lat(y, zoom))
}

/// Zoom used to rasterise tiles for a fractional camera zoom. At zoom `10.8`
/// this is `10`, and tiles are drawn at `2^0.8` (~1.74) times native size.
pub fn tile_zoom(zoom: f64) -> u8 {
    zoom.floor().clamp(0.0, 22.0) as u8
}

/// Scale factor to apply to a tile drawn at `tile.z` for a camera at `zoom`.
pub fn tile_scale(zoom: f64, tile_z: u8) -> f64 {
    2f64.powf(zoom - tile_z as f64)
}

/// Convert world pixels at `zoom` into a tile coordinate (flooring).
pub fn world_to_tile(x: f64, y: f64, zoom: f64) -> TileCoord {
    let n = 2u64.pow(tile_zoom(zoom) as u32);
    let scale = TILE_SIZE * 2f64.powf(zoom - tile_zoom(zoom) as f64);
    let tx = (x / scale).floor() as i64;
    let ty = (y / scale).floor() as i64;
    // Wrap longitude; clamp latitude (Mercator poles are unreachable).
    let tx = tx.rem_euclid(n as i64) as u64;
    let ty = ty.clamp(0, n as i64 - 1) as u64;
    TileCoord::new(tile_zoom(zoom), tx, ty)
}

/// The set of tiles that cover a viewport, plus their integer row/column range.
///
/// `center_world` is the camera centre in world pixels at `zoom`, and
/// `(width, height)` is the viewport size in points. `overscan` adds a margin
/// of tiles around the exact coverage so panning does not flash empty edges.
#[derive(Clone, Debug, PartialEq)]
pub struct TileCoverage {
    pub tiles: Vec<TileCoord>,
    pub min_x: i64,
    pub max_x: i64,
    pub min_y: i64,
    pub max_y: i64,
    pub zoom: u8,
}

/// Compute which tiles cover a viewport centred on `center_world`.
pub fn tile_coverage(
    center_world: (f64, f64),
    zoom: f64,
    size: (f32, f32),
    overscan: u32,
) -> TileCoverage {
    let z = tile_zoom(zoom);
    let n = 2u64.pow(z as u32) as i64;
    let scale = TILE_SIZE * 2f64.powf(zoom - z as f64); // drawn size of one tile
    let w = size.0.max(0.0) as f64;
    let h = size.1.max(0.0) as f64;
    let left = center_world.0 - w / 2.0;
    let top = center_world.1 - h / 2.0;
    let right = left + w;
    let bottom = top + h;
    let over = overscan as i64;
    let min_x = (left / scale).floor() as i64 - over;
    let max_x = (right / scale).floor() as i64 + over;
    let min_y = (top / scale).floor() as i64 - over;
    let max_y = (bottom / scale).floor() as i64 + over;

    let mut tiles = Vec::new();
    for ty in min_y..=max_y {
        let ty_u = ty.clamp(0, n - 1) as u64;
        for tx in min_x..=max_x {
            // Longitude wraps; latitude clamps.
            let tx_u = tx.rem_euclid(n) as u64;
            tiles.push(TileCoord::new(z, tx_u, ty_u));
        }
    }
    TileCoverage {
        tiles,
        min_x,
        max_x,
        min_y,
        max_y,
        zoom: z,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mercator_round_trips() {
        for (lon, lat) in [
            (103.8198, 1.3521),
            (0.0, 0.0),
            (-74.006, 40.7128),
            (103.6, -8.5),
            (179.9, 70.0),
        ] {
            let (x, y) = project(LngLat::new(lon, lat), 12.0);
            let back = unproject(x, y, 12.0);
            assert!((back.lon - lon).abs() < 1e-9, "lon {lon} -> {}", back.lon);
            assert!((back.lat - lat).abs() < 1e-9, "lat {lat} -> {}", back.lat);
        }
    }

    #[test]
    fn known_mercator_values() {
        // At zoom 0 the whole world is one 256px tile: (lon,lat)=(0,0) is its centre.
        let (x, y) = project(LngLat::new(0.0, 0.0), 0.0);
        assert!((x - TILE_SIZE / 2.0).abs() < 1e-9);
        assert!((y - TILE_SIZE / 2.0).abs() < 1e-9);
        // Singapore center maps into the upper-right quadrant at zoom 0.
        let (x, y) = project(LngLat::new(103.8198, 1.3521), 0.0);
        assert!(x > TILE_SIZE / 2.0 && y < TILE_SIZE / 2.0);
        // Zooming in doubles both axes.
        let p10 = project(LngLat::new(103.8198, 1.3521), 10.0);
        let p11 = project(LngLat::new(103.8198, 1.3521), 11.0);
        assert!((p11.0 - 2.0 * p10.0).abs() < 1e-6);
        assert!((p11.1 - 2.0 * p10.1).abs() < 1e-6);
    }

    #[test]
    fn tile_zoom_and_scale() {
        assert_eq!(tile_zoom(10.8), 10);
        assert_eq!(tile_zoom(11.0), 11);
        assert!((tile_scale(10.8, 10) - 2f64.powf(0.8)).abs() < 1e-12);
        assert!((tile_scale(11.0, 11) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn tile_coords_and_ancestors() {
        let t = TileCoord::new(3, 6, 4);
        assert!(t.is_valid());
        assert_eq!(t.parent(), Some(TileCoord::new(2, 3, 2)));
        assert_eq!(
            t.ancestors(),
            vec![
                TileCoord::new(2, 3, 2),
                TileCoord::new(1, 1, 1),
                TileCoord::new(0, 0, 0),
            ]
        );
        assert_eq!(TileCoord::new(0, 0, 0).parent(), None);
        assert!(!TileCoord::new(2, 4, 0).is_valid());
    }

    #[test]
    fn coverage_counts_and_overscan() {
        // A 512x512 viewport centred on the world origin at zoom 0 spans 3 tile
        // columns/rows exactly (x from -1..=1 in wrapped tile space), plus one
        // tile of overscan on each side with overscan=1.
        let cov = tile_coverage((TILE_SIZE / 2.0, TILE_SIZE / 2.0), 0.0, (512.0, 512.0), 0);
        assert_eq!(cov.tiles.len(), 9);
        assert_eq!((cov.min_x, cov.max_x, cov.min_y, cov.max_y), (-1, 1, -1, 1));
        let with_over = tile_coverage((TILE_SIZE / 2.0, TILE_SIZE / 2.0), 0.0, (512.0, 512.0), 1);
        assert_eq!(with_over.tiles.len(), 25);
        // All exact tiles are valid (wrapped/clamped), even with overscan.
        for t in &with_over.tiles {
            assert!(t.is_valid());
        }
    }

    #[test]
    fn coverage_around_singapore() {
        let (x, y) = project(LngLat::new(103.8198, 1.3521), 10.8);
        let cov = tile_coverage((x, y), 10.8, (960.0, 640.0), 1);
        assert!(!cov.tiles.is_empty());
        assert!(cov.tiles.iter().all(|t| t.z == 10));
        // x wraps but stays in [0, 1023]; y is within Mercator range.
        assert!(cov.tiles.iter().all(|t| t.x < 1024 && t.y < 1024));
    }
}
