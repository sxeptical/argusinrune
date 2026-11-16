//! Keyless Google Maps links for point markers.
//! Ported from `lib/maps-links.ts`.

const MAPS_SEARCH_URL: &str = "https://www.google.com/maps/search/?api=1";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GoogleMapsTarget {
    pub name: Option<String>,
    pub address: Option<String>,
    pub lat: Option<f64>,
    pub lng: Option<f64>,
    /// Region appended to a name-only search. Defaults to "Singapore".
    pub region: Option<String>,
}

fn clean_text(value: Option<&str>) -> Option<String> {
    let cleaned = value?.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.is_empty() { None } else { Some(cleaned) }
}

/// Six decimals is roughly 0.1 m, plenty for a marker pin. Trailing zeroes
/// are dropped (matching `String(Number.parseFloat(value.toFixed(6)))`).
fn format_coord(value: f64) -> String {
    let rounded = (value * 1e6).round() / 1e6;
    if rounded.fract() == 0.0 {
        format!("{}", rounded as i64)
    } else {
        let s = format!("{:.6}", rounded);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

pub fn format_coordinates(lat: Option<f64>, lng: Option<f64>) -> Option<String> {
    let (lat, lng) = (lat?, lng?);
    if !lat.is_finite() || !lng.is_finite() {
        return None;
    }
    if lat.abs() > 90.0 || lng.abs() > 180.0 {
        return None;
    }
    Some(format!("{},{}", format_coord(lat), format_coord(lng)))
}

/// Percent-encode a query string the way `encodeURIComponent` does.
fn encode_uri_component(input: &str) -> String {
    let mut out = String::new();
    for &b in input.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

pub fn google_maps_url(target: &GoogleMapsTarget) -> Option<String> {
    let place = clean_text(target.name.as_deref());
    let street = clean_text(target.address.as_deref());
    let coords = format_coordinates(target.lat, target.lng);
    let region_label = clean_text(Some(target.region.as_deref().unwrap_or("Singapore")));

    let query = match (&place, &street, &coords) {
        (Some(p), Some(s), _) => format!("{}, {}", p, s),
        (_, Some(s), _) => s.clone(),
        (_, None, Some(c)) => c.clone(),
        (Some(p), None, None) => match region_label {
            Some(r) => format!("{}, {}", p, r),
            None => p.clone(),
        },
        (None, None, None) => return None,
    };
    if query.is_empty() {
        return None;
    }
    Some(format!("{}&query={}", MAPS_SEARCH_URL, encode_uri_component(&query)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_name_and_address_and_encodes() {
        let url = google_maps_url(&GoogleMapsTarget {
            name: Some("Tampines ActiveSG Gym".into()),
            address: Some("1 Tampines Walk, Singapore 528523".into()),
            lat: Some(1.3531), lng: Some(103.9404), region: None,
        }).unwrap();
        assert_eq!(url, "https://www.google.com/maps/search/?api=1&query=Tampines%20ActiveSG%20Gym%2C%201%20Tampines%20Walk%2C%20Singapore%20528523");
        let url = google_maps_url(&GoogleMapsTarget {
            name: Some("Ya Kun Kaya Toast @ Marina Bay Link Mall".into()),
            address: Some("8A Marina Boulevard #B2-52, Singapore 018984".into()),
            lat: None, lng: None, region: None,
        }).unwrap();
        assert!(url.contains("%40") && url.contains("%23B2-52"));
    }

    #[test]
    fn falls_back_to_coords_or_name_region() {
        let url = google_maps_url(&GoogleMapsTarget {
            name: Some("24/7 Fitness".into()), address: Some("   ".into()),
            lat: Some(1.36134), lng: Some(103.84155), region: None,
        }).unwrap();
        assert!(url.ends_with("1.36134%2C103.84155"));
        let url = google_maps_url(&GoogleMapsTarget {
            name: Some("Adam Park".into()), lat: None, lng: None, ..Default::default()
        }).unwrap();
        assert!(url.ends_with("Adam%20Park%2C%20Singapore"));
        assert!(google_maps_url(&GoogleMapsTarget::default()).is_none());
        assert!(google_maps_url(&GoogleMapsTarget { lat: Some(f64::NAN), lng: Some(103.8), ..Default::default() }).is_none());
    }

    #[test]
    fn format_coordinates_rounds_and_rejects() {
        assert_eq!(format_coordinates(Some(1.3500004), Some(103.9400001)).unwrap(), "1.35,103.94");
        assert_eq!(format_coordinates(Some(1.23456789), Some(103.98765432)).unwrap(), "1.234568,103.987654");
        assert!(format_coordinates(None, Some(103.8)).is_none());
        assert!(format_coordinates(Some(-91.0), Some(103.8)).is_none());
    }
}
