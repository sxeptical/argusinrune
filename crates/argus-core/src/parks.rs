//! NParks recreation layer display helpers.
//! Ported from `lib/parks.ts`.

use crate::types::{
    AvailabilityTone, MapSearchResult, ParkFeatureKind, ParksGeoJson, PlacePhoto, PopupLine,
    SearchResultType,
};

type Ring = Vec<[f64; 2]>;

const ABBREVIATIONS: &[(&str, &str)] = &[
    ("PK", "Park"),
    ("PG", "Playground"),
    ("PC", "Park Connector"),
    ("RD", "Road"),
    ("AVE", "Avenue"),
    ("DR", "Drive"),
    ("JLN", "Jalan"),
    ("UPP", "Upper"),
    ("BTWN", "between"),
    ("GDNS", "Gardens"),
    ("NTH", "North"),
    ("STH", "South"),
    ("CTRL", "Central"),
];
const ACRONYMS: &[&str] = &[
    "AYE", "BKE", "CTE", "ECP", "KJE", "KPE", "MCE", "PIE", "SLE", "TPE", "MRT", "LRT", "PUB",
    "SBG", "JLG", "YCK", "HDB", "NS", "EW",
];
const SMALL_WORDS: &[&str] = &["TO", "AND", "OF", "THE", "AT", "NEAR"];

fn title_case(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
        None => String::new(),
    }
}

/// Expand NParks abbreviations and title-case all-caps names.
pub fn format_park_name(raw: &str) -> String {
    let shouting = raw.to_uppercase() == raw;
    let mut depth = 0i32;
    let mut out = String::new();
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c == '(' || c == ')' {
            if c == '(' {
                depth += 1;
            } else {
                depth = (depth - 1).max(0);
            }
            out.push(c);
            i += 1;
            continue;
        }
        if c.is_ascii_alphabetic() {
            let start = i;
            while i < bytes.len() && (bytes[i] as char).is_ascii_alphabetic() {
                i += 1;
            }
            let token = &raw[start..i];
            let upper = token.to_uppercase();
            if let Some((_, expanded)) = ABBREVIATIONS.iter().find(|(k, _)| *k == upper) {
                out.push_str(expanded);
                continue;
            }
            if !shouting {
                out.push_str(token);
                continue;
            }
            if start > 0 && SMALL_WORDS.contains(&upper.as_str()) {
                out.push_str(&token.to_lowercase());
                continue;
            }
            let has_vowel = upper
                .chars()
                .any(|c| matches!(c, 'A' | 'E' | 'I' | 'O' | 'U'));
            if ACRONYMS.contains(&upper.as_str()) || (depth > 0 && token.len() <= 3 && !has_vowel) {
                out.push_str(&upper);
            } else {
                out.push_str(&title_case(token));
            }
            continue;
        }
        out.push(c);
        i += 1;
    }
    // Replace 'S with 's (matching /'S\b/g), then trim.
    out.replace("'S", "'s").trim().to_string()
}

const METRES_PER_DEGREE_LAT: f64 = 110_574.0;
/// 111_320 * cos(1.35°) — precomputed because `f64::cos` is not const.
const METRES_PER_DEGREE_LNG: f64 = 111_289.100_931_867_83;

fn ring_area(ring: &Ring) -> f64 {
    let n = ring.len();
    if n == 0 {
        return 0.0;
    }
    let mut sum = 0.0;
    for i in 0..n {
        let j = (i + n - 1) % n;
        sum += (ring[j][0] * METRES_PER_DEGREE_LNG) * (ring[i][1] * METRES_PER_DEGREE_LAT)
            - (ring[i][0] * METRES_PER_DEGREE_LNG) * (ring[j][1] * METRES_PER_DEGREE_LAT);
    }
    (sum / 2.0).abs()
}

fn polygon_area(rings: &[Ring]) -> f64 {
    if rings.is_empty() {
        return 0.0;
    }
    ring_area(&rings[0]) - rings[1..].iter().map(ring_area).sum::<f64>()
}

fn rings_from_value(value: &serde_json::Value) -> Vec<Ring> {
    value
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|r| {
                    r.as_array().map(|points| {
                        points
                            .iter()
                            .filter_map(|p| {
                                let arr = p.as_array()?;
                                Some([arr.first()?.as_f64()?, arr.get(1)?.as_f64()?])
                            })
                            .collect()
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Area in hectares of a Polygon / MultiPolygon geometry; 0 for lines.
pub fn area_hectares(geometry: &crate::types::ParkGeometry) -> f64 {
    let square = match geometry.r#type.as_str() {
        "Polygon" => polygon_area(&rings_from_value(&geometry.coordinates)),
        "MultiPolygon" => geometry
            .coordinates
            .as_array()
            .map(|polys| {
                polys
                    .iter()
                    .map(|p| polygon_area(&rings_from_value(p)))
                    .sum()
            })
            .unwrap_or(0.0),
        _ => 0.0,
    };
    square / 10_000.0
}

fn format_hectares(hectares: f64) -> String {
    if hectares >= 10.0 {
        format!(
            "{} ha",
            crate::format::group_thousands(hectares.round() as i64)
        )
    } else {
        format!("{:.1} ha", hectares)
    }
}

pub fn park_kind_title(kind: ParkFeatureKind) -> &'static str {
    match kind {
        ParkFeatureKind::Park => "PARK",
        ParkFeatureKind::Pcn => "PARK CONNECTOR",
        ParkFeatureKind::Trail => "TRAIL",
    }
}

#[derive(Clone, Debug, Default)]
pub struct ParkFeatureDetails<'a> {
    pub kind: ParkFeatureKind,
    pub name: String,
    pub park: Option<String>,
    pub kind_type: Option<String>,
    pub cycle: Option<bool>,
    pub area_ha: Option<f64>,
    pub photos: &'a [PlacePhoto],
}

pub fn describe_park_feature(feature: &ParkFeatureDetails<'_>) -> Vec<PopupLine> {
    let mut lines = vec![PopupLine::new(&feature.name, "name")];
    match feature.kind {
        ParkFeatureKind::Park => {
            lines.push(PopupLine::toned(
                "Public park or nature area",
                "status",
                AvailabilityTone::Success,
            ));
            if let Some(area) = feature.area_ha
                && area != 0.0
            {
                lines.push(PopupLine::new(
                    format!("About {}", format_hectares(area)),
                    "count",
                ));
            }
        }
        ParkFeatureKind::Pcn => {
            lines.push(PopupLine::toned(
                "Walking, running and cycling path",
                "status",
                AvailabilityTone::Success,
            ));
            lines.push(PopupLine::new(
                "Part of the island-wide network linking parks together",
                "detail",
            ));
        }
        ParkFeatureKind::Trail => {
            let text = if feature.cycle.unwrap_or(false) {
                "Walking and cycling"
            } else {
                "Walking only"
            };
            lines.push(PopupLine::toned(text, "status", AvailabilityTone::Success));
            if let Some(t) = &feature.kind_type {
                lines.push(PopupLine::new(t, "count"));
            }
            if let Some(park) = &feature.park
                && park != &feature.name
            {
                lines.push(PopupLine::new(format!("In {}", park), "detail"));
            }
        }
    }
    if !feature.photos.is_empty()
        && let Some(credit) = crate::amenities::photo_credit(feature.photos)
    {
        lines.push(PopupLine::new(credit, "hint"));
    }
    lines.push(PopupLine::new("Source: NParks via data.gov.sg", "hint"));
    lines
}

fn point_in_ring(point: [f64; 2], ring: &Ring) -> bool {
    let (x, y) = (point[0], point[1]);
    let n = ring.len();
    let mut inside = false;
    for i in 0..n {
        let j = (i + n - 1) % n;
        let (xi, yi) = (ring[i][0], ring[i][1]);
        let (xj, yj) = (ring[j][0], ring[j][1]);
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
    }
    inside
}

fn bbox_center(coordinates: &serde_json::Value) -> Option<[f64; 2]> {
    let (mut min_lng, mut min_lat) = (f64::INFINITY, f64::INFINITY);
    let (mut max_lng, mut max_lat) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    fn visit(value: &serde_json::Value, bounds: &mut (f64, f64, f64, f64)) {
        let Some(arr) = value.as_array() else { return };
        if arr.first().and_then(serde_json::Value::as_f64).is_some() {
            if let (Some(lng), Some(lat)) = (
                arr.first().and_then(serde_json::Value::as_f64),
                arr.get(1).and_then(serde_json::Value::as_f64),
            ) {
                bounds.0 = bounds.0.min(lng);
                bounds.1 = bounds.1.min(lat);
                bounds.2 = bounds.2.max(lng);
                bounds.3 = bounds.3.max(lat);
            }
            return;
        }
        for child in arr {
            visit(child, bounds);
        }
    }
    let mut bounds = (min_lng, min_lat, max_lng, max_lat);
    visit(coordinates, &mut bounds);
    (min_lng, min_lat, max_lng, max_lat) = bounds;
    if min_lng.is_finite() {
        Some([(min_lng + max_lng) / 2.0, (min_lat + max_lat) / 2.0])
    } else {
        None
    }
}

/// One point to label / search-jump to for a park.
pub fn park_label_point(geometry: &crate::types::ParkGeometry) -> Option<[f64; 2]> {
    let center = bbox_center(&geometry.coordinates)?;
    let polygons: Vec<Vec<Ring>> = match geometry.r#type.as_str() {
        "Polygon" => vec![rings_from_value(&geometry.coordinates)],
        "MultiPolygon" => geometry
            .coordinates
            .as_array()
            .map(|p| p.iter().map(rings_from_value).collect())
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    for rings in &polygons {
        if let Some(outer) = rings.first()
            && point_in_ring(center, outer)
            && !rings[1..].iter().any(|hole| point_in_ring(center, hole))
        {
            return Some(center);
        }
    }
    let mut best: Option<[f64; 2]> = None;
    let mut best_distance = f64::INFINITY;
    for rings in &polygons {
        if let Some(outer) = rings.first() {
            for point in outer {
                let distance = (point[0] - center[0]).hypot(point[1] - center[1]);
                if distance < best_distance {
                    best = Some(*point);
                    best_distance = distance;
                }
            }
        }
    }
    best.or(Some(center))
}

/// One search entry per park area (not per connector or trail segment).
pub fn build_park_search_results(parks: &ParksGeoJson) -> Vec<MapSearchResult> {
    parks
        .features
        .iter()
        .filter(|f| f.properties.kind == ParkFeatureKind::Park)
        .filter_map(|f| {
            let center = park_label_point(&f.geometry)?;
            let large = f.properties.area_ha.unwrap_or(0.0) >= 50.0;
            Some(MapSearchResult {
                id: format!("park:{}", f.properties.name),
                result_type: SearchResultType::Park,
                name: f.properties.name.clone(),
                lat: center[1],
                lng: center[0],
                zoom: if large { 13.5 } else { 16.0 },
                keywords: vec!["park".into(), "nparks".into(), "garden".into()],
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn format_park_names() {
        assert_eq!(format_park_name("ADMIRALTY PK"), "Admiralty Park");
        assert_eq!(
            format_park_name("ISLAND GDNS WALK PG"),
            "Island Gardens Walk Playground"
        );
        assert_eq!(
            format_park_name("ANG MO KIO AVE 1 (CTE TO UPPER THOMSON)"),
            "Ang Mo Kio Avenue 1 (CTE to Upper Thomson)"
        );
        assert_eq!(
            format_park_name("ARMENIAN STREET (FCP)"),
            "Armenian Street (FCP)"
        );
        assert_eq!(
            format_park_name("PEARL'S HILL CITY PK"),
            "Pearl's Hill City Park"
        );
        assert_eq!(
            format_park_name("Bukit Timah PC (btwn King Albert Park - Adam Rd)"),
            "Bukit Timah Park Connector (between King Albert Park - Adam Road)"
        );
    }

    #[test]
    fn area_and_label_point() {
        let square = json!([
            [103.8, 1.3],
            [103.8009, 1.3],
            [103.8009, 1.3009],
            [103.8, 1.3009],
            [103.8, 1.3]
        ]);
        let geom = crate::types::ParkGeometry {
            r#type: "Polygon".into(),
            coordinates: json!([square]),
        };
        assert!((area_hectares(&geom) - 0.99).abs() < 0.1);
        let big = json!([
            [103.8, 1.3],
            [103.81, 1.3],
            [103.81, 1.31],
            [103.8, 1.31],
            [103.8, 1.3]
        ]);
        let geom2 = crate::types::ParkGeometry {
            r#type: "Polygon".into(),
            coordinates: json!([big]),
        };
        let point = park_label_point(&geom2).unwrap();
        assert!((point[0] - 103.805).abs() < 1e-6);
        assert!((point[1] - 1.305).abs() < 1e-6);
    }

    #[test]
    fn describes_park_and_search() {
        let d = ParkFeatureDetails {
            kind: ParkFeatureKind::Park,
            name: "Bishan-Ang Mo Kio Park".into(),
            area_ha: Some(62.4),
            ..Default::default()
        };
        let texts: Vec<_> = describe_park_feature(&d)
            .into_iter()
            .map(|l| l.text)
            .collect();
        assert_eq!(
            texts,
            [
                "Bishan-Ang Mo Kio Park",
                "Public park or nature area",
                "About 62 ha",
                "Source: NParks via data.gov.sg"
            ]
        );
        let parks = ParksGeoJson {
            r#type: "FeatureCollection".into(),
            features: vec![crate::types::ParkFeature {
                r#type: "Feature".into(),
                geometry: crate::types::ParkGeometry {
                    r#type: "Polygon".into(),
                    coordinates: json!([[
                        [103.8, 1.3],
                        [103.81, 1.3],
                        [103.81, 1.31],
                        [103.8, 1.3]
                    ]]),
                },
                properties: crate::types::ParkProperties {
                    kind: ParkFeatureKind::Park,
                    name: "X Park".into(),
                    park: None,
                    kind_type: None,
                    cycle: None,
                    area_ha: Some(60.0),
                },
            }],
        };
        assert_eq!(build_park_search_results(&parks).len(), 1);
    }
}
