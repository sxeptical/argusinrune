//! Background raster tile fetching, in-memory LRU, on-disk cache and parent fallback.
//!
//! * Rune can draw arbitrary raster images: [`rune::ImageCache::load`] decodes
//!   PNG/JPEG on its own worker and [`rune::el::el().pixels(..)`](rune::El::pixels)
//!   paints the handle. So we use a **dark raster basemap** instead of decoding
//!   vector tiles ourselves.
//! * Default source: **Esri World Dark Gray Canvas (Base)** — a genuinely
//!   keyless 256px dark raster basemap (JPEG). Attribution is required and is
//!   drawn by `map_view`. CARTO `dark_all` / `dark_nolabels` are also supported,
//!   but CARTO has required an API key since 2026-09-25: keyless requests serve
//!   an "API KEY REQUIRED" watermark. Set `ARGUS_CARTO_KEY` (or
//!   [`TileStore::with_key`]) to use CARTO without the watermark.
//! * Tiles are fetched on Rune worker threads through [`rune::Latest`] using
//!   `reqwest::blocking`; bytes land in state in `poll`, where an `Image` handle
//!   is created and cached. Decoding happens off the UI thread and wakes the app.
//! * Attribution is part of the public API ([`ATTRIBUTION`] /
//!   [`CARTO_ATTRIBUTION`]) and drawn by `map_view`. All requests send a
//!   descriptive `User-Agent`.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use rune::{Image, ImageCache, Latest, Waker};
use crate::projection::TileCoord;

/// Descriptive User-Agent for tile requests (CARTO asks clients to identify themselves).
pub const USER_AGENT: &str = "ArgusMap/0.1 (Singapore OSINT dashboard; https://github.com/sxeptical/argusinrune)";
/// Attribution for the default Esri Dark Gray basemap (must stay visible).
pub const ATTRIBUTION: &str = "Tiles © Esri · Esri, HERE, Garmin, © OpenStreetMap contributors";
/// Attribution for the optional CARTO basemaps.
pub const CARTO_ATTRIBUTION: &str = "© OpenStreetMap contributors · © CARTO";

/// Dark raster basemap source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TileSource {
    /// Esri World Dark Gray Canvas, **base** layer (roads/land, no labels).
    /// Keyless, dark, 256px JPEG. Default.
    EsriDarkGray,
    /// CARTO Dark Matter with labels (requires an API key).
    CartoDarkAll,
    /// CARTO Dark Matter without labels (requires an API key).
    CartoDarkNoLabels,
}

impl Default for TileSource {
    fn default() -> Self {
        Self::EsriDarkGray
    }
}

impl TileSource {
    /// Short directory name used by the on-disk cache.
    pub fn cache_name(self) -> &'static str {
        match self {
            Self::EsriDarkGray => "esri_dark_gray",
            Self::CartoDarkAll => "carto_dark_all",
            Self::CartoDarkNoLabels => "carto_dark_nolabels",
        }
    }

    /// The tile URL template. CARTO needs `?key=…` appended by the caller.
    pub fn template(self) -> &'static str {
        match self {
            Self::EsriDarkGray => "https://server.arcgisonline.com/ArcGIS/rest/services/Canvas/World_Dark_Gray_Base/MapServer/tile/{z}/{y}/{x}",
            Self::CartoDarkAll => "https://a.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}.png",
            Self::CartoDarkNoLabels => "https://a.basemaps.cartocdn.com/dark_nolabels/{z}/{x}/{y}.png",
        }
    }

    /// Attribution string that must be displayed for this source.
    pub fn attribution(self) -> &'static str {
        match self {
            Self::EsriDarkGray => ATTRIBUTION,
            Self::CartoDarkAll | Self::CartoDarkNoLabels => CARTO_ATTRIBUTION,
        }
    }

    /// Concrete URL for one tile. `key` is appended to CARTO requests.
    pub fn url(self, t: TileCoord, key: Option<&str>) -> String {
        let base = self.template()
            .replace("{z}", &t.z.to_string())
            .replace("{x}", &t.x.to_string())
            .replace("{y}", &t.y.to_string());
        match (self, key) {
            (Self::CartoDarkAll | Self::CartoDarkNoLabels, Some(k)) if !k.is_empty() => format!("{base}?key={k}"),
            _ => base,
        }
    }
}

/// A batch of fetched tiles. `None` bytes means the fetch failed; the caller
/// then falls back to an ancestor tile it already has.
pub type TileBatch = Vec<(TileCoord, Option<Vec<u8>>)>;

fn client() -> &'static reqwest::blocking::Client {
    static CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(12))
            .build()
            .expect("argus-map: tile HTTP client")
    })
}

/// Resolve a writable cache directory without adding a `dirs` dependency.
/// Honours `ARGUS_CACHE_DIR` and the platform cache location.
pub fn cache_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("ARGUS_CACHE_DIR") {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME").map(PathBuf::from).map(|p| p.join("Library/Caches"));
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")));
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    let root = base.unwrap_or_else(|| std::env::temp_dir());
    root.join("argus").join("argus-map").join("tiles")
}

fn disk_path(dir: &Path, source: TileSource, t: TileCoord) -> PathBuf {
    // Esri serves JPEG; CARTO serves PNG. Keep the real extension so content
    // sniffing / manual inspection works.
    let ext = if source == TileSource::EsriDarkGray { "jpg" } else { "png" };
    dir.join(source.cache_name())
        .join(t.z.to_string())
        .join(t.x.to_string())
        .join(format!("{}.{ext}", t.y))
}

/// A bounded LRU of decoded tile images. A small cap (a few hundred tiles)
/// keeps GPU image pages in check for a typical viewport.
struct ImageLru {
    map: HashMap<TileCoord, Image>,
    order: VecDeque<TileCoord>,
    cap: usize,
}

impl ImageLru {
    fn new(cap: usize) -> Self {
        Self { map: HashMap::new(), order: VecDeque::new(), cap }
    }

    fn get(&mut self, key: &TileCoord) -> Option<&Image> {
        if !self.map.contains_key(key) {
            return None;
        }
        if let Some(i) = self.order.iter().position(|k| k == key) {
            let k = self.order.remove(i).expect("position found");
            self.order.push_back(k);
        }
        self.map.get(key)
    }

    fn contains(&self, key: &TileCoord) -> bool {
        self.map.contains_key(key)
    }

    fn insert(&mut self, key: TileCoord, value: Image) {
        if self.map.insert(key, value).is_none() {
            self.order.push_back(key);
        }
        while self.order.len() > self.cap {
            if let Some(old) = self.order.pop_front() {
                self.map.remove(&old);
            }
        }
    }
}

/// Owns the tile pipeline. Created empty by `MapState::new`, wired to the app's
/// waker in `AppSpec::setup`, drained in `poll`.
pub struct TileStore {
    source: TileSource,
    api_key: Option<String>,
    cache_dir: PathBuf,
    fetch: Latest<TileBatch>,
    images: ImageLru,
    image_cache: Option<ImageCache>,
    /// Tiles requested but not yet landed (avoids duplicate network work).
    inflight: HashMap<TileCoord, u64>,
    generation: u64,
}

impl std::fmt::Debug for TileStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TileStore")
            .field("source", &self.source)
            .field("cached", &self.images.map.len())
            .field("inflight", &self.inflight.len())
            .finish()
    }
}

impl Default for TileStore {
    fn default() -> Self {
        Self::new(TileSource::default(), 256)
    }
}

impl TileStore {
    pub fn new(source: TileSource, memory_cap: usize) -> Self {
        Self {
            source,
            api_key: std::env::var("ARGUS_CARTO_KEY").ok().filter(|s| !s.is_empty()),
            cache_dir: cache_dir(),
            fetch: Latest::default().debounce(Duration::from_millis(40)),
            images: ImageLru::new(memory_cap.max(1)),
            image_cache: None,
            inflight: HashMap::new(),
            generation: 0,
        }
    }

    /// Attach the waker supplied by `AppSpec::setup`. Must be called before
    /// any tile is requested so decode completions wake the window.
    pub fn setup(&mut self, waker: Waker) {
        self.image_cache = Some(ImageCache::new(waker.clone()));
        self.fetch.set_waker(waker);
    }

    pub fn source(&self) -> TileSource {
        self.source
    }

    /// Optional CARTO API key (from `ARGUS_CARTO_KEY` or `with_key`).
    pub fn api_key(&self) -> Option<&str> {
        self.api_key.as_deref()
    }

    /// Attach a CARTO API key explicitly (overrides the env var).
    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    /// Attribution string for the configured source, for the app to draw.
    pub fn attribution(&self) -> &'static str {
        self.source.attribution()
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Request a batch of tiles. Dedupes against already-cached / in-flight
    /// tiles and is latest-wins: a camera move supersedes the previous batch.
    pub fn request(&mut self, coords: &[TileCoord]) {
        let mut needed = Vec::new();
        for &t in coords {
            if self.images.contains(&t) || self.inflight.contains_key(&t) {
                continue;
            }
            self.generation += 1;
            self.inflight.insert(t, self.generation);
            needed.push(t);
        }
        if needed.is_empty() {
            return;
        }
        let source = self.source;
        let key = self.api_key.clone();
        let dir = self.cache_dir.clone();
        let fetcher = move |list: Vec<TileCoord>| -> TileBatch {
            list.into_iter()
                .map(|t| {
                    let path = disk_path(&dir, source, t);
                    if let Ok(bytes) = std::fs::read(&path) {
                        return (t, Some(bytes));
                    }
                    let url = source.url(t, key.as_deref());
                    let got = client()
                        .get(&url)
                        .send()
                        .and_then(|r| r.error_for_status())
                        .and_then(|r| r.bytes())
                        .map(|b| b.to_vec())
                        .ok();
                    if let Some(bytes) = &got {
                        // Write atomically: a partial file is never a cache hit.
                        if let Some(parent) = path.parent() {
                            let _ = std::fs::create_dir_all(parent);
                            let ext = if source == TileSource::EsriDarkGray { "jpg.tmp" } else { "png.tmp" };
                            let tmp = path.with_extension(ext);
                            if std::fs::write(&tmp, bytes).is_ok() {
                                let _ = std::fs::rename(&tmp, &path);
                            }
                        }
                    }
                    (t, got)
                })
                .collect()
        };
        // `request_if_changed` would skip a repeated key; use `request` because
        // the same coordinates may become needed again after an LRU eviction.
        self.fetch.request(needed, fetcher);
    }

    /// Land a fetched batch: create image handles (decode runs on Rune's
    /// worker) and drop in-flight markers. Returns true if visible state changed.
    pub fn poll(&mut self) -> bool {
        let Some(batch) = self.fetch.take() else { return false };
        let mut changed = false;
        for (t, bytes) in batch {
            let Some(bytes) = bytes else {
                // Keep the in-flight marker on failure so a dead tile is not
                // re-requested every frame; parent fallback covers the gap.
                continue;
            };
            self.inflight.remove(&t); // may be re-requested if later evicted
            let Some(cache) = &self.image_cache else { continue };
            let image = cache.load(bytes);
            self.images.insert(t, image);
            changed = true;
        }
        changed
    }

    /// A ready (or pending) image for exactly this tile.
    pub fn image(&mut self, t: &TileCoord) -> Option<&Image> {
        self.images.get(t)
    }

    /// Immutable lookup used by the view (does not disturb LRU order).
    pub fn ready_image(&self, t: &TileCoord) -> Option<&Image> {
        self.images.map.get(t)
    }

    /// Find the nearest cached ancestor of `t`, nearest first. Used to paint a
    /// scaled low-zoom tile while its higher-zoom children load.
    pub fn ancestor_image(&self, t: &TileCoord) -> Option<(TileCoord, &Image)> {
        self.ready_ancestor(t)
    }

    /// Immutable ancestor lookup used by the view.
    pub fn ready_ancestor(&self, t: &TileCoord) -> Option<(TileCoord, &Image)> {
        for anc in t.ancestors() {
            if let Some(img) = self.images.map.get(&anc) {
                return Some((anc, img));
            }
        }
        None
    }

    pub fn cached_count(&self) -> usize {
        self.images.map.len()
    }

    /// Synchronously prime tiles for a deterministic headless snap: read from
    /// disk or fetch, decode and wait, then cache. Intended for `setup` only —
    /// it blocks the calling thread, so do not use it in an interactive frame.
    pub fn preload_sync(&mut self, coords: &[TileCoord]) -> usize {
        let mut count = 0;
        let dir = self.cache_dir.clone();
        let source = self.source;
        let key = self.api_key.clone();
        for t in coords {
            if self.images.contains(t) {
                count += 1;
                continue;
            }
            let path = disk_path(&dir, source, *t);
            let bytes = std::fs::read(&path).ok().or_else(|| {
                let got = client().get(&source.url(*t, key.as_deref())).send().and_then(|r| r.error_for_status()).and_then(|r| r.bytes()).map(|b| b.to_vec()).ok();
                if let Some(b) = &got {
                    if let Some(parent) = path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                        let _ = std::fs::write(&path, b);
                    }
                }
                got
            });
            let Some(bytes) = bytes else { continue };
            if let Some(cache) = &self.image_cache {
                let image = cache.load(bytes);
                let _ = image.wait(); // decode on Rune's worker; safe in setup
                self.images.insert(*t, image);
                count += 1;
                self.inflight.remove(t);
            }
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_urls() {
        let t = TileCoord::new(10, 822, 545);
        assert_eq!(
            TileSource::EsriDarkGray.url(t, None),
            "https://server.arcgisonline.com/ArcGIS/rest/services/Canvas/World_Dark_Gray_Base/MapServer/tile/10/545/822"
        );
        assert_eq!(
            TileSource::CartoDarkAll.url(t, None),
            "https://a.basemaps.cartocdn.com/dark_all/10/822/545.png"
        );
        assert_eq!(
            TileSource::CartoDarkAll.url(t, Some("KEY")),
            "https://a.basemaps.cartocdn.com/dark_all/10/822/545.png?key=KEY"
        );
        assert_eq!(
            TileSource::CartoDarkNoLabels.url(t, None),
            "https://a.basemaps.cartocdn.com/dark_nolabels/10/822/545.png"
        );
        assert!(TileSource::EsriDarkGray.template().contains("{z}"));
        assert_eq!(TileSource::default(), TileSource::EsriDarkGray);
        assert!(TileSource::EsriDarkGray.attribution().contains("Esri"));
    }

    #[test]
    fn lru_evicts_least_recently_used() {
        // A no-op waker is enough: load() returns a handle immediately even if
        // the bytes are not a valid image (we only test ordering here).
        let cache = ImageCache::new(std::sync::Arc::new(|| {}) as Waker);
        let image = cache.load(vec![0u8; 4]);
        let mut lru = ImageLru::new(2);
        let a = TileCoord::new(1, 0, 0);
        let b = TileCoord::new(1, 1, 0);
        let c = TileCoord::new(1, 0, 1);
        lru.insert(a, image.clone());
        lru.insert(b, image.clone());
        assert!(lru.get(&a).is_some()); // touch a -> b is now oldest
        lru.insert(c, image);
        assert!(lru.contains(&a), "a was touched, must survive");
        assert!(!lru.contains(&b), "b was least recently used");
        assert!(lru.contains(&c));
        assert_eq!(lru.order.len(), 2);
    }
}
