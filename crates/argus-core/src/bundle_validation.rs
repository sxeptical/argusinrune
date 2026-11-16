//! Shared checks for the static JSON bundles in `data/`.
//! Ported from `lib/bundle-validation.ts`. Each bundle is validated once at
//! load, so a bad regeneration fails fast instead of rendering broken markers.

use serde_json::Value;

/// `null` or a string.
pub fn is_nullable_string(value: &Value) -> bool {
    value.is_null() || value.is_string()
}

/// `null` or a valid non-negative whole number (counts, years).
pub fn is_nullable_non_negative_integer(value: &Value) -> bool {
    if value.is_null() {
        return true;
    }
    match value.as_i64() {
        Some(n) => n >= 0,
        None => value.as_f64().is_some_and(|n| n.fract() == 0.0 && n >= 0.0),
    }
}

/// `null` or an http(s) URL.
pub fn is_nullable_http_url(value: &Value) -> bool {
    match value.as_str() {
        None => value.is_null(),
        Some(s) => {
            let lower = s.to_ascii_lowercase();
            lower.starts_with("http://") || lower.starts_with("https://")
        }
    }
}

/// The fields every map place shares: a non-empty id and name and a position.
pub fn is_place(value: &Value) -> bool {
    let obj = match value.as_object() {
        Some(o) => o,
        None => return false,
    };
    let has_nonempty = |key: &str| -> bool {
        obj.get(key)
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty())
    };
    let finite = |key: &str| -> bool {
        obj.get(key)
            .and_then(Value::as_f64)
            .is_some_and(|n| n.is_finite())
    };
    has_nonempty("id") && has_nonempty("name") && finite("lat") && finite("lng")
}

/// `is_place` plus the nullable street address most bundles carry.
pub fn is_addressed_place(value: &Value) -> bool {
    is_place(value)
        && value
            .as_object()
            .is_some_and(|o| o.get("address").is_none_or(is_nullable_string))
}

pub fn is_one_of(options: &[&str], value: &Value) -> bool {
    value.as_str().is_some_and(|s| options.contains(&s))
}

const WIKIMEDIA_UPLOAD: &str = "https://upload.wikimedia.org/";
const WIKIMEDIA_THUMB: &str = "https://thumb.wikimedia.org/";
const NEA_MEDIA: &str = "https://www.nea.gov.sg/";
const COMMONS_PAGE: &str = "https://commons.wikimedia.org/";

/// Photos are hotlinked, so only from the CSP-allowed hosts.
pub fn is_photo(value: &Value) -> bool {
    let obj = match value.as_object() {
        Some(o) => o,
        None => return false,
    };
    let string = |key: &str| obj.get(key).and_then(Value::as_str);
    let (Some(thumb), Some(_author)) = (string("thumb"), string("author")) else {
        return false;
    };
    // `year` must be an integer when present.
    if let Some(year) = obj.get("year")
        && !year.is_null()
        && year.as_i64().is_none()
    {
        return false;
    }
    // `page` is rendered as an href: null or an https URL only.
    let page = obj.get("page");
    let page_ok = page.is_none_or(|p| {
        p.is_null()
            || p.as_str()
                .is_some_and(|s| s.to_ascii_lowercase().starts_with("https://"))
    });
    if !page_ok {
        return false;
    }
    match string("source") {
        Some("commons") => {
            let thumb_ok =
                thumb.starts_with(WIKIMEDIA_UPLOAD) || thumb.starts_with(WIKIMEDIA_THUMB);
            let license_ok = string("license").is_some_and(|s| !s.is_empty());
            let page_ok = page.is_none_or(|p| {
                p.is_null() || p.as_str().is_some_and(|s| s.starts_with(COMMONS_PAGE))
            });
            thumb_ok && license_ok && page_ok
        }
        Some("nea") => thumb.starts_with(NEA_MEDIA),
        _ => false,
    }
}

pub fn is_photo_list(value: &Value) -> bool {
    value.as_array().is_some_and(|arr| arr.iter().all(is_photo))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn place_and_url_checks() {
        let place = json!({"id": "p-1", "name": "Somewhere", "lat": 1.3, "lng": 103.8});
        assert!(is_place(&place));
        assert!(!is_place(
            &json!({"id": "", "name": "x", "lat": 1.3, "lng": 103.8})
        ));
        assert!(!is_place(
            &json!({"id": "x", "name": "x", "lat": null, "lng": 103.8})
        ));
        assert!(is_addressed_place(
            &json!({"id":"x","name":"x","lat":1.0,"lng":1.0,"address":null})
        ));
        assert!(is_nullable_http_url(&json!("https://example.com")));
        assert!(!is_nullable_http_url(&json!("javascript:alert(1)")));
        assert!(is_one_of(&["a", "b"], &json!("b")));
    }

    #[test]
    fn photo_hosts_and_license() {
        let commons = json!({
            "thumb": "https://thumb.wikimedia.org/a.jpg",
            "page": "https://commons.wikimedia.org/wiki/File:A.jpg",
            "author": "Terence Ong", "license": "CC BY 2.5", "source": "commons"
        });
        let nea = json!({"thumb": "https://www.nea.gov.sg/images/a.jpg", "page": null, "author": "NEA", "license": null, "source": "nea"});
        assert!(is_photo_list(&json!([commons, nea])));
        assert!(!is_photo_list(
            &json!([{"thumb":"https://evil.example/a.jpg","author":"x","page":null,"license":"CC0","source":"commons"}])
        ));
        assert!(!is_photo_list(
            &json!([{"thumb":"https://www.nea.gov.sg.evil.example/a.jpg","author":"x","page":null,"license":null,"source":"nea"}])
        ));
    }
}
