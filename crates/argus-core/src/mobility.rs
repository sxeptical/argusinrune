//! Availability bands and popup copy for carpark / EV charger layers.
//! Ported from `lib/mobility.ts`.

use crate::types::{AvailabilityTone, CarparkAvailability, EvChargingLocation, PopupLine};

pub const CARPARK_LOW_LOTS: i64 = 20;

pub fn carpark_tone(carpark: &CarparkAvailability) -> AvailabilityTone {
    match carpark.car_lots {
        None => AvailabilityTone::Muted,
        Some(0) => AvailabilityTone::Danger,
        Some(n) if n <= CARPARK_LOW_LOTS => AvailabilityTone::Warning,
        Some(_) => AvailabilityTone::Success,
    }
}

pub fn ev_charger_tone(location: &EvChargingLocation) -> AvailabilityTone {
    if location.available > 0 {
        AvailabilityTone::Success
    } else if location.occupied > 0 {
        AvailabilityTone::Warning
    } else {
        AvailabilityTone::Muted
    }
}

pub fn plural(count: i64, one: &str, many: &str) -> String {
    format!("{} {}", count, if count == 1 { one } else { many })
}

fn carpark_status(tone: AvailabilityTone) -> &'static str {
    match tone {
        AvailabilityTone::Success => "Spaces available",
        AvailabilityTone::Warning => "Filling up",
        AvailabilityTone::Danger => "Full",
        AvailabilityTone::Muted => "No car lot data",
    }
}

fn free_lots(count: i64, vehicle: &str) -> String {
    if count == 0 {
        format!("No {} lots free", vehicle)
    } else {
        format!("{} free", plural(count, &format!("{} lot", vehicle), &format!("{} lots", vehicle)))
    }
}

pub fn describe_carpark(carpark: &CarparkAvailability) -> Vec<PopupLine> {
    let tone = carpark_tone(carpark);
    let mut lines = vec![
        PopupLine { text: carpark.name.clone(), kind: "name".into(), tone: None },
        PopupLine { text: carpark_status(tone).into(), kind: "status".into(), tone: Some(tone) },
    ];
    if let Some(lots) = carpark.car_lots {
        lines.push(PopupLine { text: free_lots(lots, "car"), kind: "count".into(), tone: None });
    }
    if let Some(lots) = carpark.motorcycle_lots {
        lines.push(PopupLine { text: free_lots(lots, "motorcycle"), kind: "detail".into(), tone: None });
    }
    if let Some(lots) = carpark.heavy_lots {
        lines.push(PopupLine { text: free_lots(lots, "heavy vehicle"), kind: "detail".into(), tone: None });
    }
    lines.push(PopupLine { text: "Live from LTA · updates every minute".into(), kind: "hint".into(), tone: None });
    lines
}

pub fn describe_ev_charger(location: &EvChargingLocation) -> Vec<PopupLine> {
    let tone = ev_charger_tone(location);
    let status = match tone {
        AvailabilityTone::Success => format!("{} of {} free", location.available, plural(location.total, "charger", "chargers")),
        AvailabilityTone::Warning => {
            if location.occupied == location.total {
                format!("All {} in use", plural(location.total, "charger", "chargers"))
            } else {
                format!("No free chargers · {} in use", location.occupied)
            }
        }
        AvailabilityTone::Muted | AvailabilityTone::Danger => "Availability not reported".into(),
    };
    let speed = location.max_power_kw.map(|kw| {
        format!(
            "Up to {} kW · {}",
            format_kw(kw),
            if location.has_dc { "DC fast charging" } else { "AC standard charging" }
        )
    });
    let mut lines = vec![
        PopupLine { text: location.name.clone(), kind: "name".into(), tone: None },
        PopupLine { text: status, kind: "status".into(), tone: Some(tone) },
    ];
    if let Some(speed) = speed {
        lines.push(PopupLine { text: speed, kind: "detail".into(), tone: None });
    }
    if !location.operators.is_empty() {
        lines.push(PopupLine { text: format!("Run by {}", location.operators.join(", ")), kind: "detail".into(), tone: None });
    }
    lines.push(PopupLine { text: "Live from LTA · updates every 5 minutes".into(), kind: "hint".into(), tone: None });
    lines
}

fn format_kw(kw: f64) -> String {
    if kw.fract() == 0.0 { format!("{}", kw as i64) } else { format!("{}", kw) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn carpark() -> CarparkAvailability {
        CarparkAvailability {
            id: "HDB:TPMN".into(), name: "BLK 78B".into(), area: String::new(), agency: "HDB".into(),
            lat: 1.33, lng: 103.84, car_lots: Some(100), motorcycle_lots: None, heavy_lots: None,
        }
    }
    fn charger() -> EvChargingLocation {
        EvChargingLocation {
            id: "abc".into(), name: "CLARKE QUAY".into(), address: String::new(),
            lat: 1.29, lng: 103.84, available: 1, occupied: 1, total: 3,
            max_power_kw: Some(50.0), has_dc: true, operators: Vec::new(), plug_types: Vec::new(),
        }
    }

    #[test]
    fn tones() {
        assert_eq!(carpark_tone(&CarparkAvailability { car_lots: Some(0), ..carpark() }), AvailabilityTone::Danger);
        assert_eq!(carpark_tone(&CarparkAvailability { car_lots: Some(20), ..carpark() }), AvailabilityTone::Warning);
        assert_eq!(carpark_tone(&CarparkAvailability { car_lots: Some(21), ..carpark() }), AvailabilityTone::Success);
        assert_eq!(ev_charger_tone(&charger()), AvailabilityTone::Success);
        assert_eq!(ev_charger_tone(&EvChargingLocation { available: 0, ..charger() }), AvailabilityTone::Warning);
    }

    #[test]
    fn descriptions() {
        let lines = describe_carpark(&CarparkAvailability { car_lots: Some(139), motorcycle_lots: Some(1), ..carpark() });
        assert_eq!(lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), [
            "BLK 78B", "Spaces available", "139 car lots free", "1 motorcycle lot free", "Live from LTA · updates every minute"
        ]);
        let lines = describe_ev_charger(&EvChargingLocation { operators: vec!["SP MOBILITY".into()], ..charger() });
        assert_eq!(lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), [
            "CLARKE QUAY", "1 of 3 chargers free", "Up to 50 kW · DC fast charging", "Run by SP MOBILITY", "Live from LTA · updates every 5 minutes"
        ]);
    }
}
