//! Place photo gallery state and attribution text.
//!
//! Port of `app/components/dashboard/PlacePhotoGallery.tsx`. The gallery shows
//! one large photo, up to three thumbnails and the licence line the source
//! requires. Images themselves are decoded by `crate::images::ImageStore`.

use std::collections::HashSet;

use argus_core::types::{PlacePhoto, PlacePhotoSource};

/// UI state, reset whenever the focus dialog switches place.
#[derive(Clone, Debug, Default)]
pub struct PhotoGalleryState {
    pub place_key: Option<String>,
    pub active: usize,
    pub failed: HashSet<String>,
}

impl PhotoGalleryState {
    pub fn for_place(&mut self, key: &str) {
        if self.place_key.as_deref() == Some(key) {
            return;
        }
        self.place_key = Some(key.to_string());
        self.active = 0;
        self.failed.clear();
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn mark_failed(&mut self, url: &str) {
        self.failed.insert(url.to_string());
        if self.active > 0 {
            self.active -= 1;
        }
    }
}

/// Photos still usable after decode failures, in bundle order.
pub fn usable<'a>(photos: &'a [PlacePhoto], state: &PhotoGalleryState) -> Vec<&'a PlacePhoto> {
    photos.iter().filter(|p| !state.failed.contains(&p.thumb)).collect()
}

/// Clamp the active thumbnail when one is removed (the TS app uses
/// `Math.min(activeIndex, usable.length - 1)`).
pub fn active_index(state: &PhotoGalleryState, usable_len: usize) -> usize {
    if usable_len == 0 {
        0
    } else {
        state.active.min(usable_len - 1)
    }
}

/// The exact credit line the original renders for the active photo.
pub fn caption(photo: &PlacePhoto) -> String {
    if photo.source == PlacePhotoSource::Nea {
        return "Photo: NEA (National Environment Agency)".into();
    }
    let mut out = format!("Photo: {}", photo.author);
    if let Some(year) = photo.year {
        out.push_str(&format!(" · {year}"));
    }
    if let Some(license) = &photo.license {
        out.push_str(&format!(" · {license}"));
    }
    out.push_str(" · Wikimedia Commons");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn photo(author: &str, source: PlacePhotoSource) -> PlacePhoto {
        PlacePhoto {
            thumb: format!("https://thumb.wikimedia.org/{author}.jpg"),
            page: None,
            author: author.into(),
            license: Some("CC BY-SA 4.0".into()),
            source,
            year: Some(2020),
        }
    }

    #[test]
    fn switching_place_resets_gallery() {
        let mut state = PhotoGalleryState::default();
        state.for_place("mall/a");
        state.active = 2;
        state.failed.insert("x".into());
        state.for_place("mall/a");
        assert_eq!(state.active, 2, "same place keeps its state");
        state.for_place("park/b");
        assert_eq!(state.active, 0);
        assert!(state.failed.is_empty());
    }

    #[test]
    fn failed_photos_are_removed_and_active_is_clamped() {
        let photos = vec![
            photo("a", PlacePhotoSource::Commons),
            photo("b", PlacePhotoSource::Commons),
            photo("c", PlacePhotoSource::Commons),
        ];
        let mut state = PhotoGalleryState::default();
        state.active = 2;
        assert_eq!(active_index(&state, 3), 2);
        state.mark_failed(&photos[1].thumb);
        let usable = usable(&photos, &state);
        assert_eq!(usable.len(), 2);
        assert_eq!(active_index(&state, usable.len()), 1, "clamped to last photo");
        assert_eq!(usable[1].thumb, photos[2].thumb);
    }

    #[test]
    fn attribution_matches_both_sources() {
        let commons = photo("Terence Ong", PlacePhotoSource::Commons);
        assert_eq!(
            caption(&commons),
            "Photo: Terence Ong · 2020 · CC BY-SA 4.0 · Wikimedia Commons"
        );
        let nea = photo("NEA", PlacePhotoSource::Nea);
        assert_eq!(caption(&nea), "Photo: NEA (National Environment Agency)");
    }
}
