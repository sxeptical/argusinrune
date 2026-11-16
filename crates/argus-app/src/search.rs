//! Map search index and filtering, built on `argus-core::search`.

use argus_core::geo::within_radius;
use argus_core::parks::build_park_search_results;
use argus_core::search::{
    build_bus_stop_search_results, build_mrt_station_search_results, rank_results,
};
use argus_core::types::SearchResultType;

use crate::state::{AppState, Selection};

/// Build the (radius-scoped) search index from static bundles plus live bus stops.
pub fn build_results(s: &AppState) -> Vec<argus_core::types::MapSearchResult> {
    let filter = s.geo_filter();
    let mut results = Vec::new();
    if let Some(registry) = s.registry.as_ref() {
        results.extend(registry.search_results.iter().cloned());
    }
    if let Some(data) = s.static_data.as_ref() {
        results.extend(build_park_search_results(&data.parks_geojson));
        results.extend(build_mrt_station_search_results(&data.mrt_lines));
    }
    let stops = build_bus_stop_search_results(&s.data.snapshot().bus_stops);
    results.extend(stops);

    within_radius(results, filter.as_ref(), |r| [r.lng, r.lat])
}

/// Results shown in the search dialog for the current query.
pub fn visible<'a>(s: &'a AppState, query: &str) -> Vec<&'a argus_core::types::MapSearchResult> {
    let ranked = rank_results(&s.search_results, query);
    if query.trim().is_empty() {
        ranked.into_iter().take(8).collect()
    } else {
        ranked.into_iter().take(24).collect()
    }
}

/// Open a search hit: amenities and parks get a focus dialog, MRT stations and
/// bus stops also move the camera.
pub fn selection_for(result: &argus_core::types::MapSearchResult) -> Option<Selection> {
    match &result.result_type {
        SearchResultType::Amenity(kind) => Some(Selection::Amenity {
            kind: *kind,
            id: result.id.clone(),
        }),
        SearchResultType::Park => Some(Selection::Park {
            name: result.name.clone(),
            kind: argus_core::types::ParkFeatureKind::Park,
        }),
        SearchResultType::BusStop => Some(Selection::BusStop(result.id.clone())),
        SearchResultType::Mrt => Some(Selection::MrtStation(result.name.clone())),
    }
}
