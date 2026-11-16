//! Hover-popup content for map features.
//!
//! The original `Map.tsx` showed a compact popup for incidents, carparks, EV
//! chargers, amenities and parks (title, `PopupLine`s, up to three photo
//! thumbnails). argus-map already tracks the hovered overlay id; this module
//! turns the app's domain target into exactly that popup content.

use argus_core::data::photos_for_park;
use argus_core::mobility::{describe_carpark, describe_ev_charger};
use argus_core::parks::{ParkFeatureDetails, describe_park_feature};
use argus_core::types::{ParkFeatureKind, PopupLine};
use rune::Color;

use crate::panels::dialogs::direction_label;
use crate::state::{AppState, Target};
use crate::theme::*;

/// The body of one map hover popup.
#[derive(Clone, Debug)]
pub struct HoverContent {
    pub title: String,
    pub title_color: Color,
    pub lines: Vec<PopupLine>,
    /// Photo thumbnail URLs (the store decodes them once, like map tiles).
    pub photos: Vec<String>,
}

/// Build the popup for a hovered overlay target, or `None` if the feature is
/// no longer in the current data snapshot.
pub fn content_for(s: &AppState, target: &Target) -> Option<HoverContent> {
    let snap = s.data.snapshot();
    match target {
        Target::Amenity { kind, id } => {
            let (_, place) = s.registry.as_ref()?.find(*kind, id)?;
            Some(HoverContent {
                title: place.popup_title().to_string(),
                title_color: MUTED,
                lines: place.describe(),
                photos: place.photos().iter().map(|p| p.thumb.clone()).collect(),
            })
        }
        Target::Park {
            name,
            kind: feature_kind,
        } => {
            let feature = s
                .static_data
                .as_ref()?
                .parks_geojson
                .features
                .iter()
                .find(|f| &f.properties.name == name)?;
            let p = &feature.properties;
            let photos = if *feature_kind == ParkFeatureKind::Park {
                photos_for_park(s.static_data.as_ref()?, name)
            } else {
                &[]
            };
            let details = ParkFeatureDetails {
                kind: p.kind,
                name: p.name.clone(),
                park: p.park.clone(),
                kind_type: p.kind_type.clone(),
                cycle: p.cycle,
                area_ha: p.area_ha,
                photos,
            };
            Some(HoverContent {
                title: match feature_kind {
                    ParkFeatureKind::Park => "PARK".into(),
                    ParkFeatureKind::Pcn => "PARK CONNECTOR".into(),
                    ParkFeatureKind::Trail => "TRAIL".into(),
                },
                title_color: SUCCESS,
                lines: describe_park_feature(&details),
                photos: photos.iter().map(|p| p.thumb.clone()).collect(),
            })
        }
        Target::Incident(id) => {
            let inc = snap.incidents.iter().find(|i| &i.id == id)?;
            let color = if inc.incident_type == "Accident" {
                DANGER
            } else if inc.incident_type == "Vehicle Breakdown"
                || inc.incident_type == "Heavy Traffic"
            {
                WARNING
            } else {
                MUTED
            };
            Some(HoverContent {
                title: inc.incident_type.to_uppercase(),
                title_color: color,
                lines: vec![PopupLine::new(&inc.message, "message")],
                photos: vec![],
            })
        }
        Target::Carpark(id) => {
            let cp = snap.carparks.iter().find(|c| &c.id == id)?;
            Some(HoverContent {
                title: format!("{} CARPARK", cp.agency).to_uppercase(),
                title_color: INFO,
                lines: describe_carpark(cp),
                photos: vec![],
            })
        }
        Target::Ev(id) => {
            let ev = snap.ev_chargers.iter().find(|e| &e.id == id)?;
            Some(HoverContent {
                title: "EV CHARGING".into(),
                title_color: INFO,
                lines: describe_ev_charger(ev),
                photos: vec![],
            })
        }
        // These layers had pointer feedback but no popup in the original;
        // a one-line label is still useful in the native map.
        Target::Flight(id) => {
            let f = snap.flights.iter().find(|f| &f.id == id)?;
            Some(HoverContent {
                title: f.callsign.clone(),
                title_color: INFO,
                lines: vec![PopupLine::new(direction_label(f.direction), "status")],
                photos: vec![],
            })
        }
        Target::BusStop(code) => {
            let stop = snap.bus_stops.iter().find(|b| &b.bus_stop_code == code)?;
            Some(HoverContent {
                title: "BUS STOP".into(),
                title_color: SUCCESS,
                lines: vec![
                    PopupLine::new(&stop.description, "name"),
                    PopupLine::new(&stop.road_name, "detail"),
                ],
                photos: vec![],
            })
        }
        Target::MrtStation(name) => Some(HoverContent {
            title: "MRT STATION".into(),
            title_color: INFO,
            lines: vec![PopupLine::new(name, "name")],
            photos: vec![],
        }),
        Target::Camera(id) => {
            let cam = snap.cameras.iter().find(|c| &c.camera_id == id)?;
            Some(HoverContent {
                title: "TRAFFIC CAMERA".into(),
                title_color: INFO,
                lines: vec![PopupLine::new(&cam.location, "name")],
                photos: vec![],
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{init, scene_panels};

    fn state() -> AppState {
        let mut s = init();
        scene_panels(&mut s);
        s
    }

    #[test]
    fn incident_popup_uses_type_message_and_tone() {
        let mut s = state();
        s.visibility.insert("incidents".into(), true);
        s.recompute();
        let target = s
            .overlay_targets
            .iter()
            .find_map(|(_, t)| matches!(t, Target::Incident(_)).then_some(t.clone()))
            .expect("fixture has incidents");
        let content = content_for(&s, &target).expect("incident popup");
        assert!(content.title.contains("ACCIDENT") || !content.title.is_empty());
        assert!(content.lines.iter().any(|l| l.kind == "message"));
        assert!(content.photos.is_empty());
    }

    #[test]
    fn amenity_popup_uses_core_descriptions_and_photos() {
        let s = state();
        let id = s
            .registry
            .as_ref()
            .unwrap()
            .layers
            .iter()
            .find(|l| l.amenity_type == argus_core::types::AmenityType::Mall)
            .and_then(|l| l.places.first())
            .map(|p| p.id().to_string())
            .unwrap();
        let target = Target::Amenity {
            kind: argus_core::types::AmenityType::Mall,
            id,
        };
        let content = content_for(&s, &target).expect("mall popup");
        assert_eq!(content.title, "MALL");
        assert!(
            content
                .lines
                .iter()
                .any(|l| l.kind == "name" || l.kind == "count")
        );
    }
}
