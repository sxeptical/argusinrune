//! Search result builders and ranking over POIs.
//! Ported from `lib/map-search.ts` plus the ordering used by MapSearch.

use std::collections::BTreeMap;

use crate::geometry::build_mrt_station_geojson;
use crate::types::{BusStop, MapSearchResult, MrtGeoJson, SearchResultType};

/// `buildBusStopSearchResults` from lib/map-search.ts.
pub fn build_bus_stop_search_results(stops: &[BusStop]) -> Vec<MapSearchResult> {
    stops
        .iter()
        .map(|stop| MapSearchResult {
            id: stop.bus_stop_code.clone(),
            result_type: SearchResultType::BusStop,
            name: stop.description.clone(),
            lat: stop.latitude,
            lng: stop.longitude,
            zoom: 16.0,
            keywords: vec![stop.bus_stop_code.clone(), stop.road_name.clone()],
        })
        .collect()
}

/// MRT station search results: one entry per station, interchange lines merged
/// into the keywords. Requires the loaded MRT line GeoJSON.
pub fn build_mrt_station_search_results(mrt_lines: &MrtGeoJson) -> Vec<MapSearchResult> {
    let mut by_name: BTreeMap<String, MapSearchResult> = BTreeMap::new();
    for station in build_mrt_station_geojson(mrt_lines) {
        if let Some(existing) = by_name.get_mut(&station.properties.name) {
            existing.keywords.push(station.properties.line.clone());
            continue;
        }
        by_name.insert(
            station.properties.name.clone(),
            MapSearchResult {
                id: station.properties.name.clone(),
                result_type: SearchResultType::Mrt,
                name: station.properties.name.clone(),
                lat: station.geometry.coordinates[1],
                lng: station.geometry.coordinates[0],
                zoom: 15.0,
                keywords: vec!["mrt".into(), station.properties.line.clone()],
            },
        );
    }
    by_name.into_values().collect()
}

/// Match a search query against a result's name/keywords (case-insensitive
/// substring), used by the MapSearch panel to rank and filter results.
pub fn result_matches(result: &MapSearchResult, query: &str) -> bool {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return true;
    }
    result.name.to_lowercase().contains(&needle)
        || result
            .keywords
            .iter()
            .any(|keyword| keyword.to_lowercase().contains(&needle))
}

/// Rank results: name prefix > name substring > keyword match, then by name.
pub fn rank_results<'a>(results: &'a [MapSearchResult], query: &str) -> Vec<&'a MapSearchResult> {
    let needle = query.trim().to_lowercase();
    let mut scored: Vec<(&MapSearchResult, u8)> = results
        .iter()
        .filter(|r| result_matches(r, &needle))
        .map(|r| {
            let name = r.name.to_lowercase();
            let score = if name.starts_with(&needle) {
                0
            } else if name.contains(&needle) {
                1
            } else {
                2
            };
            (r, score)
        })
        .collect();
    scored.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.name.cmp(&b.0.name)));
    scored.into_iter().map(|(r, _)| r).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bus_stop_results_and_ranking() {
        let stops = vec![BusStop {
            bus_stop_code: "01012".into(),
            road_name: "Victoria St".into(),
            description: "Bugis Stn".into(),
            latitude: 1.3,
            longitude: 103.85,
        }];
        let results = build_bus_stop_search_results(&stops);
        assert_eq!(results[0].id, "01012");
        assert_eq!(results[0].keywords, ["01012", "Victoria St"]);
        assert!(result_matches(&results[0], "bugis"));
        assert!(!result_matches(&results[0], "nope"));
        assert_eq!(rank_results(&results, "bugis").len(), 1);
    }
}
