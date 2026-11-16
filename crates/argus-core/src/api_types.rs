//! External API response shapes ported from `types/schemas.ts`.
//!
//! In the TypeScript original these are Zod schemas that validate only the
//! consumed fields and ignore extra properties. Serde's default behaviour is
//! to ignore unknown fields, which is equivalent. Fields that TypeScript made
//! optional / nullish map to `Option`, and `z.union([number, string])` maps to
//! `NumberOrString`. Validation/refinements are exposed as `validate_*`
//! functions where the original had logic beyond shape checking.

use serde::{Deserialize, Serialize};

/// `numberOrString` from schemas.ts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum NumberOrString {
    Number(f64),
    String(String),
}

impl NumberOrString {
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            NumberOrString::Number(n) => Some(*n),
            NumberOrString::String(s) => s.trim().parse().ok(),
        }
    }
}

// ---------- LTA bus stops / arrivals / routes ----------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct LtaBusStopRow {
    pub bus_stop_code: String,
    pub road_name: String,
    pub description: String,
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LtaBusStopsResponse {
    pub value: Vec<LtaBusStopRow>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LtaBusArrivalsResponse {
    #[serde(rename = "Services")]
    pub services: Vec<crate::types::BusArrival>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct LtaBusRouteRow {
    pub service_no: String,
    pub direction: NumberOrString,
    pub stop_sequence: NumberOrString,
    pub bus_stop_code: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LtaBusRoutesResponse {
    pub value: Vec<LtaBusRouteRow>,
}

// ---------- LTA traffic cameras / incidents ----------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct RawTrafficImage {
    pub camera_id: String,
    pub latitude: f64,
    pub longitude: f64,
    pub image_link: String,
}

/// Accepts both `Traffic-Imagesv2` wrapper shapes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LtaTrafficImagesResponse {
    Flat { value: Vec<RawTrafficImage> },
    Grouped { value: Vec<CameraGroup> },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CameraGroup {
    #[serde(rename = "Cameras")]
    pub cameras: Vec<RawTrafficImage>,
}

impl LtaTrafficImagesResponse {
    /// Normalise either wrapper shape into a flat camera list.
    pub fn into_cameras(self) -> Vec<RawTrafficImage> {
        match self {
            LtaTrafficImagesResponse::Flat { value } => value,
            LtaTrafficImagesResponse::Grouped { value } => {
                value.into_iter().flat_map(|group| group.cameras).collect()
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct RawTrafficIncident {
    #[serde(rename = "Type")]
    pub incident_type: String,
    pub latitude: f64,
    pub longitude: f64,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LtaTrafficIncidentsResponse {
    pub value: Vec<RawTrafficIncident>,
}

// ---------- LTA carpark availability ----------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct RawCarparkAvailability {
    pub car_park_id: String,
    pub area: String,
    pub development: String,
    /// "lat lng" string.
    pub location: String,
    pub available_lots: i64,
    pub lot_type: String,
    pub agency: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LtaCarparkAvailabilityResponse {
    pub value: Vec<RawCarparkAvailability>,
}

// ---------- LTA EV charging points ----------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LtaEvcBatchResponse {
    pub value: Vec<LinkRow>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LinkRow {
    #[serde(rename = "Link")]
    pub link: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EvPlugType {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plug_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power_rating: Option<String>,
    pub ev_ids: Vec<EvId>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EvId {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EvChargingPoint {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operator: Option<String>,
    pub plug_types: Vec<EvPlugType>,
}

/// The batch file spells longitude as `longtitude`; accept either.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RawEvLocation {
    pub name: String,
    pub address: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latitude: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub longtitude: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub longitude: Option<f64>,
    pub charging_points: Vec<EvChargingPoint>,
}

impl RawEvLocation {
    pub fn effective_longitude(&self) -> Option<f64> {
        self.longitude.or(self.longtitude)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LtaEvBatchFile {
    pub ev_locations_data: Vec<RawEvLocation>,
}

// ---------- LTA train service alerts ----------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrainAlertSegment {
    #[serde(rename = "Line", default, skip_serializing_if = "Option::is_none")]
    pub line: Option<String>,
    /// Documented as a comma-separated string, also seen as an array.
    #[serde(rename = "Stations", default, skip_serializing_if = "Option::is_none")]
    pub stations: Option<StationList>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StationList {
    Csv(String),
    List(Vec<String>),
}

impl StationList {
    pub fn into_vec(self) -> Vec<String> {
        match self {
            StationList::Csv(s) => s
                .split(',')
                .map(|part| part.trim().to_string())
                .filter(|part| !part.is_empty())
                .collect(),
            StationList::List(v) => v,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LtaTrainAlert {
    #[serde(rename = "Status", default, skip_serializing_if = "Option::is_none")]
    pub status: Option<NumberOrString>,
    #[serde(rename = "Message", default, skip_serializing_if = "Option::is_none")]
    pub message: Option<Vec<MessageContent>>,
    #[serde(rename = "AffectedSegments", default, skip_serializing_if = "Option::is_none")]
    pub affected_segments: Option<Vec<TrainAlertSegment>>,
    #[serde(rename = "Start_time", default, skip_serializing_if = "Option::is_none")]
    pub start_time: Option<String>,
    #[serde(rename = "End_time", default, skip_serializing_if = "Option::is_none")]
    pub end_time: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MessageContent {
    #[serde(rename = "Content", default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

/// Accept `value` as either an alert array or a single all-clear alert.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LtaTrainServiceAlertsResponse {
    Many { value: Vec<LtaTrainAlert> },
    One { value: LtaTrainAlert },
}

impl LtaTrainServiceAlertsResponse {
    pub fn into_alerts(self) -> Vec<LtaTrainAlert> {
        match self {
            LtaTrainServiceAlertsResponse::Many { value } => value,
            LtaTrainServiceAlertsResponse::One { value } => vec![value],
        }
    }
}

// ---------- Data.gov.sg v1 ----------

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DataGovTimestamps {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update_timestamp: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DataGovForecastItem {
    #[serde(flatten)]
    pub timestamps: DataGovTimestamps,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forecasts: Option<Vec<AreaForecast>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AreaForecast {
    pub area: String,
    pub forecast: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DataGovForecastResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<DataGovForecastItem>>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RegionalReading {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub national: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub north: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub east: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub west: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub central: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub south: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PsiReadingItem {
    #[serde(flatten)]
    pub timestamps: DataGovTimestamps,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readings: Option<PsiReadings>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PsiReadings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub psi_twenty_four_hourly: Option<RegionalReading>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DataGovPsiResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<PsiReadingItem>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StationReadingsItem {
    #[serde(flatten)]
    pub timestamps: DataGovTimestamps,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readings: Option<Vec<ValueReading>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ValueReading {
    pub value: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DataGovStationReadingsResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<StationReadingsItem>>,
}

// ---------- Data.gov.sg v2 ----------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DataGovPm25Response {
    pub data: Pm25Data,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pm25Data {
    pub items: Vec<Pm25Item>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pm25Item {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    #[serde(rename = "updatedTimestamp", default, skip_serializing_if = "Option::is_none")]
    pub updated_timestamp: Option<String>,
    pub readings: Pm25Readings,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pm25Readings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pm25_one_hourly: Option<RegionalReading>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DataGovUvResponse {
    pub data: UvData,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UvData {
    pub records: Vec<UvRecord>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UvRecord {
    #[serde(rename = "updatedTimestamp")]
    pub updated_timestamp: String,
    pub timestamp: String,
    pub index: Vec<UvHour>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UvHour {
    pub hour: String,
    pub value: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DataGovFourDayResponse {
    pub data: FourDayData,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FourDayData {
    pub records: Vec<FourDayRecord>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FourDayRecord {
    pub timestamp: String,
    pub forecasts: Vec<FourDayForecast>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FourDayForecast {
    pub day: String,
    pub timestamp: String,
    pub forecast: ForecastText,
    pub temperature: ForecastTemperature,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ForecastText {
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ForecastTemperature {
    pub low: f64,
    pub high: f64,
}

// ---------- Aviationstack ----------

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AviationEndpoint {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub airport: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iata: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icao: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AviationAirline {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iata: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AviationFlightInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iata: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icao: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AviationAircraft {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registration: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icao24: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AviationLive {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latitude: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub longitude: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub altitude: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed_horizontal: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed_vertical: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_ground: Option<bool>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AviationStackFlight {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub departure: Option<AviationEndpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arrival: Option<AviationEndpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub airline: Option<AviationAirline>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flight: Option<AviationFlightInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aircraft: Option<AviationAircraft>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live: Option<AviationLive>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AviationError {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<NumberOrString>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_field: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub info: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AviationStackResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Vec<AviationStackFlight>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<AviationError>,
}

// ---------- OpenSky ----------

/// OpenSky state vectors are fixed positional 18-tuples. Serde derives only
/// support tuples up to arity 16, so this is a struct with a manual
/// sequence-based (de)serialisation matching the positional wire format.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OpenSkyState {
    pub icao24: Option<String>,          // 0
    pub callsign: Option<String>,        // 1
    pub origin_country: Option<String>,  // 2
    pub time_position: Option<f64>,      // 3
    pub last_contact: Option<f64>,       // 4
    pub longitude: Option<f64>,          // 5
    pub latitude: Option<f64>,           // 6
    pub baro_altitude: Option<f64>,      // 7
    pub on_ground: Option<bool>,         // 8
    pub velocity: Option<f64>,           // 9
    pub true_track: Option<f64>,         // 10
    pub vertical_rate: Option<f64>,      // 11
    pub sensors: Option<serde_json::Value>, // 12
    pub geo_altitude: Option<f64>,       // 13
    pub squawk: Option<String>,          // 14
    pub spi: Option<bool>,               // 15
    pub position_source: Option<f64>,    // 16
    pub category: Option<f64>,           // 17
}

impl<'de> Deserialize<'de> for OpenSkyState {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let values = <Vec<Option<serde_json::Value>> as Deserialize>::deserialize(d)?;
        let get_str = |i: usize| -> Option<String> {
            values.get(i).and_then(|v| v.as_ref()).and_then(|v| v.as_str()).map(str::to_string)
        };
        let get_f64 = |i: usize| -> Option<f64> {
            values.get(i).and_then(|v| v.as_ref()).and_then(|v| v.as_f64())
        };
        let get_bool = |i: usize| -> Option<bool> {
            values.get(i).and_then(|v| v.as_ref()).and_then(|v| v.as_bool())
        };
        Ok(OpenSkyState {
            icao24: get_str(0),
            callsign: get_str(1),
            origin_country: get_str(2),
            time_position: get_f64(3),
            last_contact: get_f64(4),
            longitude: get_f64(5),
            latitude: get_f64(6),
            baro_altitude: get_f64(7),
            on_ground: get_bool(8),
            velocity: get_f64(9),
            true_track: get_f64(10),
            vertical_rate: get_f64(11),
            sensors: values.get(12).and_then(|v| v.clone()),
            geo_altitude: get_f64(13),
            squawk: get_str(14),
            spi: get_bool(15),
            position_source: get_f64(16),
            category: get_f64(17),
        })
    }
}

impl Serialize for OpenSkyState {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeTuple;
        let mut tup = s.serialize_tuple(18)?;
        tup.serialize_element(&self.icao24)?;
        tup.serialize_element(&self.callsign)?;
        tup.serialize_element(&self.origin_country)?;
        tup.serialize_element(&self.time_position)?;
        tup.serialize_element(&self.last_contact)?;
        tup.serialize_element(&self.longitude)?;
        tup.serialize_element(&self.latitude)?;
        tup.serialize_element(&self.baro_altitude)?;
        tup.serialize_element(&self.on_ground)?;
        tup.serialize_element(&self.velocity)?;
        tup.serialize_element(&self.true_track)?;
        tup.serialize_element(&self.vertical_rate)?;
        tup.serialize_element(&self.sensors)?;
        tup.serialize_element(&self.geo_altitude)?;
        tup.serialize_element(&self.squawk)?;
        tup.serialize_element(&self.spi)?;
        tup.serialize_element(&self.position_source)?;
        tup.serialize_element(&self.category)?;
        tup.end()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OpenSkyResponse {
    pub states: Option<Vec<OpenSkyState>>,
}
