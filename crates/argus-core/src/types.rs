//! Domain types ported from `types/index.ts`.
//!
//! Serde attributes mirror the JSON field names used by `public/*.json` and
//! the upstream APIs (camelCase for the static bundles, PascalCase for LTA).
//! The original TypeScript `interface`/`type` names are preserved where Rust
//! naming permits.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Shared GeoJSON shapes (types/index.ts: MRTGeoJson, ParksGeoJson)
// ---------------------------------------------------------------------------

pub type LngLat = [f64; 2];

/// `MRTGeoJson` from types/index.ts (public/mrt-lines.json).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MrtGeoJson {
    pub r#type: String,
    pub features: Vec<MrtLineFeature>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MrtLineFeature {
    pub r#type: String,
    pub properties: MrtLineProperties,
    pub geometry: LineStringGeometry,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MrtLineProperties {
    pub name: String,
    pub color: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// LRT lines are drawn thinner than MRT.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LineStringGeometry {
    pub r#type: String,
    pub coordinates: Vec<LngLat>,
}

/// `ParkFeatureKind` from types/index.ts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParkFeatureKind {
    #[default]
    Park,
    Pcn,
    Trail,
}

/// `ParksGeoJson` from types/index.ts (public/parks-geometry.json).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParksGeoJson {
    pub r#type: String,
    pub features: Vec<ParkFeature>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParkFeature {
    pub r#type: String,
    pub geometry: ParkGeometry,
    pub properties: ParkProperties,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParkGeometry {
    pub r#type: String,
    /// Polygon / MultiPolygon / LineString / MultiLineString coordinates.
    pub coordinates: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParkProperties {
    pub kind: ParkFeatureKind,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub park: Option<String>,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cycle: Option<bool>,
    /// Park areas only, derived at load from the polygon.
    #[serde(rename = "areaHa", default, skip_serializing_if = "Option::is_none")]
    pub area_ha: Option<f64>,
}

// ---------------------------------------------------------------------------
// Air quality (types/index.ts)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AirRegionKey {
    North,
    South,
    East,
    West,
    Central,
}

/// `RegionalAirReadings = Record<AirRegionKey, number | null>`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RegionalAirReadings {
    pub north: Option<f64>,
    pub south: Option<f64>,
    pub east: Option<f64>,
    pub west: Option<f64>,
    pub central: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PsiStatus {
    Good,
    Moderate,
    Unhealthy,
    #[serde(rename = "Very Unhealthy")]
    VeryUnhealthy,
    Hazardous,
    Unknown,
}

impl PsiStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            PsiStatus::Good => "Good",
            PsiStatus::Moderate => "Moderate",
            PsiStatus::Unhealthy => "Unhealthy",
            PsiStatus::VeryUnhealthy => "Very Unhealthy",
            PsiStatus::Hazardous => "Hazardous",
            PsiStatus::Unknown => "Unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Pm25Band {
    Normal,
    Elevated,
    High,
    #[serde(rename = "Very High")]
    VeryHigh,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AirQualityTone {
    Success,
    Warning,
    Danger,
    Muted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UvStatus {
    Low,
    Moderate,
    High,
    #[serde(rename = "Very High")]
    VeryHigh,
    Extreme,
    Unknown,
}

impl UvStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            UvStatus::Low => "Low",
            UvStatus::Moderate => "Moderate",
            UvStatus::High => "High",
            UvStatus::VeryHigh => "Very High",
            UvStatus::Extreme => "Extreme",
            UvStatus::Unknown => "Unknown",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WeatherForecastDay {
    pub day: String,
    pub text: String,
    #[serde(rename = "tempLow")]
    pub temp_low: f64,
    #[serde(rename = "tempHigh")]
    pub temp_high: f64,
}

/// `WeatherData` from types/index.ts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeatherData {
    pub temperature: Option<f64>,
    pub humidity: Option<f64>,
    pub psi: Option<f64>,
    pub psi_status: PsiStatus,
    pub psi_regions: RegionalAirReadings,
    pub pm25_one_hour: Option<f64>,
    pub pm25_one_hour_band: Pm25Band,
    pub pm25_one_hour_regions: RegionalAirReadings,
    pub uv: Option<f64>,
    pub uv_status: UvStatus,
    pub four_day: Vec<WeatherForecastDay>,
    pub forecast: String,
    pub last_updated: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WeatherHistoryPoint {
    pub timestamp: String,
    pub temperature: Option<f64>,
    pub humidity: Option<f64>,
    pub psi: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NewsItem {
    pub title: String,
    pub source: String,
    pub url: String,
    #[serde(rename = "publishedAt")]
    pub published_at: String,
}

// ---------------------------------------------------------------------------
// Traffic / mobility (types/index.ts)
// ---------------------------------------------------------------------------

/// `RawTrafficImage` & `{ location }` — `TrafficCamera` from types/index.ts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrafficCamera {
    #[serde(rename = "CameraID")]
    pub camera_id: String,
    #[serde(rename = "Latitude")]
    pub latitude: f64,
    #[serde(rename = "Longitude")]
    pub longitude: f64,
    #[serde(rename = "ImageLink")]
    pub image_link: String,
    pub location: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrafficIncident {
    pub id: String,
    #[serde(rename = "type")]
    pub incident_type: String,
    pub lat: f64,
    pub lng: f64,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CarparkAvailability {
    pub id: String,
    pub name: String,
    pub area: String,
    pub agency: String,
    pub lat: f64,
    pub lng: f64,
    pub car_lots: Option<i64>,
    pub motorcycle_lots: Option<i64>,
    pub heavy_lots: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvChargingLocation {
    pub id: String,
    pub name: String,
    pub address: String,
    pub lat: f64,
    pub lng: f64,
    pub available: i64,
    pub occupied: i64,
    pub total: i64,
    pub max_power_kw: Option<f64>,
    pub has_dc: bool,
    pub operators: Vec<String>,
    pub plug_types: Vec<String>,
}

/// `AvailabilityTone` from lib/mobility.ts (also `AirQualityTone`-like).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AvailabilityTone {
    Success,
    Warning,
    Danger,
    Muted,
}

/// `PopupLine` from lib/mobility.ts: one line of a map hover popup.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PopupLine {
    pub text: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tone: Option<AvailabilityTone>,
}

impl PopupLine {
    pub fn new(text: impl Into<String>, kind: &str) -> Self {
        Self { text: text.into(), kind: kind.into(), tone: None }
    }
    pub fn toned(text: impl Into<String>, kind: &str, tone: AvailabilityTone) -> Self {
        Self { text: text.into(), kind: kind.into(), tone: Some(tone) }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrainAlertStatus {
    Normal,
    Disrupted,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrainServiceAlert {
    pub status: TrainAlertStatus,
    pub message: String,
    pub affected_lines: Vec<String>,
    pub affected_stations: Vec<String>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
}

// ---------------------------------------------------------------------------
// Flights (types/index.ts)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FlightDirection {
    Inbound,
    Outbound,
    Transit,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlightState {
    pub id: String,
    pub icao24: String,
    pub callsign: String,
    pub origin_country: String,
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: Option<f64>,
    pub velocity: Option<f64>,
    pub track: Option<f64>,
    pub vertical_rate: Option<f64>,
    pub on_ground: bool,
    pub direction: FlightDirection,
    pub last_contact: Option<f64>,
}

// ---------------------------------------------------------------------------
// Static point amenities (types/index.ts)
// ---------------------------------------------------------------------------

/// `PlacePhoto` from types/index.ts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlacePhoto {
    pub thumb: String,
    pub page: Option<String>,
    pub author: String,
    pub license: Option<String>,
    pub source: PlacePhotoSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub year: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlacePhotoSource {
    Commons,
    Nea,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HawkerClosureKind {
    Cleaning,
    #[serde(rename = "other-works")]
    OtherWorks,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HawkerClosure {
    pub kind: HawkerClosureKind,
    pub start: String,
    pub end: String,
    pub remarks: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HawkerStatus {
    Open,
    Planned,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HawkerCentre {
    pub id: String,
    pub name: String,
    pub address: Option<String>,
    pub lat: f64,
    pub lng: f64,
    pub stalls: Option<i64>,
    pub status: HawkerStatus,
    pub expected_year: Option<i64>,
    pub photos: Vec<PlacePhoto>,
    /// Joined at load from public/hawker-closures.json (empty in the raw bundle).
    #[serde(default)]
    pub closures: Vec<HawkerClosure>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hotel {
    pub id: String,
    pub name: String,
    pub address: Option<String>,
    pub rooms: Option<i64>,
    pub lat: f64,
    pub lng: f64,
    pub photos: Vec<PlacePhoto>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mall {
    pub id: String,
    pub name: String,
    pub address: Option<String>,
    pub lat: f64,
    pub lng: f64,
    pub opening_hours: Option<String>,
    pub stores: Option<i64>,
    pub fnb: Option<i64>,
    pub nearest_mrt: Option<String>,
    pub photos: Vec<PlacePhoto>,
}

pub const GYM_CATEGORIES: &[&str] = &[
    "gym",
    "yoga",
    "pilates",
    "martial-arts",
    "crossfit",
    "climbing",
    "public",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GymCategory {
    Gym,
    Yoga,
    Pilates,
    #[serde(rename = "martial-arts")]
    MartialArts,
    Crossfit,
    Climbing,
    Public,
}

impl GymCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            GymCategory::Gym => "gym",
            GymCategory::Yoga => "yoga",
            GymCategory::Pilates => "pilates",
            GymCategory::MartialArts => "martial-arts",
            GymCategory::Crossfit => "crossfit",
            GymCategory::Climbing => "climbing",
            GymCategory::Public => "public",
        }
    }
}

pub const GYM_BRANDS: &[&str] = &[
    "ActiveSG",
    "Anytime Fitness",
    "Snap Fitness",
    "F45 Training",
    "Virgin Active",
    "CrossFit",
    "Fitness First",
    "True Fitness",
    "Pure Fitness",
    "UFIT",
    "GymmBoxx",
    "24/7 Fitness",
    "Gold's Gym",
    "EnergyOne",
    "GoFit",
    "Homeground",
    "Spartans",
    "Independent",
];

pub const GYM_SOURCES: &[&str] = &["alltheplaces", "osm", "sportsg", "curated"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GymSource {
    Alltheplaces,
    Osm,
    Sportsg,
    Curated,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Gym {
    pub id: String,
    pub name: String,
    /// Normalised operator brand; stored as a free string so brand lists can
    /// evolve without a schema change (validated against GYM_BRANDS at load).
    pub brand: String,
    pub category: GymCategory,
    pub address: Option<String>,
    pub lat: f64,
    pub lng: f64,
    pub opening_hours: Option<String>,
    pub website: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aliases: Option<Vec<String>>,
    pub source: GymSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommunityClubKind {
    #[serde(rename = "community-club")]
    CommunityClub,
    #[serde(rename = "passionwave")]
    Passionwave,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommunityClub {
    pub id: String,
    pub name: String,
    pub kind: CommunityClubKind,
    pub address: Option<String>,
    pub lat: f64,
    pub lng: f64,
    pub photos: Vec<PlacePhoto>,
}

pub const HERITAGE_KINDS: &[&str] = &["historic-site", "monument", "museum"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HeritageKind {
    #[serde(rename = "historic-site")]
    HistoricSite,
    Monument,
    Museum,
}

impl HeritageKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            HeritageKind::HistoricSite => "historic-site",
            HeritageKind::Monument => "monument",
            HeritageKind::Museum => "museum",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HeritageSource {
    Nhb,
    Osm,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeritageSite {
    pub id: String,
    pub name: String,
    pub kind: HeritageKind,
    pub address: Option<String>,
    pub description: Option<String>,
    pub website: Option<String>,
    pub lat: f64,
    pub lng: f64,
    pub photos: Vec<PlacePhoto>,
    pub source: HeritageSource,
}

pub const HEALTHCARE_KINDS: &[&str] = &["gp", "hospital", "dental"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthcareKind {
    Gp,
    Hospital,
    Dental,
}

impl HealthcareKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            HealthcareKind::Gp => "gp",
            HealthcareKind::Hospital => "hospital",
            HealthcareKind::Dental => "dental",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthcareSource {
    Chas,
    Osm,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthcareSite {
    pub id: String,
    pub name: String,
    pub kind: HealthcareKind,
    pub address: Option<String>,
    pub phone: Option<String>,
    pub programmes: Vec<String>,
    pub lat: f64,
    pub lng: f64,
    pub photos: Vec<PlacePhoto>,
    pub source: HealthcareSource,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicLibrary {
    pub id: String,
    pub name: String,
    pub branch: Option<String>,
    pub address: Option<String>,
    pub lat: f64,
    pub lng: f64,
    pub photos: Vec<PlacePhoto>,
    pub source: LibrarySource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LibrarySource {
    Nlb,
    Osm,
}

pub const SUPERMARKET_BRANDS: &[&str] = &[
    "NTUC FairPrice",
    "Cold Storage",
    "Sheng Siong",
    "Giant",
    "Prime",
    "Don Don Donki",
    "Ang Mo",
    "U Stars",
    "Hao Mart",
    "Scarlett",
    "Other",
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Supermarket {
    pub id: String,
    pub name: String,
    /// Normalised operator brand; validated against SUPERMARKET_BRANDS at load.
    pub brand: String,
    pub address: Option<String>,
    pub lat: f64,
    pub lng: f64,
    pub opening_hours: Option<String>,
    pub website: Option<String>,
}

pub const SCHOOL_LEVELS: &[&str] = &[
    "primary",
    "secondary",
    "jc",
    "mixed",
    "polytechnic",
    "university",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SchoolLevel {
    Primary,
    Secondary,
    Jc,
    Mixed,
    Polytechnic,
    University,
}

impl SchoolLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            SchoolLevel::Primary => "primary",
            SchoolLevel::Secondary => "secondary",
            SchoolLevel::Jc => "jc",
            SchoolLevel::Mixed => "mixed",
            SchoolLevel::Polytechnic => "polytechnic",
            SchoolLevel::University => "university",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CoordsSource {
    Onemap,
    Osm,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct School {
    pub id: String,
    pub name: String,
    pub level: SchoolLevel,
    pub address: Option<String>,
    pub postal_code: Option<String>,
    pub phone: Option<String>,
    pub website: Option<String>,
    pub mrt: Option<String>,
    pub bus: Option<String>,
    pub lat: f64,
    pub lng: f64,
    pub coords_source: CoordsSource,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pharmacy {
    pub id: String,
    pub name: String,
    pub address: Option<String>,
    pub lat: f64,
    pub lng: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MallStore {
    pub name: String,
    pub unit: Option<String>,
    pub level: Option<String>,
    pub category: Option<String>,
    pub fnb: bool,
    pub halal: bool,
    pub website: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MallStoreIndexEntry {
    pub stores: i64,
    #[serde(rename = "listName")]
    pub list_name: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MallStoreIndex {
    pub fetched_at: String,
    pub malls: std::collections::BTreeMap<String, MallStoreIndexEntry>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AmenitiesBundle {
    pub hawkers: Vec<HawkerCentre>,
    pub malls: Vec<Mall>,
}

/// `AmenityType` from types/index.ts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AmenityType {
    Mall,
    Hawker,
    Gym,
    #[serde(rename = "community-club")]
    CommunityClub,
    Pharmacy,
    Hotel,
    Heritage,
    Healthcare,
    Library,
    Supermarket,
    School,
}

impl AmenityType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AmenityType::Mall => "mall",
            AmenityType::Hawker => "hawker",
            AmenityType::Gym => "gym",
            AmenityType::CommunityClub => "community-club",
            AmenityType::Pharmacy => "pharmacy",
            AmenityType::Hotel => "hotel",
            AmenityType::Heritage => "heritage",
            AmenityType::Healthcare => "healthcare",
            AmenityType::Library => "library",
            AmenityType::Supermarket => "supermarket",
            AmenityType::School => "school",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "mall" => AmenityType::Mall,
            "hawker" => AmenityType::Hawker,
            "gym" => AmenityType::Gym,
            "community-club" => AmenityType::CommunityClub,
            "pharmacy" => AmenityType::Pharmacy,
            "hotel" => AmenityType::Hotel,
            "heritage" => AmenityType::Heritage,
            "healthcare" => AmenityType::Healthcare,
            "library" => AmenityType::Library,
            "supermarket" => AmenityType::Supermarket,
            "school" => AmenityType::School,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchResultType {
    #[serde(rename = "bus-stop")]
    BusStop,
    Mrt,
    Park,
    #[serde(untagged)]
    Amenity(AmenityType),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MapSearchResult {
    pub id: String,
    #[serde(rename = "type")]
    pub result_type: SearchResultType,
    pub name: String,
    pub lat: f64,
    pub lng: f64,
    pub zoom: f64,
    pub keywords: Vec<String>,
}

// ---------------------------------------------------------------------------
// LTA bus contracts (types/schemas.ts, re-exported by types/index.ts)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct BusStop {
    pub bus_stop_code: String,
    pub road_name: String,
    pub description: String,
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct BusTiming {
    pub origin_code: String,
    pub destination_code: String,
    pub estimated_arrival: String,
    /// Upstream sends latitude/longitude as strings.
    pub latitude: String,
    pub longitude: String,
    pub visit_number: String,
    pub load: String,
    pub feature: String,
    #[serde(rename = "Type")]
    pub timing_type: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct BusArrival {
    pub service_no: String,
    pub operator: String,
    pub next_bus: BusTiming,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_bus2: Option<BusTiming>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_bus3: Option<BusTiming>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BusRouteStop {
    pub bus_stop_code: String,
    pub description: String,
    pub latitude: f64,
    pub longitude: f64,
    pub stop_sequence: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BusRouteDirection {
    pub direction: i64,
    pub preferred: bool,
    pub selected_stop_index: Option<i64>,
    pub origin_code: String,
    pub destination_code: String,
    pub stops: Vec<BusRouteStop>,
    pub path: Vec<LngLat>,
    pub selected_path_index: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BusRouteResponse {
    pub service_no: String,
    pub directions: Vec<BusRouteDirection>,
}
