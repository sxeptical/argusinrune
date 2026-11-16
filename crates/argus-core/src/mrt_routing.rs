//! MRT route planning via Dijkstra over station×line nodes.
//! Ported from `lib/mrt-routing.ts`.

use std::collections::HashMap;
use std::sync::LazyLock;

use crate::mrt_network::MRT_OPERATIONAL_LINE_STATIONS;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MrtRouteSegment {
    pub line: String,
    pub from: String,
    pub to: String,
    pub stops: i64,
    /// Stations ridden in order, `from` to `to`; draws loop routes correctly.
    pub stations: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MrtRoutePlan {
    pub start: String,
    pub end: String,
    pub stations: Vec<String>,
    pub segments: Vec<MrtRouteSegment>,
    pub transfers: i64,
    pub estimated_minutes: f64,
}

pub struct MrtRouteDefaults {
    pub start: &'static str,
    pub end: &'static str,
}
pub const MRT_ROUTE_DEFAULTS: MrtRouteDefaults = MrtRouteDefaults { start: "", end: "" };

use serde::{Deserialize, Serialize};

fn minutes_per_stop(line: &str) -> f64 {
    match line {
        "North South Line" | "East West Line" => 2.5,
        "Changi Airport Branch" | "North East Line" => 3.0,
        "Circle Line" | "Circle Line Extension" => 2.0,
        "Downtown Line" => 1.5,
        "Thomson-East Coast Line" | "Thomson-East Coast Line Extension" => 2.5,
        "Bukit Panjang LRT"
        | "Sengkang LRT East Loop"
        | "Sengkang LRT West Loop"
        | "Punggol LRT East Loop"
        | "Punggol LRT West Loop" => 1.5,
        _ => 2.0,
    }
}

fn transfer_minutes(station: &str, line_count: usize) -> f64 {
    match station {
        "Dhoby Ghaut" | "Outram Park" => 6.0,
        "Marina Bay" => 4.0,
        _ => {
            if line_count >= 3 { 5.0 } else { 3.0 }
        }
    }
}

const ENTRY_EXIT_BUFFER_MINUTES: f64 = 2.0;

/// Minimal binary heap for Dijkstra — compares only on distance (like the TS
/// MinHeap), O(log V) push/pop.
#[derive(Clone, Debug)]
struct MinHeap {
    items: Vec<HeapEntry>,
}

#[derive(Clone, Debug)]
struct HeapEntry {
    key: String,
    distance: f64,
}

impl MinHeap {
    fn new() -> Self {
        Self { items: Vec::new() }
    }
    fn len(&self) -> usize {
        self.items.len()
    }
    fn push(&mut self, entry: HeapEntry) {
        self.items.push(entry);
        let mut i = self.items.len() - 1;
        while i > 0 {
            let parent = (i - 1) / 2;
            if self.items[parent].distance <= self.items[i].distance {
                break;
            }
            self.items.swap(parent, i);
            i = parent;
        }
    }
    fn pop(&mut self) -> Option<HeapEntry> {
        if self.items.is_empty() {
            return None;
        }
        let top = self.items[0].clone();
        let last = self.items.pop();
        if !self.items.is_empty() {
            if let Some(last) = last {
                self.items[0] = last;
                let mut i = 0;
                loop {
                    let left = i * 2 + 1;
                    let right = i * 2 + 2;
                    let mut smallest = i;
                    if left < self.items.len()
                        && self.items[left].distance < self.items[smallest].distance
                    {
                        smallest = left;
                    }
                    if right < self.items.len()
                        && self.items[right].distance < self.items[smallest].distance
                    {
                        smallest = right;
                    }
                    if smallest == i {
                        break;
                    }
                    self.items.swap(i, smallest);
                    i = smallest;
                }
            }
        }
        Some(top)
    }
}

#[derive(Clone, Debug)]
struct StationLineNode {
    station: String,
    line: String,
}

#[derive(Clone, Debug)]
struct Network {
    nodes: HashMap<String, StationLineNode>,
    station_to_keys: HashMap<String, Vec<String>>,
    graph: HashMap<String, Vec<(String, f64)>>,
}

impl Network {
    fn build() -> Self {
        let mut nodes: HashMap<String, StationLineNode> = HashMap::new();
        let mut station_to_keys: HashMap<String, Vec<String>> = HashMap::new();

        for (line, stops) in MRT_OPERATIONAL_LINE_STATIONS.iter() {
            for station in stops {
                let key = format!("{}::{}", station, line);
                if nodes.contains_key(&key) {
                    continue;
                }
                nodes.insert(
                    key.clone(),
                    StationLineNode {
                        station: station.clone(),
                        line: line.clone(),
                    },
                );
                station_to_keys
                    .entry(station.clone())
                    .or_default()
                    .push(key);
            }
        }

        let mut graph: HashMap<String, Vec<(String, f64)>> = HashMap::new();
        let mut add_edge = |from: String, to: String, weight: f64| {
            graph.entry(from).or_default().push((to, weight));
        };
        for (line, stops) in MRT_OPERATIONAL_LINE_STATIONS.iter() {
            for pair in stops.windows(2) {
                let from = format!("{}::{}", pair[0], line);
                let to = format!("{}::{}", pair[1], line);
                let weight = minutes_per_stop(line);
                add_edge(from.clone(), to.clone(), weight);
                add_edge(to, from, weight);
            }
        }
        for (station, keys) in station_to_keys.iter() {
            let weight = transfer_minutes(station, keys.len());
            for i in 0..keys.len() {
                for j in (i + 1)..keys.len() {
                    add_edge(keys[i].clone(), keys[j].clone(), weight);
                    add_edge(keys[j].clone(), keys[i].clone(), weight);
                }
            }
        }

        Self {
            nodes,
            station_to_keys,
            graph,
        }
    }
}

static NETWORK: LazyLock<Network> = LazyLock::new(Network::build);

pub static MRT_STATION_NAMES: LazyLock<Vec<String>> = LazyLock::new(|| {
    let mut names: Vec<String> = NETWORK.station_to_keys.keys().cloned().collect();
    names.sort();
    names
});

pub fn is_routeable_mrt_station(station: &str) -> bool {
    NETWORK.station_to_keys.contains_key(station)
}

fn make_route_segments(nodes: &[StationLineNode]) -> (Vec<MrtRouteSegment>, i64, Vec<String>) {
    if nodes.is_empty() {
        return (Vec::new(), 0, Vec::new());
    }
    let mut transfer_count = 0;
    let mut active = MrtRouteSegment {
        line: nodes[0].line.clone(),
        from: nodes[0].station.clone(),
        to: nodes[0].station.clone(),
        stops: 0,
        stations: vec![nodes[0].station.clone()],
    };
    let mut segments = Vec::new();
    let mut transfer_stations: Vec<String> = Vec::new();

    for pair in nodes.windows(2) {
        let previous = &pair[0];
        let current = &pair[1];
        if previous.station == current.station && previous.line != current.line {
            transfer_count += 1;
            transfer_stations.push(current.station.clone());
            if active.stops > 0 {
                segments.push(active);
            }
            active = MrtRouteSegment {
                line: current.line.clone(),
                from: current.station.clone(),
                to: current.station.clone(),
                stops: 0,
                stations: vec![current.station.clone()],
            };
            continue;
        }
        if active.line != current.line {
            if active.stops > 0 {
                segments.push(active);
            }
            active = MrtRouteSegment {
                line: current.line.clone(),
                from: previous.station.clone(),
                to: current.station.clone(),
                stops: 1,
                stations: vec![previous.station.clone(), current.station.clone()],
            };
            continue;
        }
        active.to = current.station.clone();
        active.stops += 1;
        active.stations.push(current.station.clone());
    }
    if active.stops > 0 {
        segments.push(active);
    }
    (segments, transfer_count, transfer_stations)
}

pub fn plan_mrt_route(start: &str, end: &str) -> Option<MrtRoutePlan> {
    let start_keys = NETWORK.station_to_keys.get(start)?;
    let end_keys = NETWORK.station_to_keys.get(end)?;
    if start_keys.is_empty() || end_keys.is_empty() {
        return None;
    }
    if start == end {
        return Some(MrtRoutePlan {
            start: start.into(),
            end: end.into(),
            stations: vec![start.into()],
            segments: Vec::new(),
            transfers: 0,
            estimated_minutes: 0.0,
        });
    }

    let mut distances: HashMap<String, f64> = HashMap::new();
    let mut previous: HashMap<String, Option<String>> = HashMap::new();
    let mut queue = MinHeap::new();
    for key in start_keys {
        distances.insert(key.clone(), 0.0);
        previous.insert(key.clone(), None);
        queue.push(HeapEntry { key: key.clone(), distance: 0.0 });
    }

    let mut best_end_key: Option<String> = None;
    while queue.len() > 0 {
        let current = queue.pop()?;
        let best = distances.get(&current.key);
        if best.is_none() || current.distance > *best.unwrap() {
            continue;
        }
        if end_keys.contains(&current.key) {
            best_end_key = Some(current.key);
            break;
        }
        for (to, weight) in NETWORK.graph.get(&current.key).into_iter().flatten() {
            let next = current.distance + weight;
            let known = distances.get(to);
            if known.is_none() || next < *known.unwrap() {
                distances.insert(to.clone(), next);
                previous.insert(to.clone(), Some(current.key.clone()));
                queue.push(HeapEntry { key: to.clone(), distance: next });
            }
        }
    }
    let best_end_key = best_end_key?;

    let mut path_keys = Vec::new();
    let mut cursor: Option<String> = Some(best_end_key);
    while let Some(key) = cursor {
        path_keys.push(key.clone());
        cursor = previous.get(&key).cloned().flatten();
    }
    path_keys.reverse();

    let path_nodes: Vec<&StationLineNode> = path_keys
        .iter()
        .filter_map(|key| NETWORK.nodes.get(key))
        .collect();
    if path_nodes.is_empty() {
        return None;
    }

    let mut stations = vec![path_nodes[0].station.clone()];
    for pair in path_nodes.windows(2) {
        if pair[0].station != pair[1].station {
            stations.push(pair[1].station.clone());
        }
    }

    let (segments, transfers, transfer_stations) = make_route_segments(&path_nodes.iter().map(|n| (*n).clone()).collect::<Vec<_>>());
    let travel_minutes: f64 = segments
        .iter()
        .map(|seg| seg.stops as f64 * minutes_per_stop(&seg.line))
        .sum();
    let transfer_minutes_total: f64 = transfer_stations
        .iter()
        .map(|station| transfer_minutes(station, NETWORK.station_to_keys.get(station).map_or(0, |k| k.len())))
        .sum();
    let estimated_minutes = travel_minutes + transfer_minutes_total + ENTRY_EXIT_BUFFER_MINUTES;

    Some(MrtRoutePlan {
        start: start.into(),
        end: end.into(),
        stations,
        segments,
        transfers,
        estimated_minutes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routeable_stations() {
        assert!(is_routeable_mrt_station("Jurong East"));
        assert!(is_routeable_mrt_station("Hume"));
        assert!(is_routeable_mrt_station("Prince Edward Road"));
        assert!(!is_routeable_mrt_station("Nonexistent Station"));
        assert!(!is_routeable_mrt_station(""));
    }

    #[test]
    fn station_names_sorted_and_contains_well_known() {
        assert!(!MRT_STATION_NAMES.is_empty());
        assert!(MRT_STATION_NAMES.windows(2).all(|p| p[0] <= p[1]));
        for name in ["Raffles Place", "Marina Bay", "Changi Airport"] {
            assert!(MRT_STATION_NAMES.iter().any(|n| n == name));
        }
    }

    #[test]
    fn plan_basic_routes() {
        assert!(plan_mrt_route("Fake Station", "Bugis").is_none());
        assert!(plan_mrt_route("Bugis", "Fake Station").is_none());
        let same = plan_mrt_route("Bugis", "Bugis").unwrap();
        assert_eq!(same.stations, ["Bugis"]);
        assert_eq!(same.estimated_minutes, 0.0);
        assert_eq!(same.transfers, 0);
        assert_eq!(
            plan_mrt_route("Hillview", "Beauty World").unwrap().stations,
            ["Hillview", "Hume", "Beauty World"]
        );
        let plan = plan_mrt_route("Bugis", "Paya Lebar").unwrap();
        assert_eq!(plan.transfers, 0);
    }

    #[test]
    fn lrt_loop_routes() {
        let route = plan_mrt_route("Ranggung", "Compassvale").unwrap();
        assert_eq!(route.transfers, 0);
        assert_eq!(route.segments, vec![MrtRouteSegment {
            line: "Sengkang LRT East Loop".into(),
            from: "Ranggung".into(),
            to: "Compassvale".into(),
            stops: 2,
            stations: vec!["Ranggung".into(), "Sengkang".into(), "Compassvale".into()],
        }]);
        let route = plan_mrt_route("Senja", "Bukit Panjang").unwrap();
        assert_eq!(route.segments, vec![MrtRouteSegment {
            line: "Bukit Panjang LRT".into(),
            from: "Senja".into(),
            to: "Bukit Panjang".into(),
            stops: 1,
            stations: vec!["Senja".into(), "Bukit Panjang".into()],
        }]);
        let route = plan_mrt_route("Renjong", "Kovan").unwrap();
        assert_eq!(
            route.segments.iter().map(|s| s.line.as_str()).collect::<Vec<_>>(),
            ["Sengkang LRT West Loop", "North East Line"]
        );
        assert_eq!(route.segments[0].to, "Sengkang");
        assert_eq!(route.transfers, 1);
    }
}
