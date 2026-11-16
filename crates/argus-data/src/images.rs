//! Image byte fetching for the Rune UI, with a strict URL allowlist.
//!
//! Camera frames and place photos are hotlinked from upstream hosts. The app
//! decodes them with Rune's `ImageCache`, but the *bytes* are fetched here so
//! the same host checks that validate the LTA feed and the static bundles also
//! guard every runtime request. Nothing here depends on Rune: the app calls
//! [`Client::get_image_bytes`] from a `rune::Latest` worker closure.

use crate::error::{Error, Result};
use crate::http::DEFAULT_TIMEOUT_MS;
use crate::lta::is_safe_camera_url;
use crate::Client;

/// Hard cap on a single image response (LTA frames are ~160 KiB, Commons
/// thumbnails ~40 KiB). Larger bodies are refused before they reach the UI.
pub const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;

/// Hosts allowed for place photos by the original CSP / bundle validator.
const PHOTO_HOSTS: &[&str] = &[
    "https://upload.wikimedia.org/",
    "https://thumb.wikimedia.org/",
    "https://www.nea.gov.sg/",
];

/// True for a camera frame or place-photo URL this app may fetch.
///
/// Cameras reuse the LTA allowlist in [`lta::is_safe_camera_url`](crate::lta::is_safe_camera_url);
/// photos reuse the hosts accepted by `bundle_validation::is_photo`. Matching
/// is on a lower-cased URL with the `https://host/` prefix, so
/// `www.nea.gov.sg.evil.example/` cannot sneak through.
pub fn is_allowed_image_url(url: &str) -> bool {
    if is_safe_camera_url(url) {
        return true;
    }
    let lower = url.to_ascii_lowercase();
    PHOTO_HOSTS.iter().any(|host| lower.starts_with(host))
}

impl Client {
    /// Fetch one allowlisted image as bytes. `Err(Forbidden)` for any other
    /// host, so an upstream cannot turn a feed into an arbitrary HTTP client.
    pub fn get_image_bytes(&self, url: &str) -> Result<Vec<u8>> {
        if !is_allowed_image_url(url) {
            return Err(Error::Forbidden {
                message: "image host is not allowlisted".into(),
            });
        }
        self.http().get_bytes_limited(
            "images",
            url,
            &[("Accept", "image/avif,image/webp,image/apng,image/*,*/*;q=0.8")],
            DEFAULT_TIMEOUT_MS,
            MAX_IMAGE_BYTES,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist_covers_cameras_and_photos_only() {
        assert!(is_allowed_image_url(
            "https://images.data.gov.sg/api/traffic-images/1701.jpg"
        ));
        assert!(is_allowed_image_url(
            "https://dm-traffic-camera-itsc.s3.ap-southeast-1.amazonaws.com/2026-11-17/00-55/2701.jpg?X-Amz-Security-Token=abc"
        ));
        assert!(is_allowed_image_url(
            "https://thumb.wikimedia.org/wikipedia/commons/thumb/0/08/Park.jpg/500px-Park.jpg"
        ));
        assert!(is_allowed_image_url("HTTPS://UPLOAD.WIKIMEDIA.ORG/a.jpg"));
        assert!(is_allowed_image_url("https://www.nea.gov.sg/images/hawker.jpg"));
        assert!(!is_allowed_image_url("https://www.nea.gov.sg.evil.example/a.jpg"));
        assert!(!is_allowed_image_url("https://evil.example/1701.jpg"));
        assert!(!is_allowed_image_url("http://images.data.gov.sg/insecure.jpg"));
        assert!(!is_allowed_image_url("file:///etc/passwd"));
    }

    #[test]
    fn disallowed_url_is_forbidden_without_network() {
        let client = Client::new();
        let err = client.get_image_bytes("https://evil.example/a.jpg").unwrap_err();
        assert!(matches!(err, Error::Forbidden { .. }));
    }
}
