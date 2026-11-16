//! Popup copy for the remaining static point-amenity layers.
//! Ported from `lib/{gyms,schools,heritage,healthcare,hotels,libraries,pharmacies,
//! supermarkets,community-clubs}.ts`.

use crate::amenities::{format_opening_hours, photo_credit};
use crate::format::group_thousands;
use crate::types::*;

// --------------------------------------------------------------------------- gyms

pub fn gym_category_label(category: GymCategory) -> &'static str {
    match category {
        GymCategory::Gym => "Gym",
        GymCategory::Yoga => "Yoga studio",
        GymCategory::Pilates => "Pilates studio",
        GymCategory::MartialArts => "Martial arts gym",
        GymCategory::Crossfit => "CrossFit gym",
        GymCategory::Climbing => "Climbing gym",
        GymCategory::Public => "Public gym",
    }
}

pub fn gym_source_hint(source: GymSource) -> &'static str {
    match source {
        GymSource::Alltheplaces => "Source: All The Places (CC0)",
        GymSource::Osm => "Location: © OpenStreetMap contributors (ODbL)",
        GymSource::Sportsg => "Source: SportSG via data.gov.sg (Singapore Open Data Licence)",
        GymSource::Curated => "Source: ActiveSG facility directory",
    }
}

pub fn describe_gym(gym: &Gym) -> Vec<PopupLine> {
    let category = gym_category_label(gym.category);
    let hours = gym.opening_hours.as_deref().and_then(format_opening_hours);
    let brand = if gym.brand == "Independent" || gym.brand == gym.name {
        None
    } else {
        Some(gym.brand.as_str())
    };
    let mut lines = vec![
        PopupLine::new(&gym.name, "name"),
        PopupLine::toned(
            match brand {
                Some(b) => format!("{} · {}", b, category),
                None => category.into(),
            },
            "status",
            AvailabilityTone::Muted,
        ),
    ];
    if let Some(hours) = hours {
        lines.push(PopupLine::new(hours, "detail"));
    }
    if let Some(address) = &gym.address
        && !address.is_empty()
    {
        lines.push(PopupLine::new(address, "detail"));
    }
    lines.push(PopupLine::new(gym_source_hint(gym.source), "hint"));
    lines
}

// --------------------------------------------------------------------------- schools

pub fn school_level_label(level: SchoolLevel) -> &'static str {
    match level {
        SchoolLevel::Primary => "Primary",
        SchoolLevel::Secondary => "Secondary",
        SchoolLevel::Jc => "JC",
        SchoolLevel::Mixed => "Mixed",
        SchoolLevel::Polytechnic => "Polytechnic",
        SchoolLevel::University => "University",
    }
}

pub fn school_kind_label(level: SchoolLevel) -> &'static str {
    match level {
        SchoolLevel::Primary => "Primary school",
        SchoolLevel::Secondary => "Secondary school",
        SchoolLevel::Jc => "JC school",
        SchoolLevel::Mixed => "Mixed school",
        SchoolLevel::Polytechnic => "Polytechnic",
        SchoolLevel::University => "University",
    }
}

pub fn is_higher_learning(level: SchoolLevel) -> bool {
    matches!(level, SchoolLevel::Polytechnic | SchoolLevel::University)
}

pub fn school_source_hint(school: &School) -> &'static str {
    if is_higher_learning(school.level) {
        "Source: curated list of autonomous universities and polytechnics; location via OneMap"
    } else {
        "Source: Ministry of Education via data.gov.sg (Singapore Open Data Licence)"
    }
}

pub fn describe_school(school: &School) -> Vec<PopupLine> {
    let mut lines = vec![
        PopupLine::new(&school.name, "name"),
        PopupLine::toned(
            school_kind_label(school.level),
            "status",
            AvailabilityTone::Muted,
        ),
    ];
    if let Some(address) = &school.address
        && !address.is_empty()
    {
        lines.push(PopupLine::new(address, "detail"));
    }
    if let Some(phone) = &school.phone
        && !phone.is_empty()
    {
        lines.push(PopupLine::new(format!("Tel {}", phone), "detail"));
    }
    if let Some(mrt) = &school.mrt
        && !mrt.is_empty()
    {
        lines.push(PopupLine::new(format!("MRT: {}", mrt), "detail"));
    }
    if let Some(bus) = &school.bus
        && !bus.is_empty()
    {
        lines.push(PopupLine::new(format!("Bus: {}", bus), "detail"));
    }
    lines.push(PopupLine::new(school_source_hint(school), "hint"));
    lines
}

// --------------------------------------------------------------------------- heritage

pub fn heritage_kind_label(kind: HeritageKind) -> &'static str {
    match kind {
        HeritageKind::HistoricSite => "Historic site",
        HeritageKind::Monument => "National monument",
        HeritageKind::Museum => "Museum",
    }
}

fn heritage_source_hint(source: HeritageSource) -> &'static str {
    match source {
        HeritageSource::Nhb => {
            "Source: National Heritage Board via data.gov.sg (Singapore Open Data Licence)"
        }
        HeritageSource::Osm => "Location: © OpenStreetMap contributors (ODbL)",
    }
}

pub fn describe_heritage(site: &HeritageSite) -> Vec<PopupLine> {
    let mut lines = vec![
        PopupLine::new(&site.name, "name"),
        PopupLine::toned(
            heritage_kind_label(site.kind),
            "status",
            AvailabilityTone::Muted,
        ),
    ];
    if let Some(desc) = &site.description
        && !desc.is_empty()
    {
        lines.push(PopupLine::new(desc, "detail"));
    }
    if let Some(address) = &site.address
        && !address.is_empty()
    {
        lines.push(PopupLine::new(address, "detail"));
    }
    if let Some(credit) = photo_credit(&site.photos) {
        lines.push(PopupLine::new(credit, "hint"));
    }
    lines.push(PopupLine::new(heritage_source_hint(site.source), "hint"));
    lines
}

// --------------------------------------------------------------------------- healthcare

pub fn healthcare_kind_label(kind: HealthcareKind) -> &'static str {
    match kind {
        HealthcareKind::Gp => "GP clinic",
        HealthcareKind::Hospital => "Hospital",
        HealthcareKind::Dental => "Dental clinic",
    }
}

fn healthcare_source_hint(source: HealthcareSource) -> &'static str {
    match source {
        HealthcareSource::Chas => {
            "Source: MOH CHAS Clinics via data.gov.sg (Singapore Open Data Licence) · data as of 2021"
        }
        HealthcareSource::Osm => "Location: © OpenStreetMap contributors (ODbL)",
    }
}

pub fn describe_healthcare(site: &HealthcareSite) -> Vec<PopupLine> {
    let mut lines = vec![
        PopupLine::new(&site.name, "name"),
        PopupLine::toned(
            healthcare_kind_label(site.kind),
            "status",
            AvailabilityTone::Muted,
        ),
    ];
    if !site.programmes.is_empty() {
        lines.push(PopupLine::new(
            format!("Schemes: {}", site.programmes.join(" · ")),
            "detail",
        ));
    }
    if let Some(phone) = &site.phone
        && !phone.is_empty()
    {
        lines.push(PopupLine::new(format!("Tel {}", phone), "detail"));
    }
    if let Some(address) = &site.address
        && !address.is_empty()
    {
        lines.push(PopupLine::new(address, "detail"));
    }
    if let Some(credit) = photo_credit(&site.photos) {
        lines.push(PopupLine::new(credit, "hint"));
    }
    lines.push(PopupLine::new(healthcare_source_hint(site.source), "hint"));
    lines
}

// --------------------------------------------------------------------------- hotels

const HOTEL_SOURCE_HINT: &str =
    "Source: Hotels Licensing Board via data.gov.sg (Singapore Open Data Licence)";

pub fn describe_hotel(hotel: &Hotel) -> Vec<PopupLine> {
    let mut lines = vec![
        PopupLine::new(&hotel.name, "name"),
        PopupLine::toned("Licensed hotel", "status", AvailabilityTone::Muted),
    ];
    if let Some(rooms) = hotel.rooms {
        lines.push(PopupLine::new(
            format!("{} licensed rooms", group_thousands(rooms)),
            "count",
        ));
    }
    if let Some(address) = &hotel.address
        && !address.is_empty()
    {
        lines.push(PopupLine::new(address, "detail"));
    }
    if let Some(credit) = photo_credit(&hotel.photos) {
        lines.push(PopupLine::new(credit, "hint"));
    }
    lines.push(PopupLine::new(HOTEL_SOURCE_HINT, "hint"));
    lines
}

// --------------------------------------------------------------------------- libraries

fn library_source_hint(source: LibrarySource) -> &'static str {
    match source {
        LibrarySource::Nlb => {
            "Source: National Library Board via data.gov.sg (Singapore Open Data Licence) · data as of 2019"
        }
        LibrarySource::Osm => "Location: © OpenStreetMap contributors (ODbL)",
    }
}

pub fn describe_library(library: &PublicLibrary) -> Vec<PopupLine> {
    let mut lines = vec![
        PopupLine::new(&library.name, "name"),
        PopupLine::toned("Public library", "status", AvailabilityTone::Muted),
    ];
    if let Some(branch) = &library.branch
        && !branch.is_empty()
    {
        lines.push(PopupLine::new(format!("Branch {}", branch), "count"));
    }
    if let Some(address) = &library.address
        && !address.is_empty()
    {
        lines.push(PopupLine::new(address, "detail"));
    }
    if let Some(credit) = photo_credit(&library.photos) {
        lines.push(PopupLine::new(credit, "hint"));
    }
    lines.push(PopupLine::new(library_source_hint(library.source), "hint"));
    lines
}

// --------------------------------------------------------------------------- pharmacies

const PHARMACY_SOURCE_HINT: &str = "Source: HSA via data.gov.sg (Singapore Open Data Licence)";

pub fn describe_pharmacy(pharmacy: &Pharmacy) -> Vec<PopupLine> {
    let mut lines = vec![
        PopupLine::new(&pharmacy.name, "name"),
        PopupLine::toned("Retail pharmacy", "status", AvailabilityTone::Muted),
    ];
    if let Some(address) = &pharmacy.address
        && !address.is_empty()
    {
        lines.push(PopupLine::new(address, "detail"));
    }
    lines.push(PopupLine::new(PHARMACY_SOURCE_HINT, "hint"));
    lines
}

// --------------------------------------------------------------------------- supermarkets

const SUPERMARKET_SOURCE_HINT: &str = "Location: © OpenStreetMap contributors (ODbL)";

pub fn describe_supermarket(market: &Supermarket) -> Vec<PopupLine> {
    let hours = market
        .opening_hours
        .as_deref()
        .and_then(format_opening_hours);
    let brand = if market.brand == "Other" || market.brand == market.name {
        None
    } else {
        Some(market.brand.as_str())
    };
    let mut lines = vec![
        PopupLine::new(&market.name, "name"),
        PopupLine::toned(
            match brand {
                Some(b) => format!("{} supermarket", b),
                None => "Supermarket".into(),
            },
            "status",
            AvailabilityTone::Muted,
        ),
    ];
    if let Some(hours) = hours {
        lines.push(PopupLine::new(hours, "detail"));
    }
    if let Some(address) = &market.address
        && !address.is_empty()
    {
        lines.push(PopupLine::new(address, "detail"));
    }
    lines.push(PopupLine::new(SUPERMARKET_SOURCE_HINT, "hint"));
    lines
}

// --------------------------------------------------------------------------- community clubs

pub fn community_club_kind_label(kind: CommunityClubKind) -> &'static str {
    match kind {
        CommunityClubKind::CommunityClub => "Community club",
        CommunityClubKind::Passionwave => "PAssion WaVe outlet",
    }
}

const COMMUNITY_CLUB_SOURCE_HINT: &str =
    "Source: People's Association via data.gov.sg (Singapore Open Data Licence)";

pub fn describe_community_club(club: &CommunityClub) -> Vec<PopupLine> {
    let mut lines = vec![
        PopupLine::new(&club.name, "name"),
        PopupLine::toned(
            community_club_kind_label(club.kind),
            "status",
            AvailabilityTone::Muted,
        ),
    ];
    if let Some(address) = &club.address
        && !address.is_empty()
    {
        lines.push(PopupLine::new(address, "detail"));
    }
    if let Some(credit) = photo_credit(&club.photos) {
        lines.push(PopupLine::new(credit, "hint"));
    }
    lines.push(PopupLine::new(COMMUNITY_CLUB_SOURCE_HINT, "hint"));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(lines: &[PopupLine]) -> Vec<&str> {
        lines.iter().map(|l| l.text.as_str()).collect()
    }

    #[test]
    fn describes_gym_school_and_hotel() {
        let gym = Gym {
            id: "g".into(),
            name: "Marymount CC".into(),
            brand: "Anytime Fitness".into(),
            category: GymCategory::Gym,
            address: Some("191 Sin Ming Avenue".into()),
            lat: 1.3,
            lng: 103.8,
            opening_hours: Some("24/7".into()),
            website: None,
            aliases: None,
            source: GymSource::Alltheplaces,
        };
        assert_eq!(
            texts(&describe_gym(&gym)),
            [
                "Marymount CC",
                "Anytime Fitness · Gym",
                "Open 24 hours",
                "191 Sin Ming Avenue",
                "Source: All The Places (CC0)"
            ]
        );
        let school = School {
            id: "s".into(),
            name: "ADMIRALTY PRIMARY SCHOOL".into(),
            level: SchoolLevel::Primary,
            address: Some("11 WOODLANDS CIRCLE".into()),
            postal_code: None,
            phone: Some("+65 6362 0598".into()),
            website: None,
            mrt: Some("Admiralty Station".into()),
            bus: None,
            lat: 1.4,
            lng: 103.8,
            coords_source: CoordsSource::Onemap,
        };
        assert_eq!(texts(&describe_school(&school))[1], "Primary school");
        let hotel = Hotel {
            id: "h".into(),
            name: "Hotel Bencoolen".into(),
            address: Some("Singapore 189626".into()),
            rooms: Some(84),
            lat: 1.3,
            lng: 103.8,
            photos: vec![],
        };
        assert_eq!(texts(&describe_hotel(&hotel))[2], "84 licensed rooms");
    }
}
