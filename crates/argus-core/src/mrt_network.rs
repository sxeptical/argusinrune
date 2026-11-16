//! MRT/LRT station lists for routing and map display.
//! Ported from `lib/mrt-network.ts`.

use std::sync::LazyLock;

fn stations<const N: usize>(items: [&str; N]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

pub static LRT_LINE_STATIONS: LazyLock<Vec<(String, Vec<String>)>> = LazyLock::new(|| {
    vec![
        (
            "Bukit Panjang LRT".into(),
            stations([
                "Choa Chu Kang", "South View", "Keat Hong", "Teck Whye", "Phoenix",
                "Bukit Panjang", "Petir", "Pending", "Bangkit", "Fajar", "Segar",
                "Jelapang", "Senja", "Bukit Panjang",
            ]),
        ),
        (
            "Sengkang LRT East Loop".into(),
            stations(["Sengkang", "Compassvale", "Rumbia", "Bakau", "Kangkar", "Ranggung", "Sengkang"]),
        ),
        (
            "Sengkang LRT West Loop".into(),
            stations([
                "Sengkang", "Cheng Lim", "Farmway", "Kupang", "Thanggam", "Fernvale",
                "Layar", "Tongkang", "Renjong", "Sengkang",
            ]),
        ),
        (
            "Punggol LRT East Loop".into(),
            stations([
                "Punggol", "Cove", "Meridian", "Coral Edge", "Riviera", "Kadaloor",
                "Oasis", "Damai", "Punggol",
            ]),
        ),
        (
            "Punggol LRT West Loop".into(),
            stations([
                "Punggol", "Sam Kee", "Teck Lee", "Punggol Point", "Samudera", "Nibong",
                "Sumang", "Soo Teck", "Punggol",
            ]),
        ),
    ]
});

pub static MRT_OPERATIONAL_LINE_STATIONS: LazyLock<Vec<(String, Vec<String>)>> =
    LazyLock::new(|| {
        let mut lines = vec![
            (
                "North South Line".to_string(),
                stations([
                    "Jurong East", "Bukit Batok", "Bukit Gombak", "Choa Chu Kang", "Yew Tee",
                    "Kranji", "Marsiling", "Woodlands", "Admiralty", "Sembawang", "Canberra",
                    "Yishun", "Khatib", "Yio Chu Kang", "Ang Mo Kio", "Bishan", "Braddell",
                    "Toa Payoh", "Novena", "Newton", "Orchard", "Somerset", "Dhoby Ghaut",
                    "City Hall", "Raffles Place", "Marina Bay", "Marina South Pier",
                ]),
            ),
            (
                "East West Line".to_string(),
                stations([
                    "Tuas Link", "Tuas West Road", "Tuas Crescent", "Gul Circle", "Joo Koon",
                    "Pioneer", "Boon Lay", "Lakeside", "Chinese Garden", "Jurong East",
                    "Clementi", "Dover", "Buona Vista", "Commonwealth", "Queenstown", "Redhill",
                    "Tiong Bahru", "Outram Park", "Tanjong Pagar", "Raffles Place", "City Hall",
                    "Bugis", "Lavender", "Kallang", "Aljunied", "Paya Lebar", "Eunos",
                    "Kembangan", "Bedok", "Tanah Merah", "Simei", "Tampines", "Pasir Ris",
                ]),
            ),
            (
                "Changi Airport Branch".to_string(),
                stations(["Tanah Merah", "Expo", "Changi Airport"]),
            ),
            (
                "North East Line".to_string(),
                stations([
                    "HarbourFront", "Outram Park", "Chinatown", "Clarke Quay", "Dhoby Ghaut",
                    "Little India", "Farrer Park", "Boon Keng", "Potong Pasir", "Woodleigh",
                    "Serangoon", "Kovan", "Hougang", "Buangkok", "Sengkang", "Punggol",
                    "Punggol Coast",
                ]),
            ),
            (
                "Circle Line".to_string(),
                stations([
                    "Dhoby Ghaut", "Bras Basah", "Esplanade", "Promenade", "Nicoll Highway",
                    "Stadium", "Mountbatten", "Dakota", "Paya Lebar", "MacPherson", "Tai Seng",
                    "Bartley", "Serangoon", "Lorong Chuan", "Bishan", "Marymount", "Caldecott",
                    "Botanic Gardens", "Farrer Road", "Holland Village", "Buona Vista",
                    "one-north", "Kent Ridge", "Haw Par Villa", "Pasir Panjang", "Labrador Park",
                    "Telok Blangah", "HarbourFront",
                ]),
            ),
            (
                "Circle Line Extension".to_string(),
                stations(["HarbourFront", "Keppel", "Cantonment", "Prince Edward Road", "Marina Bay"]),
            ),
            (
                "Downtown Line".to_string(),
                stations([
                    "Bukit Panjang", "Cashew", "Hillview", "Hume", "Beauty World",
                    "King Albert Park", "Sixth Avenue", "Tan Kah Kee", "Botanic Gardens",
                    "Stevens", "Newton", "Little India", "Rochor", "Bugis", "Promenade",
                    "Bayfront", "Downtown", "Telok Ayer", "Chinatown", "Fort Canning",
                    "Bencoolen", "Jalan Besar", "Bendemeer", "Geylang Bahru", "Mattar",
                    "MacPherson", "Ubi", "Kaki Bukit", "Bedok North", "Bedok Reservoir",
                    "Tampines West", "Tampines", "Tampines East", "Upper Changi", "Expo",
                ]),
            ),
            (
                "Thomson-East Coast Line".to_string(),
                stations([
                    "Woodlands North", "Woodlands", "Woodlands South", "Springleaf", "Lentor",
                    "Mayflower", "Bright Hill", "Upper Thomson", "Caldecott", "Stevens", "Napier",
                    "Orchard Boulevard", "Orchard", "Great World", "Havelock", "Outram Park",
                    "Maxwell", "Shenton Way", "Marina Bay", "Gardens by the Bay", "Tanjong Rhu",
                    "Katong Park", "Tanjong Katong", "Marine Parade", "Marine Terrace", "Siglap",
                    "Bayshore",
                ]),
            ),
        ];
        lines.extend(LRT_LINE_STATIONS.clone());
        lines
    });

/// Future / non-routeable extensions shown on the map only.
pub static MRT_FUTURE_LINE_STATIONS: LazyLock<Vec<(String, Vec<String>)>> = LazyLock::new(|| {
    vec![
        (
            "Thomson-East Coast Line Extension".into(),
            stations(["Bayshore", "Bedok South", "Sungei Bedok"]),
        ),
        (
            "Downtown Line Extension".into(),
            stations(["Expo", "Xilin", "Sungei Bedok"]),
        ),
        (
            "Jurong Region Line".into(),
            stations([
                "Choa Chu Kang", "Choa Chu Kang West", "JS2A", "Tengah", "Hong Kah",
                "Corporation", "Jurong West", "Bahar Junction", "Boon Lay", "Enterprise",
                "Tukang", "Jurong Hill", "Jurong Pier",
            ]),
        ),
        (
            "Jurong Region Line West Branch".into(),
            stations([
                "Bahar Junction", "Gek Poh", "Tawas", "Nanyang Gateway", "Nanyang Crescent",
                "Peng Kang Hill",
            ]),
        ),
        (
            "Jurong Region Line East Branch".into(),
            stations([
                "Tengah", "Tengah Plantation", "Tengah Park", "Bukit Batok West", "Toh Guan",
                "Jurong East", "Jurong Town Hall", "Pandan Reservoir",
            ]),
        ),
        (
            "Jurong Region Line West Coast Extension".into(),
            stations(["Pandan Reservoir", "West Coast", "Kent Ridge"]),
        ),
        (
            "Cross Island Line".into(),
            stations([
                "Aviation Park", "Loyang", "Pasir Ris", "Tampines North", "Defu", "Hougang",
                "Serangoon North", "Tavistock", "Ang Mo Kio", "Teck Ghee", "Bright Hill",
                "Turf City", "King Albert Park", "Clementi", "West Coast", "Jurong Lake District",
                "CR20 (Taman Jurong)", "CR21 (Jurong Pier)", "CR22 (Jurong Industrial Estate)",
                "CR23 (Gul Circle)",
            ]),
        ),
        (
            "Johor Bahru-Singapore RTS".into(),
            stations(["Bukit Chagar", "Woodlands North"]),
        ),
    ]
});

/// Operational lines plus future extensions for map display.
pub static MRT_DISPLAY_LINE_STATIONS: LazyLock<Vec<(String, Vec<String>)>> = LazyLock::new(|| {
    let mut lines = MRT_OPERATIONAL_LINE_STATIONS.clone();
    lines.extend(MRT_FUTURE_LINE_STATIONS.clone());
    lines
});

pub static MRT_OPERATIONAL_LINE_COUNT: LazyLock<usize> =
    LazyLock::new(|| MRT_OPERATIONAL_LINE_STATIONS.len());
pub static MRT_DISPLAY_LINE_COUNT: LazyLock<usize> =
    LazyLock::new(|| MRT_DISPLAY_LINE_STATIONS.len());

/// Look up a line's stations in the display network.
pub fn display_stations(line: &str) -> Option<&'static [String]> {
    MRT_DISPLAY_LINE_STATIONS
        .iter()
        .find(|(name, _)| name == line)
        .map(|(_, s)| s.as_slice())
}

/// LTA line codes to display names; unmapped lines keep their raw codes.
pub fn lta_line_display_name(code: &str) -> String {
    let name = match code {
        "NSL" => "North South Line",
        "EWL" => "East West Line",
        "NEL" => "North East Line",
        "CCL" => "Circle Line",
        "CEL" => "Circle Line Extension",
        "CGL" => "Changi Airport Branch",
        "DTL" => "Downtown Line",
        "TEL" => "Thomson-East Coast Line",
        "JRL" => "Jurong Region Line",
        "CRL" => "Cross Island Line",
        "BPL" => "Bukit Panjang LRT",
        "SEL" => "Sengkang LRT East Loop",
        "SWL" => "Sengkang LRT West Loop",
        "PEL" => "Punggol LRT East Loop",
        "PWL" => "Punggol LRT West Loop",
        _ => code,
    };
    name.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mrt_routing::is_routeable_mrt_station;

    #[test]
    fn future_extensions_are_display_only_and_not_routeable() {
        assert_eq!(
            display_stations("Thomson-East Coast Line Extension").unwrap(),
            ["Bayshore", "Bedok South", "Sungei Bedok"]
        );
        assert!(!is_routeable_mrt_station("Bedok South"));
        assert!(!is_routeable_mrt_station("Sungei Bedok"));
        assert!(is_routeable_mrt_station("Bayshore"));
        assert!(!is_routeable_mrt_station("Xilin"));
        assert!(is_routeable_mrt_station("Expo"));
    }

    #[test]
    fn station_totals_match_lta() {
        let count = |lines: &[&str]| -> usize {
            lines
                .iter()
                .flat_map(|line| display_stations(line).unwrap().iter())
                .collect::<std::collections::BTreeSet<_>>()
                .len()
        };
        assert_eq!(count(&["Thomson-East Coast Line", "Thomson-East Coast Line Extension"]), 29);
        assert_eq!(count(&["Downtown Line", "Downtown Line Extension"]), 37);
    }

    #[test]
    fn lrt_loops_close_on_interchange() {
        for (line, stops) in LRT_LINE_STATIONS.iter() {
            assert_eq!(MRT_OPERATIONAL_LINE_STATIONS.iter().find(|(n, _)| n == line).unwrap().1, *stops);
            for stop in stops {
                assert!(is_routeable_mrt_station(stop));
            }
        }
        for line in [
            "Sengkang LRT East Loop", "Sengkang LRT West Loop",
            "Punggol LRT East Loop", "Punggol LRT West Loop",
        ] {
            let stops = display_stations(line).unwrap();
            assert_eq!(stops[0], *stops.last().unwrap());
        }
        let bpl = display_stations("Bukit Panjang LRT").unwrap();
        assert_eq!(bpl[0], "Choa Chu Kang");
        assert_eq!(bpl.iter().filter(|s| **s == "Bukit Panjang").count(), 2);
    }
}
