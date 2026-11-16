//! Upstream wire shapes for the few LTA responses whose exact field spelling
//! differs from `argus-core::api_types` (`CameraID`/`CarParkID` acronyms and
//! the EV batch file's camelCase keys). Deserialise into these, then convert
//! into the core types so the shared pure normalizers stay unchanged.

use argus_core::api_types::{
    EvChargingPoint, EvId, EvPlugType, LtaEvBatchFile, RawCarparkAvailability, RawEvLocation,
    RawTrafficImage,
};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct WireTrafficImage {
    #[serde(rename = "CameraID")]
    pub camera_id: String,
    pub latitude: f64,
    pub longitude: f64,
    pub image_link: String,
}

impl From<WireTrafficImage> for RawTrafficImage {
    fn from(c: WireTrafficImage) -> Self {
        RawTrafficImage {
            camera_id: c.camera_id,
            latitude: c.latitude,
            longitude: c.longitude,
            image_link: c.image_link,
        }
    }
}

#[derive(Deserialize)]
pub struct WireCameraGroup {
    #[serde(rename = "Cameras")]
    pub cameras: Vec<WireTrafficImage>,
}

/// Both `Traffic-Imagesv2` wrapper shapes.
#[derive(Deserialize)]
#[serde(untagged)]
pub enum WireTrafficImagesResponse {
    Flat { value: Vec<WireTrafficImage> },
    Grouped { value: Vec<WireCameraGroup> },
}

impl WireTrafficImagesResponse {
    pub fn into_cameras(self) -> Vec<RawTrafficImage> {
        match self {
            WireTrafficImagesResponse::Flat { value } => value.into_iter().map(Into::into).collect(),
            WireTrafficImagesResponse::Grouped { value } => value
                .into_iter()
                .flat_map(|g| g.cameras.into_iter().map(Into::into))
                .collect(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct WireCarparkRow {
    #[serde(rename = "CarParkID")]
    pub car_park_id: String,
    pub area: String,
    pub development: String,
    pub location: String,
    pub available_lots: i64,
    pub lot_type: String,
    pub agency: String,
}

impl From<WireCarparkRow> for RawCarparkAvailability {
    fn from(row: WireCarparkRow) -> Self {
        RawCarparkAvailability {
            car_park_id: row.car_park_id,
            area: row.area,
            development: row.development,
            location: row.location,
            available_lots: row.available_lots,
            lot_type: row.lot_type,
            agency: row.agency,
        }
    }
}

#[derive(Deserialize)]
pub struct WireCarparkResponse {
    pub value: Vec<WireCarparkRow>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireEvId {
    pub status: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireEvPlugType {
    pub plug_type: Option<String>,
    pub current: Option<String>,
    pub power_rating: Option<String>,
    pub ev_ids: Vec<WireEvId>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireEvChargingPoint {
    pub operator: Option<String>,
    pub plug_types: Vec<WireEvPlugType>,
}

#[derive(Deserialize)]
pub struct WireEvLocation {
    pub name: String,
    pub address: String,
    pub latitude: Option<f64>,
    pub longtitude: Option<f64>,
    pub longitude: Option<f64>,
    #[serde(rename = "chargingPoints")]
    pub charging_points: Vec<WireEvChargingPoint>,
}

impl From<WireEvLocation> for RawEvLocation {
    fn from(loc: WireEvLocation) -> Self {
        RawEvLocation {
            name: loc.name,
            address: loc.address,
            latitude: loc.latitude,
            longtitude: loc.longtitude,
            longitude: loc.longitude,
            charging_points: loc
                .charging_points
                .into_iter()
                .map(|p| EvChargingPoint {
                    operator: p.operator,
                    plug_types: p
                        .plug_types
                        .into_iter()
                        .map(|t| EvPlugType {
                            plug_type: t.plug_type,
                            current: t.current,
                            power_rating: t.power_rating,
                            ev_ids: t.ev_ids.into_iter().map(|e| EvId { status: e.status }).collect(),
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}

#[derive(Deserialize)]
pub struct WireEvBatchFile {
    #[serde(rename = "evLocationsData")]
    pub ev_locations_data: Vec<WireEvLocation>,
}

impl From<WireEvBatchFile> for LtaEvBatchFile {
    fn from(file: WireEvBatchFile) -> Self {
        LtaEvBatchFile {
            ev_locations_data: file.ev_locations_data.into_iter().map(Into::into).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_shapes_match_upstream_fixtures() {
        let body = include_str!("../tests/fixtures/lta_cameras_grouped.json");
        let cameras: WireTrafficImagesResponse = serde_json::from_str(body).unwrap();
        assert_eq!(cameras.into_cameras().len(), 3);

        let body = include_str!("../tests/fixtures/lta_carparks.json");
        let rows: WireCarparkResponse = serde_json::from_str(body).unwrap();
        assert_eq!(rows.value.len(), 4);

        let body = include_str!("../tests/fixtures/lta_ev_batch.json");
        let file: WireEvBatchFile = serde_json::from_str(body).unwrap();
        let core: LtaEvBatchFile = file.into();
        assert_eq!(core.ev_locations_data.len(), 2);
    }
}
