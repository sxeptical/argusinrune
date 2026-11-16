//! Static data loaders for `public/*.json`.
//!
//! Data-loading approach: paths are resolved relative to a **configurable data
//! directory** (`DataDir`), never baked in with `include_bytes!`. `DataDir::default()`
//! reads `ARGUS_DATA_DIR` when set, otherwise `CARGO_MANIFEST_DIR/../../public`
//! (the checked-in Next.js `public/` tree). Callers that ship a different bundle
//! can point at their own directory. Every bundle is validated on load, mirroring
//! the TypeScript `validate*Bundle` functions, then deserialised into the typed
//! domain models from `crate::types`.

use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::bundle_validation::{
    is_addressed_place, is_nullable_http_url, is_nullable_non_negative_integer,
    is_nullable_string, is_photo_list,
};
use crate::parks::{area_hectares, format_park_name, park_label_point};
use crate::types::*;

#[derive(Debug)]
pub enum DataError {
    Io(String),
    Json(String),
    Validation(String),
}

impl std::fmt::Display for DataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataError::Io(s) => write!(f, "I/O error: {}", s),
            DataError::Json(s) => write!(f, "JSON error: {}", s),
            DataError::Validation(s) => write!(f, "invalid bundle: {}", s),
        }
    }
}
impl std::error::Error for DataError {}

/// A configurable directory that holds the static JSON bundles.
#[derive(Clone, Debug)]
pub struct DataDir {
    root: PathBuf,
}

impl DataDir {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { root: path.into() }
    }

    /// `ARGUS_DATA_DIR` when set, else `<crate>/../../public`.
    pub fn default() -> Self {
        if let Ok(dir) = std::env::var("ARGUS_DATA_DIR") {
            return Self::new(dir);
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("public");
        Self::new(root)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn read_value(&self, relative: &str) -> Result<Value, DataError> {
        let path = self.root.join(relative);
        let text = std::fs::read_to_string(&path)
            .map_err(|e| DataError::Io(format!("{}: {}", path.display(), e)))?;
        serde_json::from_str(&text).map_err(|e| DataError::Json(format!("{}: {}", path.display(), e)))
    }

    pub fn load<T: DeserializeOwned>(&self, relative: &str) -> Result<T, DataError> {
        let value = self.read_value(relative)?;
        serde_json::from_value(value).map_err(|e| DataError::Json(e.to_string()))
    }
}

// ---------------------------------------------------------------------------
// Bundle validators (mirror lib/data/* + lib/park-photos.ts)
// ---------------------------------------------------------------------------

pub fn validate_amenities_bundle(value: &Value) -> bool {
    let Some(obj) = value.as_object() else { return false };
    let (Some(hawkers), Some(malls)) = (obj.get("hawkers"), obj.get("malls")) else {
        return false;
    };
    let is_mall = |v: &Value| -> bool {
        let o = match v.as_object() { Some(o) => o, None => return false };
        is_addressed_place(v)
            && o.get("openingHours").map_or(false, is_nullable_string)
            && o.get("stores").map_or(false, is_nullable_non_negative_integer)
            && o.get("fnb").map_or(false, is_nullable_non_negative_integer)
            && o.get("nearestMrt").map_or(false, is_nullable_string)
            && o.get("photos").map_or(false, is_photo_list)
    };
    let is_hawker = |v: &Value| -> bool {
        let o = match v.as_object() { Some(o) => o, None => return false };
        is_addressed_place(v)
            && o.get("stalls").map_or(false, is_nullable_non_negative_integer)
            && o.get("status").and_then(Value::as_str).map_or(false, |s| s == "open" || s == "planned")
            && o.get("expectedYear").map_or(false, is_nullable_non_negative_integer)
            && o.get("photos").map_or(false, is_photo_list)
    };
    hawkers.as_array().map_or(false, |a| a.iter().all(is_hawker))
        && malls.as_array().map_or(false, |a| a.iter().all(is_mall))
}

fn is_iso_date(s: &str) -> bool {
    s.len() == 10
        && s.as_bytes().get(4) == Some(&b'-')
        && s.as_bytes().get(7) == Some(&b'-')
        && s[0..4].bytes().all(|b| b.is_ascii_digit())
        && s[5..7].bytes().all(|b| b.is_ascii_digit())
        && s[8..10].bytes().all(|b| b.is_ascii_digit())
}

fn is_closure(v: &Value) -> bool {
    let o = match v.as_object() { Some(o) => o, None => return false };
    let kind_ok = o.get("kind").and_then(Value::as_str).map_or(false, |k| k == "cleaning" || k == "other-works");
    let start = o.get("start").and_then(Value::as_str);
    let end = o.get("end").and_then(Value::as_str);
    let dates_ok = match (start, end) {
        (Some(s), Some(e)) => is_iso_date(s) && is_iso_date(e) && e >= s,
        _ => false,
    };
    let remarks_ok = o.get("remarks").map_or(false, |r| r.is_null() || r.is_string());
    kind_ok && dates_ok && remarks_ok
}

pub fn validate_hawker_closures_bundle(value: &Value) -> bool {
    let Some(obj) = value.as_object() else { return false };
    obj.values().all(|periods| {
        periods.as_array().map_or(false, |arr| arr.iter().all(is_closure))
    })
}

pub fn validate_mall_store_index(value: &Value) -> bool {
    let obj = match value.as_object() { Some(o) => o, None => return false };
    let fetched_ok = obj.get("fetchedAt").and_then(Value::as_str).is_some();
    let Some(malls) = obj.get("malls").and_then(Value::as_object) else {
        return false;
    };
    fetched_ok
        && malls.iter().all(|(id, entry)| {
            let id_ok = !id.is_empty()
                && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
            let entry = match entry.as_object() { Some(o) => o, None => return false };
            let stores_ok = entry.get("stores").and_then(Value::as_i64).map_or(false, |n| n > 0);
            let list_ok = entry.get("listName").and_then(Value::as_str).is_some();
            id_ok && stores_ok && list_ok
        })
}

pub fn validate_park_photos(value: &Value) -> bool {
    let Some(obj) = value.as_object() else { return false };
    obj.values().all(is_photo_list)
}

pub fn validate_parks_geojson(value: &Value) -> bool {
    let obj = match value.as_object() { Some(o) => o, None => return false };
    if obj.get("type").and_then(Value::as_str) != Some("FeatureCollection") {
        return false;
    }
    let Some(features) = obj.get("features").and_then(Value::as_array) else {
        return false;
    };
    features.iter().all(|feature| {
        let props = feature.get("properties");
        let kind_ok = props
            .and_then(|p| p.get("kind"))
            .and_then(Value::as_str)
            .map_or(false, |k| k == "park" || k == "pcn" || k == "trail");
        let name_ok = props
            .and_then(|p| p.get("name"))
            .and_then(Value::as_str)
            .map_or(false, |s| !s.is_empty());
        kind_ok && name_ok
    })
}

/// Generic array-of-places validator parameterised by the per-record predicate.
fn validate_place_array(value: &Value, pred: impl Fn(&Value) -> bool) -> bool {
    value.as_array().map_or(false, |arr| arr.iter().all(pred))
}

pub fn validate_community_clubs_bundle(value: &Value) -> bool {
    validate_place_array(value, |v| {
        is_addressed_place(v)
            && v.get("kind").and_then(Value::as_str).map_or(false, |k| k == "community-club" || k == "passionwave")
            && v.get("photos").map_or(false, is_photo_list)
    })
}

pub fn validate_gyms_bundle(value: &Value) -> bool {
    validate_place_array(value, |v| {
        is_addressed_place(v)
            && v.get("brand").and_then(Value::as_str).map_or(false, |b| GYM_BRANDS.contains(&b))
            && v.get("category").and_then(Value::as_str).map_or(false, |c| GYM_CATEGORIES.contains(&c))
            && v.get("openingHours").map_or(false, is_nullable_string)
            && v.get("website").map_or(false, is_nullable_http_url)
            && v.get("aliases").map_or(true, |a| {
                a.is_array() && a.as_array().unwrap().iter().all(Value::is_string)
            })
            && v.get("source").and_then(Value::as_str).map_or(false, |s| GYM_SOURCES.contains(&s))
    })
}

pub fn validate_healthcare_bundle(value: &Value) -> bool {
    validate_place_array(value, |v| {
        is_addressed_place(v)
            && v.get("kind").and_then(Value::as_str).map_or(false, |k| HEALTHCARE_KINDS.contains(&k))
            && v.get("phone").map_or(false, is_nullable_string)
            && v.get("programmes").and_then(Value::as_array).map_or(false, |a| a.iter().all(Value::is_string))
            && v.get("photos").map_or(false, is_photo_list)
            && v.get("source").and_then(Value::as_str).map_or(false, |s| s == "chas" || s == "osm")
    })
}

pub fn validate_heritage_bundle(value: &Value) -> bool {
    validate_place_array(value, |v| {
        is_addressed_place(v)
            && v.get("kind").and_then(Value::as_str).map_or(false, |k| HERITAGE_KINDS.contains(&k))
            && v.get("description").map_or(false, is_nullable_string)
            && v.get("website").map_or(false, is_nullable_http_url)
            && v.get("photos").map_or(false, is_photo_list)
            && v.get("source").and_then(Value::as_str).map_or(false, |s| s == "nhb" || s == "osm")
    })
}

pub fn validate_hotels_bundle(value: &Value) -> bool {
    validate_place_array(value, |v| {
        is_addressed_place(v)
            && {
                match v.get("rooms") {
                    Some(r) => r.as_i64().map_or(false, |n| n > 0),
                    None => false,
                }
            }
            && v.get("photos").map_or(false, is_photo_list)
    })
}

pub fn validate_libraries_bundle(value: &Value) -> bool {
    validate_place_array(value, |v| {
        is_addressed_place(v)
            && v.get("branch").map_or(false, is_nullable_string)
            && v.get("photos").map_or(false, is_photo_list)
            && v.get("source").and_then(Value::as_str).map_or(false, |s| s == "nlb" || s == "osm")
    })
}

pub fn validate_pharmacies_bundle(value: &Value) -> bool {
    validate_place_array(value, is_addressed_place)
}

pub fn validate_schools_bundle(value: &Value) -> bool {
    validate_place_array(value, |v| {
        is_addressed_place(v)
            && v.get("level").and_then(Value::as_str).map_or(false, |l| SCHOOL_LEVELS.contains(&l))
            && v.get("postalCode").map_or(false, is_nullable_string)
            && v.get("phone").map_or(false, is_nullable_string)
            && v.get("website").map_or(false, is_nullable_http_url)
            && v.get("mrt").map_or(false, is_nullable_string)
            && v.get("bus").map_or(false, is_nullable_string)
            && v.get("coordsSource").and_then(Value::as_str).map_or(false, |s| s == "onemap" || s == "osm")
    })
}

pub fn validate_supermarkets_bundle(value: &Value) -> bool {
    validate_place_array(value, |v| {
        is_addressed_place(v)
            && v.get("brand").and_then(Value::as_str).map_or(false, |b| SUPERMARKET_BRANDS.contains(&b))
            && v.get("openingHours").map_or(false, is_nullable_string)
            && v.get("website").map_or(false, is_nullable_http_url)
    })
}

pub fn is_mall_store_list(value: &Value) -> bool {
    validate_place_array(value, |v| {
        let o = match v.as_object() { Some(o) => o, None => return false };
        let name_ok = o.get("name").and_then(Value::as_str).map_or(false, |s| !s.is_empty());
        let nullable = |k: &str| o.get(k).map_or(false, is_nullable_string);
        let bool_field = |k: &str| o.get(k).and_then(Value::as_bool).is_some();
        let website_ok = o.get("website").map_or(false, |w| {
            w.is_null() || w.as_str().map_or(false, |s| {
                let lower = s.to_ascii_lowercase();
                lower.starts_with("http://") || lower.starts_with("https://")
            })
        });
        name_ok && nullable("unit") && nullable("level") && nullable("category")
            && bool_field("fnb") && bool_field("halal") && website_ok
    })
}

// ---------------------------------------------------------------------------
// Loaders
// ---------------------------------------------------------------------------

fn load_validated<T: DeserializeOwned>(
    dir: &DataDir,
    relative: &str,
    validator: impl Fn(&Value) -> bool,
) -> Result<T, DataError> {
    let value = dir.read_value(relative)?;
    if !validator(&value) {
        return Err(DataError::Validation(format!("{} failed validation", relative)));
    }
    serde_json::from_value(value).map_err(|e| DataError::Json(format!("{}: {}", relative, e)))
}

/// Everything the dashboard needs from `public/`, loaded and pre-processed.
#[derive(Clone, Debug)]
pub struct StaticData {
    pub malls: Vec<Mall>,
    pub hawker_centres: Vec<HawkerCentre>,
    pub community_clubs: Vec<CommunityClub>,
    pub gyms: Vec<Gym>,
    pub healthcare_sites: Vec<HealthcareSite>,
    pub heritage_sites: Vec<HeritageSite>,
    pub hotels: Vec<Hotel>,
    pub libraries: Vec<PublicLibrary>,
    pub pharmacies: Vec<Pharmacy>,
    pub schools: Vec<School>,
    pub supermarkets: Vec<Supermarket>,
    pub mrt_lines: MrtGeoJson,
    pub parks_geojson: ParksGeoJson,
    pub park_photos: std::collections::BTreeMap<String, Vec<PlacePhoto>>,
    pub mall_store_index: MallStoreIndex,
}

impl StaticData {
    /// Load a single bundle (e.g. `"gyms.json"`), validating it first.
    pub fn load_bundle<T: DeserializeOwned>(
        dir: &DataDir,
        relative: &str,
        validator: impl Fn(&Value) -> bool,
    ) -> Result<T, DataError> {
        load_validated(dir, relative, validator)
    }

    pub fn load_mall_store_list(dir: &DataDir, mall_id: &str) -> Result<Vec<MallStore>, DataError> {
        load_validated(dir, &format!("mall-stores/{}.json", mall_id), is_mall_store_list)
    }

    pub fn mall_stores_url(index: &MallStoreIndex, mall_id: &str) -> Option<String> {
        index.malls.contains_key(mall_id)
            .then(|| format!("/mall-stores/{}.json", mall_id))
    }

    pub fn load(dir: &DataDir) -> Result<Self, DataError> {
        let amenities: AmenitiesBundle = load_validated(dir, "amenities.json", validate_amenities_bundle)?;
        let closures: std::collections::BTreeMap<String, Vec<HawkerClosure>> =
            load_validated(dir, "hawker-closures.json", validate_hawker_closures_bundle)?;
        // Join closures by hawker id, and reject dangling references.
        let mut hawker_centres = amenities.hawkers;
        for hawker in &mut hawker_centres {
            hawker.closures = closures.get(&hawker.id).cloned().unwrap_or_default();
        }
        for id in closures.keys() {
            if !hawker_centres.iter().any(|h| h.id == *id) {
                return Err(DataError::Validation(format!("hawker closures reference unknown centre {}", id)));
            }
        }

        let community_clubs = load_validated(dir, "community-clubs.json", validate_community_clubs_bundle)?;
        let gyms = load_validated(dir, "gyms.json", validate_gyms_bundle)?;
        let healthcare_sites = load_validated(dir, "healthcare.json", validate_healthcare_bundle)?;
        let heritage_sites = load_validated(dir, "heritage.json", validate_heritage_bundle)?;
        let hotels = load_validated(dir, "hotels.json", validate_hotels_bundle)?;
        let libraries = load_validated(dir, "libraries.json", validate_libraries_bundle)?;
        let pharmacies = load_validated(dir, "pharmacies.json", validate_pharmacies_bundle)?;
        let schools = load_validated(dir, "schools.json", validate_schools_bundle)?;
        let supermarkets = load_validated(dir, "supermarkets.json", validate_supermarkets_bundle)?;
        let mrt_lines: MrtGeoJson = dir.load("mrt-lines.json")?;
        let park_photos: std::collections::BTreeMap<String, Vec<PlacePhoto>> =
            load_validated(dir, "park-photos.json", validate_park_photos)?;
        let mall_store_index = load_validated(dir, "mall-stores/index.json", validate_mall_store_index)?;

        // NParks names are often all-caps abbreviations; expose readable names.
        let raw_parks: ParksGeoJson = load_validated(dir, "parks-geometry.json", validate_parks_geojson)?;
        let parks_geojson = ParksGeoJson {
            r#type: raw_parks.r#type,
            features: raw_parks
                .features
                .into_iter()
                .map(|mut feature| {
                    feature.properties.name = format_park_name(&feature.properties.name);
                    if let Some(park) = feature.properties.park.take() {
                        feature.properties.park = Some(format_park_name(&park));
                    }
                    if feature.properties.kind == ParkFeatureKind::Park {
                        feature.properties.area_ha = Some((area_hectares(&feature.geometry) * 10.0).round() / 10.0);
                    }
                    feature
                })
                .collect(),
        };

        Ok(Self {
            malls: amenities.malls,
            hawker_centres,
            community_clubs,
            gyms,
            healthcare_sites,
            heritage_sites,
            hotels,
            libraries,
            pharmacies,
            schools,
            supermarkets,
            mrt_lines,
            parks_geojson,
            park_photos,
            mall_store_index,
        })
    }
}

/// `photosForPark` from lib/park-photos.ts.
pub fn photos_for_park<'a>(data: &'a StaticData, name: &str) -> &'a [PlacePhoto] {
    data.park_photos.get(name).map(|v| v.as_slice()).unwrap_or(&[])
}

/// One label point per park (lib/data/parks.ts PARK_LABELS_GEOJSON).
pub fn park_labels(data: &StaticData) -> Vec<(String, f64, f64)> {
    data.parks_geojson
        .features
        .iter()
        .filter(|f| f.properties.kind == ParkFeatureKind::Park)
        .filter_map(|f| {
            park_label_point(&f.geometry).map(|p| {
                (f.properties.name.clone(), p[0], p[1])
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validators_accept_valid_and_reject_invalid() {
        let place = serde_json::json!({"id":"h1","name":"H","address":null,"lat":1.3,"lng":103.8,"stalls":10,"status":"open","expectedYear":null,"photos":[]});
        assert!(validate_amenities_bundle(&serde_json::json!({"hawkers":[place],"malls":[]})));
        assert!(!validate_amenities_bundle(&serde_json::json!({"hawkers":[{"status":"closed"}],"malls":[]})));
        assert!(validate_hawker_closures_bundle(&serde_json::json!({"h1":[{"kind":"cleaning","start":"2026-12-12","end":"2026-12-15","remarks":null}]})));
        assert!(!validate_hawker_closures_bundle(&serde_json::json!({"h1":[{"kind":"fire-drill","start":"2026-12-12","end":"2026-12-15","remarks":null}]})));
        assert!(validate_parks_geojson(&serde_json::json!({"type":"FeatureCollection","features":[{"properties":{"kind":"park","name":"X"}}]})));
    }

    #[test]
    fn loads_all_bundles_from_public() {
        let dir = DataDir::default();
        let data = StaticData::load(&dir).expect("public bundles should load and validate");
        assert!(data.hawker_centres.len() > 100);
        assert!(data.malls.len() > 150);
        assert!(data.gyms.len() > 300);
        assert!(data.schools.len() >= 320);
        assert!(data.healthcare_sites.len() > 1400);
        assert!(data.parks_geojson.features.iter().any(|f| f.properties.kind == ParkFeatureKind::Park));
        assert!(!park_labels(&data).is_empty());
    }
}
