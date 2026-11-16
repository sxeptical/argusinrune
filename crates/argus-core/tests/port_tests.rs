//! Integration tests that exercise the pure logic against the real
//! `data/*.json` bundles (bundles, per-amenity bundle validation, mall stores,
//! parks, map geometry/features, MRT routing).

use argus_core::amenity_layers::AmenityRegistry;
use argus_core::data::{DataDir, StaticData, photos_for_park};
use argus_core::format::parse_iso;
use argus_core::geometry::{
    build_mrt_station_geojson, build_route_geojson, display_line_stations_map, route_station_keys,
};
use argus_core::mrt_routing::plan_mrt_route;
use argus_core::types::AmenityType;
use argus_core::types::{ParkFeatureKind, SchoolLevel};

fn data() -> StaticData {
    StaticData::load(&DataDir::default()).expect("data bundles must load")
}

// (bundle name, expected minimum rows, require unique names, latitudes, longitudes)
type BundleGroup<'a> = (&'a str, usize, bool, Vec<f64>, Vec<f64>);

#[test]
fn bundles_are_large_unique_and_in_singapore() {
    let d = data();
    let groups: Vec<BundleGroup<'_>> = vec![
        (
            "hawkers",
            d.hawker_centres.len(),
            false,
            d.hawker_centres.iter().map(|p| p.lat).collect(),
            d.hawker_centres.iter().map(|p| p.lng).collect(),
        ),
        (
            "malls",
            d.malls.len(),
            false,
            d.malls.iter().map(|p| p.lat).collect(),
            d.malls.iter().map(|p| p.lng).collect(),
        ),
        (
            "libraries",
            d.libraries.len(),
            true,
            d.libraries.iter().map(|p| p.lat).collect(),
            d.libraries.iter().map(|p| p.lng).collect(),
        ),
        (
            "supermarkets",
            d.supermarkets.len(),
            false,
            d.supermarkets.iter().map(|p| p.lat).collect(),
            d.supermarkets.iter().map(|p| p.lng).collect(),
        ),
        (
            "heritage",
            d.heritage_sites.len(),
            false,
            d.heritage_sites.iter().map(|p| p.lat).collect(),
            d.heritage_sites.iter().map(|p| p.lng).collect(),
        ),
        (
            "hotels",
            d.hotels.len(),
            false,
            d.hotels.iter().map(|p| p.lat).collect(),
            d.hotels.iter().map(|p| p.lng).collect(),
        ),
        (
            "community-clubs",
            d.community_clubs.len(),
            false,
            d.community_clubs.iter().map(|p| p.lat).collect(),
            d.community_clubs.iter().map(|p| p.lng).collect(),
        ),
        (
            "pharmacies",
            d.pharmacies.len(),
            false,
            d.pharmacies.iter().map(|p| p.lat).collect(),
            d.pharmacies.iter().map(|p| p.lng).collect(),
        ),
        (
            "gyms",
            d.gyms.len(),
            false,
            d.gyms.iter().map(|p| p.lat).collect(),
            d.gyms.iter().map(|p| p.lng).collect(),
        ),
        (
            "schools",
            d.schools.len(),
            true,
            d.schools.iter().map(|p| p.lat).collect(),
            d.schools.iter().map(|p| p.lng).collect(),
        ),
        (
            "healthcare",
            d.healthcare_sites.len(),
            false,
            d.healthcare_sites.iter().map(|p| p.lat).collect(),
            d.healthcare_sites.iter().map(|p| p.lng).collect(),
        ),
    ];
    let mins = [100, 150, 28, 540, 240, 400, 120, 200, 300, 320, 1400];
    for (i, (name, count, at_least, lats, lngs)) in groups.iter().enumerate() {
        assert!(
            if *at_least {
                *count >= mins[i]
            } else {
                *count > mins[i]
            },
            "{} too small: {}",
            name,
            count
        );
        for lat in lats {
            assert!((1.1..=1.5).contains(lat), "{} lat {}", name, lat);
        }
        for lng in lngs {
            assert!((103.5..=104.1).contains(lng), "{} lng {}", name, lng);
        }
    }
}

#[test]
fn dataset_kind_coverage() {
    let d = data();
    assert!(
        d.gyms
            .iter()
            .any(|g| g.source == argus_core::types::GymSource::Alltheplaces)
    );
    assert!(
        d.gyms
            .iter()
            .any(|g| g.source == argus_core::types::GymSource::Osm)
    );
    assert!(
        d.gyms
            .iter()
            .any(|g| g.source == argus_core::types::GymSource::Sportsg)
    );
    assert!(d.gyms.iter().any(|g| g.brand == "ActiveSG"));
    assert_eq!(
        d.schools
            .iter()
            .filter(|s| s.level == SchoolLevel::Polytechnic)
            .count(),
        5
    );
    assert!(
        d.schools
            .iter()
            .filter(|s| s.level == SchoolLevel::University)
            .count()
            >= 6
    );
    assert!(
        d.heritage_sites
            .iter()
            .any(|s| s.kind == argus_core::types::HeritageKind::Museum
                && s.source == argus_core::types::HeritageSource::Osm)
    );
    assert!(
        d.healthcare_sites
            .iter()
            .any(|s| s.kind == argus_core::types::HealthcareKind::Hospital)
    );
    assert!(
        d.libraries
            .iter()
            .any(|l| l.source == argus_core::types::LibrarySource::Osm)
    );
    assert!(d.supermarkets.iter().any(|m| m.brand == "Other"));
    assert!(
        d.community_clubs
            .iter()
            .any(|c| c.kind == argus_core::types::CommunityClubKind::Passionwave)
    );
}

#[test]
fn amenity_photos_and_closure_join() {
    let d = data();
    assert!(d.malls.iter().filter(|m| !m.photos.is_empty()).count() > 40);
    assert!(
        d.hawker_centres
            .iter()
            .filter(|h| !h.photos.is_empty())
            .count()
            > 90
    );
    let with_closures = d
        .hawker_centres
        .iter()
        .filter(|h| !h.closures.is_empty())
        .count();
    assert!(with_closures > 100);
    assert!(
        d.hawker_centres
            .iter()
            .filter(|h| h.closures.is_empty())
            .count()
            > 0
    );
    let total: usize = d.hawker_centres.iter().map(|h| h.closures.len()).sum();
    assert!(total > 400);
    for hawker in &d.hawker_centres {
        for closure in &hawker.closures {
            assert!(closure.end >= closure.start);
            assert_eq!(closure.start.len(), 10);
        }
    }
}

#[test]
fn hawker_centre_popups_include_closure() {
    use argus_core::amenities::describe_hawker_centre;
    let d = data();
    let h = d
        .hawker_centres
        .iter()
        .find(|h| !h.closures.is_empty())
        .unwrap();
    let lines = describe_hawker_centre(h, "2026-12-13");
    assert_eq!(lines[0].text, h.name);
    assert_eq!(lines[1].text, "In operation");
}

#[test]
fn mall_store_index_is_valid() {
    let d = data();
    let index = &d.mall_store_index;
    assert!(index.malls.len() > 40);
    let mall_ids: std::collections::BTreeSet<_> = d.malls.iter().map(|m| m.id.as_str()).collect();
    for id in index.malls.keys() {
        assert!(mall_ids.contains(id.as_str()), "unknown mall {}", id);
        let stores = StaticData::load_mall_store_list(&DataDir::default(), id)
            .unwrap_or_else(|e| panic!("{}: {}", id, e));
        assert_eq!(stores.len() as i64, index.malls[id].stores);
    }
    assert_eq!(StaticData::mall_stores_url(index, "not-a-mall"), None);
}

#[test]
fn parks_geometry_and_photos() {
    let d = data();
    let parks = &d.parks_geojson;
    assert!(
        parks
            .features
            .iter()
            .filter(|f| f.properties.kind == ParkFeatureKind::Park)
            .count()
            > 100
    );
    assert!(
        parks
            .features
            .iter()
            .filter(|f| f.properties.kind != ParkFeatureKind::Park)
            .count()
            > 100
    );
    let reserve = parks
        .features
        .iter()
        .find(|f| f.properties.name == "Central Catchment Nature Reserve")
        .unwrap();
    assert!(reserve.properties.area_ha.unwrap_or(0.0) > 2000.0);
    assert!(d.park_photos.len() > 50);
    let park_names: std::collections::BTreeSet<_> = parks
        .features
        .iter()
        .filter(|f| f.properties.kind == ParkFeatureKind::Park)
        .map(|f| f.properties.name.as_str())
        .collect();
    for name in d.park_photos.keys() {
        assert!(
            park_names.contains(name.as_str()),
            "photo key {} is not a park",
            name
        );
    }
    assert!(!photos_for_park(&d, "Central Catchment Nature Reserve").is_empty());
    let a = photos_for_park(&d, "East Coast Park Area C");
    let b = photos_for_park(&d, "East Coast Park Area D");
    assert_eq!(
        a.iter().map(|p| &p.thumb).collect::<Vec<_>>(),
        b.iter().map(|p| &p.thumb).collect::<Vec<_>>()
    );
    for bad in [
        "ChineseGardenSingapore-bridge-pagoda",
        "Former_Merlion_Park",
        "Marsiling_Park_Connector",
    ] {
        assert!(
            !d.park_photos
                .values()
                .flatten()
                .any(|p| p.page.as_deref().is_some_and(|page| page.contains(bad)))
        );
    }
}

#[test]
fn map_geometry_loop_route_and_station_keys() {
    let d = data();
    let mrt_lines = &d.mrt_lines;
    let segments = vec![argus_core::mrt_routing::MrtRouteSegment {
        line: "Sengkang LRT East Loop".into(),
        from: "Ranggung".into(),
        to: "Compassvale".into(),
        stops: 2,
        stations: vec!["Ranggung".into(), "Sengkang".into(), "Compassvale".into()],
    }];
    let route = build_route_geojson(mrt_lines, &segments, &display_line_stations_map());
    assert_eq!(route.features.len(), 1);
    let coords = &route.features[0].geometry.coordinates;
    let line = mrt_lines
        .features
        .iter()
        .find(|f| f.properties.name == "Sengkang LRT East Loop")
        .unwrap();
    assert_eq!(
        *coords,
        vec![
            line.geometry.coordinates[5],
            line.geometry.coordinates[6],
            line.geometry.coordinates[1]
        ]
    );

    let plan = plan_mrt_route("Newton", "Bedok").unwrap();
    let keys = route_station_keys(&plan.segments);
    let stations = build_mrt_station_geojson(mrt_lines);
    let station_key = |name: &str, line: &str| format!("{}|{}", line, name);
    for key in &keys {
        let matched = stations
            .iter()
            .any(|f| station_key(&f.properties.name, &f.properties.line) == *key);
        assert!(matched, "station key {} has no feature", key);
    }
    let newton_at_line = stations
        .iter()
        .filter(|f| f.properties.name == "Newton")
        .filter(|f| keys.contains(&station_key("Newton", &f.properties.line)))
        .count();
    assert_eq!(newton_at_line, 1);

    let mut labels = std::collections::BTreeSet::new();
    for feature in &stations {
        if feature.properties.label.is_empty() {
            continue;
        }
        assert!(labels.insert(feature.properties.name.to_lowercase()));
    }
}

#[test]
fn amenity_registry_invariants() {
    let d = data();
    let registry = AmenityRegistry::new(&d);
    assert_eq!(registry.layers.len(), 11);
    assert_eq!(
        registry.layers.last().unwrap().amenity_type,
        AmenityType::Healthcare
    );
    let total: usize = registry.layers.iter().map(|l| l.places.len()).sum();
    assert_eq!(registry.search_results.len(), total);
    assert!(
        registry
            .search_results
            .iter()
            .all(|r| (r.zoom - 16.0).abs() < f64::EPSILON)
    );
    for layer in &registry.layers {
        assert_eq!(layer.by_id.len(), layer.places.len());
        if let Some(filter) = &layer.filter {
            let sum: i64 = filter.choices.iter().map(|c| c.count).sum();
            assert_eq!(
                sum,
                layer.places.len() as i64,
                "{} choices don't sum",
                layer.amenity_type.as_str()
            );
        }
    }
    let gym = registry.layer(AmenityType::Gym).unwrap();
    assert_eq!(gym.filter.as_ref().unwrap().choices[0].value, "ActiveSG");
    let supermarket = registry.layer(AmenityType::Supermarket).unwrap();
    assert_eq!(
        supermarket
            .filter
            .as_ref()
            .unwrap()
            .choices
            .last()
            .unwrap()
            .value,
        "Other"
    );
    assert!(!registry.display_coords.is_empty());
    for key in registry.display_coords.keys() {
        let (kind, id) = key.split_once(':').unwrap();
        let kind = AmenityType::parse(kind).unwrap();
        assert!(registry.find(kind, id).is_some());
        assert!(
            !registry.co_located(kind, id, |_, _| true).is_empty(),
            "{} has no peers",
            key
        );
    }
    let mall = &registry.layer(AmenityType::Mall).unwrap().places[0];
    assert_eq!(
        registry
            .find(AmenityType::Mall, mall.id())
            .map(|(_, p)| p.name()),
        Some(mall.name())
    );
    assert!(registry.find(AmenityType::Gym, mall.id()).is_none());
}

#[test]
fn parses_iso_timestamps() {
    assert!(parse_iso("2026-11-02T08:00:00Z").is_some());
    assert!(parse_iso("2026-11-02T08:00:00+08:00").is_some());
    assert!(parse_iso("garbage").is_none());
}
