//! Rune image pipeline for hotlinked camera frames and place photos.
//!
//! Bytes come from [`argus_data::Client::get_image_bytes`] (which enforces the
//! LTA / Commons / NEA host allowlist) on a Rune `Latest` worker, then Rune's
//! [`ImageCache`] decodes them off the UI thread. The decoded [`Image`] handles
//! are cached by URL and painted with `El::pixels`, exactly like argus-map's
//! raster tiles. `preload_sync` is the deterministic headless-snap path: it
//! fetches and waits for decode in `AppSpec::setup`.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use argus_data::Client as DataClient;
use rune::{Image, ImageCache, Latest, Waker};

/// A batch of fetched images. `None` bytes means the fetch failed.
type ImageBatch = Vec<(String, Option<Vec<u8>>)>;

/// How long a failed image URL is left alone before it may be retried.
const RETRY_COOLDOWN_S: f64 = 30.0;

/// Decoded images keyed by URL. One store serves the camera panel/dialog and
/// every place-photo gallery, so a URL shared by two views is fetched once.
pub struct ImageStore {
    client: Option<Arc<DataClient>>,
    cache: Option<ImageCache>,
    images: HashMap<String, Image>,
    fetch: Latest<ImageBatch>,
    inflight: HashSet<String>,
    failed: HashMap<String, f64>,
    generation: u64,
}

impl std::fmt::Debug for ImageStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageStore")
            .field("cached", &self.images.len())
            .field("inflight", &self.inflight.len())
            .finish_non_exhaustive()
    }
}

impl Default for ImageStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ImageStore {
    pub fn new() -> Self {
        Self {
            client: None,
            cache: None,
            images: HashMap::new(),
            fetch: Latest::default().debounce(Duration::from_millis(40)),
            inflight: HashSet::new(),
            failed: HashMap::new(),
            generation: 0,
        }
    }

    /// Share the app's configured client (keys/UA never reach the UI).
    pub fn set_client(&mut self, client: Arc<DataClient>) {
        self.client = Some(client);
    }

    /// Attach the waker from `AppSpec::setup` before requesting anything.
    pub fn setup(&mut self, waker: Waker) {
        self.cache = Some(ImageCache::new(waker.clone()));
        self.fetch.set_waker(waker);
    }

    /// A ready decoded image for `url`, if it has been fetched and decoded.
    pub fn image(&self, url: &str) -> Option<&Image> {
        self.images.get(url).filter(|img| img.is_ready())
    }

    /// Queue a fetch for every URL in `urls` that is not already cached.
    ///
    /// `fresh` marks camera frames: their presigned URL can stay identical
    /// while the LTA replaces the JPEG, so a feed refresh must re-fetch them
    /// even when a handle for that URL exists. Returns true when a request was
    /// scheduled (the caller may want to wake for the answer).
    pub fn update(&mut self, now: f64, urls: &[String], fresh: &[String]) -> bool {
        let Some(client) = self.client.clone() else { return false };
        let mut needed: Vec<String> = Vec::new();
        let mut seen: HashSet<&str> = HashSet::new();
        for url in urls.iter().chain(fresh.iter()) {
            if !seen.insert(url.as_str()) {
                continue;
            }
            let is_fresh = fresh.iter().any(|u| u == url);
            if !is_fresh && (self.images.contains_key(url) || self.inflight.contains(url)) {
                continue;
            }
            if !is_fresh && self.failed.get(url).is_some_and(|at| now - *at < RETRY_COOLDOWN_S) {
                continue;
            }
            needed.push(url.clone());
        }
        self.inflight = needed.iter().cloned().collect();
        if needed.is_empty() {
            return false;
        }
        self.generation += 1;
        let generation = self.generation;
        self.fetch.request((needed, generation), move |(list, _)| {
            list.into_iter()
                .map(|url| {
                    let bytes = client.get_image_bytes(&url).ok();
                    (url, bytes)
                })
                .collect()
        });
        true
    }

    /// Land fetched bytes: create a decode handle and drop the in-flight mark.
    /// Returns true only when visible state changed.
    pub fn poll(&mut self, now: f64) -> bool {
        let Some(batch) = self.fetch.take() else { return false };
        let Some(cache) = &self.cache else { return false };
        let mut changed = false;
        for (url, bytes) in batch {
            self.inflight.remove(&url);
            match bytes {
                Some(bytes) => {
                    self.failed.remove(&url);
                    let image = cache.load(bytes);
                    // Supersede a stale frame for the same camera URL.
                    self.images.insert(url, image);
                    changed = true;
                }
                None => {
                    self.failed.insert(url, now);
                }
            }
        }
        changed
    }

    /// Deterministic snap path: fetch and decode before the single headless
    /// frame. Safe only in `setup` — it blocks the calling thread.
    pub fn preload_sync(&mut self, urls: &[String]) -> usize {
        let Some(client) = self.client.clone() else { return 0 };
        let Some(cache) = &self.cache else { return 0 };
        let mut count = 0;
        for url in urls {
            if self.images.contains_key(url) {
                count += 1;
                continue;
            }
            let Some(bytes) = client.get_image_bytes(url).ok() else { continue };
            let image = cache.load(bytes);
            if image.wait().is_err() {
                continue;
            }
            self.images.insert(url.clone(), image);
            self.inflight.remove(url);
            self.failed.remove(url);
            count += 1;
        }
        count
    }

    #[allow(dead_code)]
    pub fn cancel(&self) {
        self.fetch.cancel();
    }

    /// Cached URL count, for tests / source-health style diagnostics.
    #[allow(dead_code)]
    pub fn cached_count(&self) -> usize {
        self.images.len()
    }

    /// Number of URLs currently waiting on a worker (tests / diagnostics).
    #[allow(dead_code)]
    pub fn inflight_count(&self) -> usize {
        self.inflight.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(n: usize) -> String {
        format!("https://thumb.wikimedia.org/wikipedia/commons/thumb/{n}.jpg")
    }

    #[test]
    fn update_dedupes_and_skips_cached_urls() {
        let mut store = ImageStore::new();
        // Without a client nothing is scheduled, but the store stays usable.
        assert!(!store.update(0.0, &[url(1), url(1)], &[]));
        assert_eq!(store.inflight_count(), 0);

        let client = Arc::new(argus_data::Client::new());
        store.set_client(client);
        assert!(store.update(0.0, &[url(1), url(1), url(2)], &[]));
        assert_eq!(store.inflight_count(), 2);
        // A second frame while in flight does not restart the work.
        assert!(!store.update(0.1, &[url(1), url(2)], &[]));
    }

    #[test]
    fn fresh_urls_bypass_the_cache_key() {
        let mut store = ImageStore::new();
        store.set_client(Arc::new(argus_data::Client::new()));
        let camera = "https://images.data.gov.sg/api/traffic-images/1701.jpg".to_string();
        assert!(store.update(0.0, &[], &[camera.clone()]));
        assert_eq!(store.inflight_count(), 1);
        // The same presigned URL is re-fetched on the next feed refresh.
        assert!(store.update(60.0, &[], &[camera]));
        assert_eq!(store.inflight_count(), 1);
    }

    #[test]
    fn failed_urls_are_negatively_cached() {
        let mut store = ImageStore::new();
        store.set_client(Arc::new(argus_data::Client::new()));
        let u = url(9);
        store.failed.insert(u.clone(), 10.0);
        assert!(!store.update(10.0 + RETRY_COOLDOWN_S - 1.0, &[u.clone()], &[]));
        assert!(store.update(10.0 + RETRY_COOLDOWN_S, &[u], &[]));
    }
}
