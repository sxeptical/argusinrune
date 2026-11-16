//! Amenity registry, sub-filters, display collision offsets and search.
//! Ported from `lib/amenity-layers.ts`.

use std::collections::{BTreeMap, HashMap};

use serde_json::Value;

use crate::data::StaticData;
use crate::popups;
use crate::preferences::parse_layer_option_selection;
use crate::types::*;

pub const AMENITY_COINCIDENCE_THRESHOLD_M: f64 = 10.0;
pub const AMENITY_COLLISION_RADIUS_M: f64 = 24.0;
const AMENITY_COLLISION_SPACING_M: f64 = 12.0;
const METRES_PER_DEGREE_LAT: f64 = (std::f64::consts::PI / 180.0) * 6_371_000.0;

/// A concrete amenity record, discriminated by type.
#[derive(Clone, Debug, PartialEq)]
pub enum Place {
    Mall(Mall),
    Hawker(HawkerCentre),
    Gym(Gym),
    CommunityClub(CommunityClub),
    Pharmacy(Pharmacy),
    Hotel(Hotel),
    Heritage(HeritageSite),
    Healthcare(HealthcareSite),
    Library(PublicLibrary),
    Supermarket(Supermarket),
    School(School),
}

impl Place {
    pub fn amenity_type(&self) -> AmenityType {
        match self {
            Place::Mall(_) => AmenityType::Mall,
            Place::Hawker(_) => AmenityType::Hawker,
            Place::Gym(_) => AmenityType::Gym,
            Place::CommunityClub(_) => AmenityType::CommunityClub,
            Place::Pharmacy(_) => AmenityType::Pharmacy,
            Place::Hotel(_) => AmenityType::Hotel,
            Place::Heritage(_) => AmenityType::Heritage,
            Place::Healthcare(_) => AmenityType::Healthcare,
            Place::Library(_) => AmenityType::Library,
            Place::Supermarket(_) => AmenityType::Supermarket,
            Place::School(_) => AmenityType::School,
        }
    }
    pub fn id(&self) -> &str {
        match self {
            Place::Mall(p) => &p.id,
            Place::Hawker(p) => &p.id,
            Place::Gym(p) => &p.id,
            Place::CommunityClub(p) => &p.id,
            Place::Pharmacy(p) => &p.id,
            Place::Hotel(p) => &p.id,
            Place::Heritage(p) => &p.id,
            Place::Healthcare(p) => &p.id,
            Place::Library(p) => &p.id,
            Place::Supermarket(p) => &p.id,
            Place::School(p) => &p.id,
        }
    }
    pub fn name(&self) -> &str {
        match self {
            Place::Mall(p) => &p.name,
            Place::Hawker(p) => &p.name,
            Place::Gym(p) => &p.name,
            Place::CommunityClub(p) => &p.name,
            Place::Pharmacy(p) => &p.name,
            Place::Hotel(p) => &p.name,
            Place::Heritage(p) => &p.name,
            Place::Healthcare(p) => &p.name,
            Place::Library(p) => &p.name,
            Place::Supermarket(p) => &p.name,
            Place::School(p) => &p.name,
        }
    }
    pub fn address(&self) -> Option<&str> {
        let a = match self {
            Place::Mall(p) => &p.address,
            Place::Hawker(p) => &p.address,
            Place::Gym(p) => &p.address,
            Place::CommunityClub(p) => &p.address,
            Place::Pharmacy(p) => &p.address,
            Place::Hotel(p) => &p.address,
            Place::Heritage(p) => &p.address,
            Place::Healthcare(p) => &p.address,
            Place::Library(p) => &p.address,
            Place::Supermarket(p) => &p.address,
            Place::School(p) => &p.address,
        };
        a.as_deref()
    }
    pub fn lat(&self) -> f64 {
        match self {
            Place::Mall(p) => p.lat,
            Place::Hawker(p) => p.lat,
            Place::Gym(p) => p.lat,
            Place::CommunityClub(p) => p.lat,
            Place::Pharmacy(p) => p.lat,
            Place::Hotel(p) => p.lat,
            Place::Heritage(p) => p.lat,
            Place::Healthcare(p) => p.lat,
            Place::Library(p) => p.lat,
            Place::Supermarket(p) => p.lat,
            Place::School(p) => p.lat,
        }
    }
    pub fn lng(&self) -> f64 {
        match self {
            Place::Mall(p) => p.lng,
            Place::Hawker(p) => p.lng,
            Place::Gym(p) => p.lng,
            Place::CommunityClub(p) => p.lng,
            Place::Pharmacy(p) => p.lng,
            Place::Hotel(p) => p.lng,
            Place::Heritage(p) => p.lng,
            Place::Healthcare(p) => p.lng,
            Place::Library(p) => p.lng,
            Place::Supermarket(p) => p.lng,
            Place::School(p) => p.lng,
        }
    }
    pub fn photos(&self) -> &[PlacePhoto] {
        match self {
            Place::Mall(p) => &p.photos,
            Place::Hawker(p) => &p.photos,
            Place::CommunityClub(p) => &p.photos,
            Place::Heritage(p) => &p.photos,
            Place::Healthcare(p) => &p.photos,
            Place::Hotel(p) => &p.photos,
            Place::Library(p) => &p.photos,
            _ => &[],
        }
    }
    /// The sub-filter option this place belongs to.
    pub fn filter_option(&self) -> Option<String> {
        match self {
            Place::Gym(p) => Some(p.brand.clone()),
            Place::Supermarket(p) => Some(p.brand.clone()),
            Place::Heritage(p) => Some(p.kind.as_str().to_string()),
            Place::Healthcare(p) => Some(p.kind.as_str().to_string()),
            Place::School(p) => Some(p.level.as_str().to_string()),
            _ => None,
        }
    }
    pub fn keywords(&self) -> Vec<String> {
        let mut kw: Vec<String> = match self {
            Place::Mall(_) => ["mall", "shopping"].iter().map(|s| s.to_string()).collect(),
            Place::Hawker(_) => ["hawker", "food centre"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            Place::Gym(p) => {
                let mut v: Vec<String> = ["gym", "fitness", popups::gym_category_label(p.category)]
                    .iter()
                    .map(|s| s.to_string())
                    .collect();
                v.push(p.brand.clone());
                if let Some(aliases) = &p.aliases {
                    v.extend(aliases.clone());
                }
                v
            }
            Place::CommunityClub(p) => [
                "community club",
                "PA",
                popups::community_club_kind_label(p.kind),
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            Place::Pharmacy(_) => ["pharmacy", "chemist"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            Place::Hotel(p) => {
                let mut v: Vec<String> = ["hotel", "stay"].iter().map(|s| s.to_string()).collect();
                if let Some(rooms) = p.rooms {
                    v.push(rooms.to_string());
                }
                v
            }
            Place::Heritage(p) => vec![
                "heritage".into(),
                popups::heritage_kind_label(p.kind).into(),
            ],
            Place::Healthcare(p) => [
                "healthcare",
                "clinic",
                popups::healthcare_kind_label(p.kind),
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            Place::Library(p) => {
                let mut v: Vec<String> = ["library", "NLB"].iter().map(|s| s.to_string()).collect();
                if let Some(branch) = &p.branch {
                    v.push(branch.clone());
                }
                v
            }
            Place::Supermarket(p) => ["supermarket", "grocery", p.brand.as_str()]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            Place::School(p) => vec!["school".into(), popups::school_level_label(p.level).into()],
        };
        if let Some(address) = self.address()
            && !address.is_empty()
        {
            kw.push(address.to_string());
        }
        kw
    }
    pub fn describe(&self) -> Vec<PopupLine> {
        match self {
            Place::Mall(p) => crate::amenities::describe_mall(p),
            Place::Hawker(p) => {
                crate::amenities::describe_hawker_centre(p, &crate::amenities::today())
            }
            Place::Gym(p) => popups::describe_gym(p),
            Place::CommunityClub(p) => popups::describe_community_club(p),
            Place::Pharmacy(p) => popups::describe_pharmacy(p),
            Place::Hotel(p) => popups::describe_hotel(p),
            Place::Heritage(p) => popups::describe_heritage(p),
            Place::Healthcare(p) => popups::describe_healthcare(p),
            Place::Library(p) => popups::describe_library(p),
            Place::Supermarket(p) => popups::describe_supermarket(p),
            Place::School(p) => popups::describe_school(p),
        }
    }
    pub fn popup_title(&self) -> &'static str {
        match self {
            Place::Mall(_) => "MALL",
            Place::Hawker(_) => "HAWKER CENTRE",
            Place::Gym(_) => "GYM",
            Place::CommunityClub(p) if p.kind == CommunityClubKind::Passionwave => "PASSIONWAVE",
            Place::CommunityClub(_) => "COMMUNITY CLUB",
            Place::Pharmacy(_) => "PHARMACY",
            Place::Hotel(_) => "HOTEL",
            Place::Heritage(_) => "HERITAGE",
            Place::Healthcare(_) => "HEALTHCARE",
            Place::Library(_) => "LIBRARY",
            Place::Supermarket(_) => "SUPERMARKET",
            Place::School(_) => "SCHOOL",
        }
    }
    pub fn muted(&self) -> bool {
        matches!(self, Place::Hawker(h) if h.status == HawkerStatus::Planned)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayerOption {
    pub value: String,
    pub label: String,
    pub count: i64,
}

#[derive(Clone, Debug)]
pub struct SubFilter {
    pub label: String,
    pub options: Vec<String>,
    pub option_label: BTreeMap<String, String>,
    pub by_count: bool,
    pub first: Option<String>,
    pub last: Option<String>,
    pub choices: Vec<LayerOption>,
}

#[derive(Clone, Debug)]
pub struct AmenityLayer {
    pub amenity_type: AmenityType,
    pub layer_key: String,
    pub color: String,
    pub places: Vec<Place>,
    pub by_id: HashMap<String, usize>,
    pub filter: Option<SubFilter>,
    pub min_zoom: Option<(f64, f64)>,
}

pub type AmenityFilters = HashMap<AmenityType, BTreeMap<String, bool>>;

fn build_choices(
    options: &[String],
    option_label: &BTreeMap<String, String>,
    by_count: bool,
    first: Option<&str>,
    last: Option<&str>,
    places: &[Place],
) -> Vec<LayerOption> {
    let mut counts: BTreeMap<String, i64> = BTreeMap::new();
    for place in places {
        if let Some(option) = place.filter_option() {
            *counts.entry(option).or_default() += 1;
        }
    }
    let pin = |value: &str| -> i64 {
        if first == Some(value) {
            -1
        } else if last == Some(value) {
            1
        } else {
            0
        }
    };
    let mut result: Vec<LayerOption> = options
        .iter()
        .map(|value| LayerOption {
            value: value.clone(),
            label: option_label
                .get(value)
                .cloned()
                .unwrap_or_else(|| value.clone()),
            count: counts.get(value).copied().unwrap_or(0),
        })
        .filter(|option| option.count > 0)
        .collect();
    result.sort_by(|a, b| {
        pin(&a.value).cmp(&pin(&b.value)).then_with(|| {
            if by_count {
                b.count.cmp(&a.count).then_with(|| a.value.cmp(&b.value))
            } else {
                std::cmp::Ordering::Equal
            }
        })
    });
    result
}

fn label_map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// The amenity registry, built from the loaded static bundles.
#[derive(Clone, Debug)]
pub struct AmenityRegistry {
    pub layers: Vec<AmenityLayer>,
    pub display_coords: BTreeMap<String, AmenityDisplayPoint>,
    pub group_by_key: HashMap<String, Vec<CoLocatedItem>>,
    pub search_results: Vec<MapSearchResult>,
}

impl AmenityRegistry {
    pub fn new(data: &StaticData) -> Self {
        let amenity_type_order = [
            AmenityType::Mall,
            AmenityType::Hawker,
            AmenityType::Gym,
            AmenityType::CommunityClub,
            AmenityType::Pharmacy,
            AmenityType::Hotel,
            AmenityType::Heritage,
            AmenityType::Library,
            AmenityType::Supermarket,
            AmenityType::School,
            AmenityType::Healthcare,
        ];
        let mut layers = Vec::new();
        for kind in amenity_type_order {
            let places: Vec<Place> = match kind {
                AmenityType::Mall => data.malls.iter().cloned().map(Place::Mall).collect(),
                AmenityType::Hawker => data
                    .hawker_centres
                    .iter()
                    .cloned()
                    .map(Place::Hawker)
                    .collect(),
                AmenityType::Gym => data.gyms.iter().cloned().map(Place::Gym).collect(),
                AmenityType::CommunityClub => data
                    .community_clubs
                    .iter()
                    .cloned()
                    .map(Place::CommunityClub)
                    .collect(),
                AmenityType::Pharmacy => data
                    .pharmacies
                    .iter()
                    .cloned()
                    .map(Place::Pharmacy)
                    .collect(),
                AmenityType::Hotel => data.hotels.iter().cloned().map(Place::Hotel).collect(),
                AmenityType::Heritage => data
                    .heritage_sites
                    .iter()
                    .cloned()
                    .map(Place::Heritage)
                    .collect(),
                AmenityType::Library => {
                    data.libraries.iter().cloned().map(Place::Library).collect()
                }
                AmenityType::Supermarket => data
                    .supermarkets
                    .iter()
                    .cloned()
                    .map(Place::Supermarket)
                    .collect(),
                AmenityType::School => data.schools.iter().cloned().map(Place::School).collect(),
                AmenityType::Healthcare => data
                    .healthcare_sites
                    .iter()
                    .cloned()
                    .map(Place::Healthcare)
                    .collect(),
            };
            let (color, layer_key, filter, min_zoom) = match kind {
                AmenityType::Mall => ("#e879c6", "malls", None, None),
                AmenityType::Hawker => ("#f0b060", "hawkers", None, None),
                AmenityType::Gym => (
                    "#c084fc",
                    "gyms",
                    Some((
                        "Brands",
                        GYM_BRANDS,
                        true,
                        Some("ActiveSG"),
                        None,
                        label_map(&[]),
                    )),
                    None,
                ),
                AmenityType::CommunityClub => ("#4fc3e5", "communityClubs", None, None),
                AmenityType::Pharmacy => ("#f0715f", "pharmacies", None, None),
                AmenityType::Hotel => ("#5f9fe8", "hotels", None, None),
                AmenityType::Heritage => (
                    "#d9c84f",
                    "heritage",
                    Some((
                        "Types",
                        HERITAGE_KINDS,
                        false,
                        None,
                        None,
                        label_map(&[
                            ("historic-site", "Historic site"),
                            ("monument", "National monument"),
                            ("museum", "Museum"),
                        ]),
                    )),
                    None,
                ),
                AmenityType::Library => ("#5fd6a8", "libraries", None, None),
                AmenityType::Supermarket => (
                    "#d95fb0",
                    "supermarkets",
                    Some((
                        "Brands",
                        SUPERMARKET_BRANDS,
                        true,
                        None,
                        Some("Other"),
                        label_map(&[]),
                    )),
                    None,
                ),
                AmenityType::School => (
                    "#a678ff",
                    "schools",
                    Some((
                        "Levels",
                        SCHOOL_LEVELS,
                        false,
                        None,
                        None,
                        label_map(&[
                            ("primary", "Primary"),
                            ("secondary", "Secondary"),
                            ("jc", "JC"),
                            ("mixed", "Mixed"),
                            ("polytechnic", "Polytechnic"),
                            ("university", "University"),
                        ]),
                    )),
                    None,
                ),
                AmenityType::Healthcare => (
                    "#d9553f",
                    "healthcare",
                    Some((
                        "Types",
                        HEALTHCARE_KINDS,
                        false,
                        None,
                        None,
                        label_map(&[
                            ("gp", "GP clinic"),
                            ("hospital", "Hospital"),
                            ("dental", "Dental clinic"),
                        ]),
                    )),
                    Some((11.0, 15.0)),
                ),
            };
            let filter = filter.map(|(label, opts, by_count, first, last, option_label)| {
                let options: Vec<String> = opts.iter().map(|s| s.to_string()).collect();
                let choices =
                    build_choices(&options, &option_label, by_count, first, last, &places);
                SubFilter {
                    label: label.into(),
                    options,
                    option_label,
                    by_count,
                    first: first.map(str::to_string),
                    last: last.map(str::to_string),
                    choices,
                }
            });
            let by_id = places
                .iter()
                .enumerate()
                .map(|(i, p)| (p.id().to_string(), i))
                .collect();
            layers.push(AmenityLayer {
                amenity_type: kind,
                layer_key: layer_key.into(),
                color: color.into(),
                places,
                by_id,
                filter,
                min_zoom,
            });
        }

        let search_results: Vec<MapSearchResult> = layers
            .iter()
            .flat_map(|layer| {
                layer.places.iter().map(move |place| MapSearchResult {
                    id: place.id().to_string(),
                    result_type: SearchResultType::Amenity(layer.amenity_type),
                    name: place.name().to_string(),
                    lat: place.lat(),
                    lng: place.lng(),
                    zoom: 16.0,
                    keywords: place.keywords(),
                })
            })
            .collect();

        let groups = group_co_located(
            &layers
                .iter()
                .map(|layer| OffsetLayer {
                    name: layer.amenity_type.as_str().to_string(),
                    places: layer
                        .places
                        .iter()
                        .map(|p| OffsetPoint {
                            id: p.id().to_string(),
                            lat: p.lat(),
                            lng: p.lng(),
                        })
                        .collect(),
                })
                .collect::<Vec<_>>(),
            AMENITY_COINCIDENCE_THRESHOLD_M,
        );
        let mut group_by_key = HashMap::new();
        for group in &groups {
            for member in group {
                group_by_key.insert(member.key.clone(), group.clone());
            }
        }
        let display_coords = ring_offsets(&groups, AMENITY_COLLISION_RADIUS_M);

        Self {
            layers,
            display_coords,
            group_by_key,
            search_results,
        }
    }

    pub fn layer(&self, kind: AmenityType) -> Option<&AmenityLayer> {
        self.layers.iter().find(|l| l.amenity_type == kind)
    }

    pub fn display_point(&self, kind: AmenityType, place: &Place) -> AmenityDisplayPoint {
        self.display_coords
            .get(&amenity_display_key(kind.as_str(), place.id()))
            .copied()
            .unwrap_or(AmenityDisplayPoint {
                lat: place.lat(),
                lng: place.lng(),
            })
    }

    pub fn find(&self, kind: AmenityType, id: &str) -> Option<(AmenityType, &Place)> {
        let layer = self.layer(kind)?;
        let idx = *layer.by_id.get(id)?;
        Some((kind, &layer.places[idx]))
    }

    pub fn co_located(
        &self,
        kind: AmenityType,
        id: &str,
        shown: impl Fn(AmenityType, &Place) -> bool,
    ) -> Vec<(AmenityType, &Place)> {
        let group = match self
            .group_by_key
            .get(&amenity_display_key(kind.as_str(), id))
        {
            Some(g) => g,
            None => return Vec::new(),
        };
        group
            .iter()
            .filter(|member| !(member.amenity_type == kind && member.id == id))
            .filter_map(|member| {
                let layer = self.layer(member.amenity_type)?;
                let idx = *layer.by_id.get(&member.id)?;
                let place = &layer.places[idx];
                shown(member.amenity_type, place).then_some((member.amenity_type, place))
            })
            .collect()
    }
}

/// Whether a place passes its layer's sub-filter (absent option reads as on).
pub fn passes_sub_filter(layer: &AmenityLayer, filters: &AmenityFilters, place: &Place) -> bool {
    match &layer.filter {
        None => true,
        Some(_filter) => match filters.get(&layer.amenity_type) {
            None => true,
            Some(selection) => match place.filter_option() {
                None => true,
                Some(option) => selection.get(&option).copied().unwrap_or(true),
            },
        },
    }
}

pub fn visible_places<'a>(layer: &'a AmenityLayer, filters: &AmenityFilters) -> Vec<&'a Place> {
    if layer.filter.is_none() || !filters.contains_key(&layer.amenity_type) {
        return layer.places.iter().collect();
    }
    layer
        .places
        .iter()
        .filter(|place| passes_sub_filter(layer, filters, place))
        .collect()
}

/// Hydrate persisted sub-filter selections.
pub fn parse_amenity_filters(stored: &Value, registry: &AmenityRegistry) -> Option<AmenityFilters> {
    let obj = stored.as_object()?;
    let mut filters = AmenityFilters::new();
    for layer in &registry.layers {
        let Some(filter) = &layer.filter else {
            continue;
        };
        let all_on: BTreeMap<String, bool> =
            filter.options.iter().map(|o| (o.clone(), true)).collect();
        let raw = obj
            .get(layer.amenity_type.as_str())
            .cloned()
            .unwrap_or(Value::Null);
        let Some(selection) = parse_layer_option_selection(&raw, &all_on) else {
            continue;
        };
        let any_choice_on = filter
            .choices
            .iter()
            .any(|choice| selection.get(&choice.value).copied().unwrap_or(false));
        filters.insert(
            layer.amenity_type,
            if any_choice_on { selection } else { all_on },
        );
    }
    Some(filters)
}

// ---------------------------------------------------------------------------
// Collision offsets
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AmenityDisplayPoint {
    pub lat: f64,
    pub lng: f64,
}

#[derive(Clone, Debug)]
pub struct OffsetPoint {
    pub id: String,
    pub lat: f64,
    pub lng: f64,
}

#[derive(Clone, Debug)]
pub struct OffsetLayer {
    pub name: String,
    pub places: Vec<OffsetPoint>,
}

#[derive(Clone, Debug)]
pub struct CoLocatedItem {
    pub amenity_type: AmenityType,
    pub id: String,
    pub lat: f64,
    pub lng: f64,
    pub key: String,
}

pub fn amenity_display_key(kind: &str, id: &str) -> String {
    format!("{}:{}", kind, id)
}

fn metres_between(a: AmenityDisplayPoint, b: AmenityDisplayPoint) -> f64 {
    let d_lat = (b.lat - a.lat) * METRES_PER_DEGREE_LAT;
    let d_lng = (b.lng - a.lng)
        * METRES_PER_DEGREE_LAT
        * (((a.lat + b.lat) / 2.0) * std::f64::consts::PI / 180.0).cos();
    d_lat.hypot(d_lng)
}

/// Group points within `thresholdM` of each other across layers (union-find
/// with grid bucketing), deterministic and independent of source order.
pub fn group_co_located(layers: &[OffsetLayer], threshold_m: f64) -> Vec<Vec<CoLocatedItem>> {
    if threshold_m <= 0.0 {
        return Vec::new();
    }
    let mut items: Vec<CoLocatedItem> = Vec::new();
    for (layer_index, layer) in layers.iter().enumerate() {
        let _ = layer_index;
        // Sort inside each layer by id so source order does not matter.
        let mut places = layer.places.clone();
        places.sort_by(|a, b| a.id.cmp(&b.id));
        let kind = AmenityType::parse(&layer.name).unwrap_or(AmenityType::Mall);
        for place in places {
            items.push(CoLocatedItem {
                amenity_type: kind,
                id: place.id.clone(),
                lat: place.lat,
                lng: place.lng,
                key: amenity_display_key(&layer.name, &place.id),
            });
        }
    }
    if items.len() < 2 {
        return Vec::new();
    }

    let cell_lat_deg = (threshold_m * 1.5) / METRES_PER_DEGREE_LAT;
    let max_abs_lat = items
        .iter()
        .fold(0.0_f64, |acc, item| acc.max(item.lat.abs()));
    let cell_lng_deg = cell_lat_deg / max_abs_lat.clamp(0.2, 80.0).to_radians().cos();
    let mut buckets: HashMap<String, Vec<usize>> = HashMap::new();
    let cell_of = |lat: f64, lng: f64| -> String {
        format!(
            "{}:{}",
            (lat / cell_lat_deg).floor(),
            (lng / cell_lng_deg).floor()
        )
    };
    for (index, item) in items.iter().enumerate() {
        buckets
            .entry(cell_of(item.lat, item.lng))
            .or_default()
            .push(index);
    }

    let mut parent: Vec<usize> = (0..items.len()).collect();
    fn find(parent: &mut [usize], mut index: usize) -> usize {
        let mut root = index;
        while parent[root] != root {
            root = parent[root];
        }
        while parent[index] != index {
            let next = parent[index];
            parent[index] = root;
            index = next;
        }
        root
    }
    for (index, item) in items.iter().enumerate() {
        let cell_lat = (item.lat / cell_lat_deg).floor();
        let cell_lng = (item.lng / cell_lng_deg).floor();
        for dy in -1..=1 {
            for dx in -1..=1 {
                let key = format!("{}:{}", cell_lat + dy as f64, cell_lng + dx as f64);
                if let Some(other_indices) = buckets.get(&key) {
                    for &other in other_indices {
                        if other <= index {
                            continue;
                        }
                        if metres_between(
                            AmenityDisplayPoint {
                                lat: item.lat,
                                lng: item.lng,
                            },
                            AmenityDisplayPoint {
                                lat: items[other].lat,
                                lng: items[other].lng,
                            },
                        ) <= threshold_m
                        {
                            let a = find(&mut parent, index);
                            let b = find(&mut parent, other);
                            if a != b {
                                parent[a] = b;
                            }
                        }
                    }
                }
            }
        }
    }
    let mut groups: HashMap<usize, Vec<CoLocatedItem>> = HashMap::new();
    for (index, item) in items.into_iter().enumerate() {
        let root = find(&mut parent, index);
        groups.entry(root).or_default().push(item);
    }
    let mut result: Vec<Vec<CoLocatedItem>> = groups
        .into_values()
        .filter(|members| members.len() > 1)
        .collect();
    // Deterministic ordering of groups by their first member's key.
    result.sort_by(|a, b| a[0].key.cmp(&b[0].key));
    // Sort members deterministically: registry order then id.
    for group in &mut result {
        group.sort_by(|a, b| {
            a.amenity_type
                .as_str()
                .cmp(b.amenity_type.as_str())
                .then_with(|| a.id.cmp(&b.id))
        });
    }
    result
}

fn ring_offsets(
    groups: &[Vec<CoLocatedItem>],
    radius_m: f64,
) -> BTreeMap<String, AmenityDisplayPoint> {
    let mut offsets = BTreeMap::new();
    for members in groups {
        let ring_radius = radius_m
            .max(members.len() as f64 * AMENITY_COLLISION_SPACING_M / (2.0 * std::f64::consts::PI));
        for (position, item) in members.iter().enumerate() {
            let angle = 2.0 * std::f64::consts::PI * position as f64 / members.len() as f64;
            let east = angle.cos() * ring_radius;
            let north = angle.sin() * ring_radius;
            let cos_lat = (item.lat * std::f64::consts::PI / 180.0).cos().max(0.2);
            offsets.insert(
                item.key.clone(),
                AmenityDisplayPoint {
                    lat: item.lat + north / METRES_PER_DEGREE_LAT,
                    lng: item.lng + east / (METRES_PER_DEGREE_LAT * cos_lat),
                },
            );
        }
    }
    offsets
}

pub fn compute_amenity_offsets(
    layers: &[OffsetLayer],
    threshold_m: f64,
    radius_m: f64,
) -> BTreeMap<String, AmenityDisplayPoint> {
    ring_offsets(&group_co_located(layers, threshold_m), radius_m)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(id: &str, lat: f64, lng: f64) -> OffsetPoint {
        OffsetPoint {
            id: id.into(),
            lat,
            lng,
        }
    }
    fn layer(name: &str, points: Vec<OffsetPoint>) -> OffsetLayer {
        OffsetLayer {
            name: name.into(),
            places: points,
        }
    }

    #[test]
    fn groups_nearby_points_and_ignores_far_ones() {
        let near = compute_amenity_offsets(
            &[layer(
                "mall",
                vec![
                    point("a", 1.3, 103.8),
                    point("b", 1.3 + 5.0 / 111_320.0, 103.8),
                ],
            )],
            AMENITY_COINCIDENCE_THRESHOLD_M,
            AMENITY_COLLISION_RADIUS_M,
        );
        assert_eq!(near.len(), 2);
        assert!(near.contains_key("mall:a"));
        let far = compute_amenity_offsets(
            &[layer(
                "mall",
                vec![
                    point("a", 1.3, 103.8),
                    point("b", 1.3 + 20.0 / 111_320.0, 103.8),
                ],
            )],
            AMENITY_COINCIDENCE_THRESHOLD_M,
            AMENITY_COLLISION_RADIUS_M,
        );
        assert!(far.is_empty());
    }

    #[test]
    fn grouping_is_order_independent() {
        let places = vec![
            point("b", 1.3, 103.8),
            point("a", 1.3 + 4.0 / 111_320.0, 103.8),
            point("c", 1.3, 103.8 + 4.0 / 111_320.0),
        ];
        let forward = compute_amenity_offsets(&[layer("gym", places.clone())], 10.0, 24.0);
        let mut reversed = places.clone();
        reversed.reverse();
        let reversed = compute_amenity_offsets(&[layer("gym", reversed)], 10.0, 24.0);
        assert_eq!(forward, reversed);
        assert_eq!(
            forward.keys().cloned().collect::<Vec<_>>(),
            ["gym:a", "gym:b", "gym:c"]
        );
    }

    #[test]
    fn far_layers_do_not_move_a_group() {
        let colliding = vec![
            layer("gym", vec![point("z", 1.3, 103.8)]),
            layer("pharmacy", vec![point("y", 1.3, 103.8)]),
        ];
        let base = compute_amenity_offsets(&colliding, 10.0, 24.0);
        let mut with_extra = colliding.clone();
        with_extra.push(layer("hotel", vec![point("h", 1.4, 103.9)]));
        let with_extra = compute_amenity_offsets(&with_extra, 10.0, 24.0);
        assert_eq!(with_extra.get("gym:z"), base.get("gym:z"));
        assert_eq!(with_extra.get("pharmacy:y"), base.get("pharmacy:y"));
        assert!(!with_extra.contains_key("hotel:h"));
    }
}
