//! Layer registry and pure layer-list grouping/filtering.
//! Ported from `lib/layers.ts` and `buildLayerGroups` in
//! `app/hooks/use-layer-list.ts`.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LayerGroupId {
    Transport,
    Ground,
    Amenities,
}

impl LayerGroupId {
    pub fn as_str(&self) -> &'static str {
        match self {
            LayerGroupId::Transport => "transport",
            LayerGroupId::Ground => "ground",
            LayerGroupId::Amenities => "amenities",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum LayerLegendItem {
    #[serde(rename = "dot")]
    Dot { tone: String, label: String },
    #[serde(rename = "swatch")]
    Swatch { shape: String, tone: String, label: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayerDefinition {
    pub key: String,
    pub group: LayerGroupId,
    pub label: String,
    pub note: String,
    pub tone: String,
    pub dot: String,
    #[serde(default)]
    pub legend: Vec<LayerLegendItem>,
    #[serde(rename = "defaultOn", default)]
    pub default_on: bool,
}

fn legend_dots(items: &[(&str, &str)]) -> Vec<LayerLegendItem> {
    items
        .iter()
        .map(|(tone, label)| LayerLegendItem::Dot { tone: (*tone).into(), label: (*label).into() })
        .collect()
}

pub fn layer_definitions() -> Vec<LayerDefinition> {
    let dot = |key: &str, group: LayerGroupId, label: &str, note: &str, tone: &str, on: bool, legend: Vec<LayerLegendItem>| {
        LayerDefinition {
            key: key.into(), group, label: label.into(), note: note.into(),
            tone: format!("text-signal-{}", tone), dot: format!("bg-signal-{}", tone),
            legend, default_on: on,
        }
    };
    let single = |tone: &str, label: &str| legend_dots(&[(tone, label)]);
    vec![
        dot("flights", LayerGroupId::Transport, "Air Activity", "live tracks", "inbound", true,
            legend_dots(&[("bg-signal-inbound", "Inbound"), ("bg-signal-outbound", "Outbound"), ("bg-signal-transit", "Transit")])),
        dot("mrt", LayerGroupId::Transport, "MRT & LRT", "lines + stations", "mrt", true, single("bg-signal-mrt", "MRT")),
        dot("busStops", LayerGroupId::Transport, "Bus Stops", "monitor points", "bus", true, single("bg-signal-bus", "Bus")),
        dot("cameras", LayerGroupId::Ground, "Road Cameras", "stream nodes", "camera", true, single("bg-signal-camera", "Cameras")),
        dot("incidents", LayerGroupId::Ground, "Road Incidents", "live events", "incident", false, single("bg-signal-incident", "Incidents")),
        dot("carparks", LayerGroupId::Ground, "Carparks", "live free lots", "carpark", false, single("bg-signal-carpark", "Carparks")),
        dot("evChargers", LayerGroupId::Ground, "EV Charging", "live chargers", "ev", false, single("bg-signal-ev", "EV")),
        dot("parks", LayerGroupId::Amenities, "Parks & Trails", "parks · connectors · trails", "park", false, vec![
            LayerLegendItem::Swatch { shape: "area".into(), tone: "border-signal-park bg-signal-park/30".into(), label: "Parks".into() },
            LayerLegendItem::Swatch { shape: "line".into(), tone: "bg-signal-park".into(), label: "Park connectors".into() },
            LayerLegendItem::Swatch { shape: "dashed".into(), tone: "border-signal-park".into(), label: "Trails".into() },
        ]),
        dot("malls", LayerGroupId::Amenities, "Malls", "shopping centres", "mall", false, single("bg-signal-mall", "Malls")),
        dot("hawkers", LayerGroupId::Amenities, "Hawker Centres", "food centres", "hawker", false, single("bg-signal-hawker", "Hawkers")),
        dot("gyms", LayerGroupId::Amenities, "Gyms", "fitness centres", "gym", false, single("bg-signal-gym", "Gyms")),
        dot("communityClubs", LayerGroupId::Amenities, "Community Clubs", "PA outlets", "community-club", false, single("bg-signal-community-club", "Community Clubs")),
        dot("pharmacies", LayerGroupId::Amenities, "Pharmacies", "retail pharmacies", "pharmacy", false, single("bg-signal-pharmacy", "Pharmacies")),
        dot("hotels", LayerGroupId::Amenities, "Hotels", "licensed hotels", "hotel", false, single("bg-signal-hotel", "Hotels")),
        dot("heritage", LayerGroupId::Amenities, "Heritage", "historic sites · monuments · museums", "heritage", false, single("bg-signal-heritage", "Heritage")),
        dot("healthcare", LayerGroupId::Amenities, "Healthcare", "GP · hospitals · dental", "healthcare", false, single("bg-signal-healthcare", "Healthcare")),
        dot("libraries", LayerGroupId::Amenities, "Libraries", "public libraries", "library", false, single("bg-signal-library", "Libraries")),
        dot("supermarkets", LayerGroupId::Amenities, "Supermarkets", "grocery supermarkets", "supermarket", false, single("bg-signal-supermarket", "Supermarkets")),
        dot("schools", LayerGroupId::Amenities, "Schools", "MOE schools · polytechnics · universities", "school", false, single("bg-signal-school", "Schools")),
    ]
}

pub static LAYERS: std::sync::LazyLock<Vec<LayerDefinition>> =
    std::sync::LazyLock::new(layer_definitions);

pub fn layer_by_key(key: &str) -> Option<&'static LayerDefinition> {
    LAYERS.iter().find(|l| l.key == key)
}

pub fn default_sensor_visibility() -> BTreeMap<String, bool> {
    LAYERS.iter().map(|l| (l.key.clone(), l.default_on)).collect()
}

/// Totally-on record for a sub-filter's options.
pub fn all_layer_options_on(all_values: &[String]) -> BTreeMap<String, bool> {
    all_values.iter().map(|v| (v.clone(), true)).collect()
}

/// Toggle one sub-filter option. When everything is on, tapping solos it;
/// deselecting the last on-option reverts to all-on.
pub fn toggle_layer_option_selection(
    selected: &BTreeMap<String, bool>,
    value: &str,
    all_values: &[String],
) -> BTreeMap<String, bool> {
    let on_count = all_values
        .iter()
        .filter(|option| selected.get(option.as_str()).copied().unwrap_or(true))
        .count();
    if on_count == all_values.len() {
        return all_values
            .iter()
            .map(|option| (option.clone(), option == value))
            .collect();
    }
    let mut next = selected.clone();
    let currently_on = selected.get(value).copied().unwrap_or(true);
    next.insert(value.to_string(), !currently_on);
    if all_values
        .iter()
        .all(|option| !next.get(option.as_str()).copied().unwrap_or(true))
    {
        return all_layer_options_on(all_values);
    }
    next
}

// ---------------------------------------------------------------------------
// buildLayerGroups (use-layer-list.ts)
// ---------------------------------------------------------------------------

/// The row shape the layer list needs (a projection of `SensorRow`).
#[derive(Clone, Debug, PartialEq)]
pub struct LayerRow {
    pub key: String,
    pub label: String,
    pub note: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayerListGroup {
    pub id: LayerGroupId,
    pub label: String,
    pub rows: Vec<LayerRow>,
    pub visible_count: usize,
}

fn matches_filter(row: &LayerRow, group_label: &str, normalized: &str) -> bool {
    row.label.to_lowercase().contains(normalized)
        || row.note.to_lowercase().contains(normalized)
        || group_label.to_lowercase().contains(normalized)
}

/// Pure grouping + filtering over layer rows, in registry display order.
pub fn build_layer_groups(
    rows: &[LayerRow],
    visibility: &HashMap<String, bool>,
    normalized_filter: &str,
) -> Vec<LayerListGroup> {
    let filter_active = !normalized_filter.is_empty();
    let rows_by_key: HashMap<&str, &LayerRow> = rows.iter().map(|r| (r.key.as_str(), r)).collect();
    let groups = [
        (LayerGroupId::Transport, "Transport"),
        (LayerGroupId::Ground, "Roads & Parking"),
        (LayerGroupId::Amenities, "Amenities"),
    ];
    let mut result = Vec::new();
    for (id, label) in groups {
        let group_rows: Vec<LayerRow> = LAYERS
            .iter()
            .filter(|layer| layer.group == id && rows_by_key.contains_key(layer.key.as_str()))
            .filter_map(|layer| rows_by_key.get(layer.key.as_str()).map(|r| (*r).clone()))
            .collect();
        let group_label_matches = filter_active && label.to_lowercase().contains(normalized_filter);
        let filtered_rows = if filter_active && !group_label_matches {
            group_rows
                .into_iter()
                .filter(|row| matches_filter(row, label, normalized_filter))
                .collect()
        } else {
            group_rows
        };
        if filtered_rows.is_empty() {
            continue;
        }
        let visible_count = filtered_rows
            .iter()
            .filter(|row| visibility.get(row.key.as_str()).copied().unwrap_or(false))
            .count();
        result.push(LayerListGroup { id, label: label.into(), rows: filtered_rows, visible_count });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_defaults_and_toggle() {
        let defaults = default_sensor_visibility();
        assert_eq!(defaults["flights"], true);
        assert_eq!(defaults["parks"], false);
        assert_eq!(LAYERS.len(), 19);
        let values: Vec<String> = ["a".into(), "b".into()].to_vec();
        let all = all_layer_options_on(&values);
        let solo = toggle_layer_option_selection(&all, "a", &values);
        assert_eq!(solo["a"], true);
        assert_eq!(solo["b"], false);
        let back = toggle_layer_option_selection(&solo, "a", &values);
        assert_eq!(back, all);
    }

    #[test]
    fn build_groups_respects_registry_order() {
        let rows = vec![
            LayerRow { key: "flights".into(), label: "Air Activity".into(), note: "live tracks".into() },
            LayerRow { key: "parks".into(), label: "Parks & Trails".into(), note: "parks".into() },
        ];
        let mut visibility = HashMap::new();
        visibility.insert("flights".to_string(), true);
        let groups = build_layer_groups(&rows, &visibility, "");
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].id, LayerGroupId::Transport);
        assert_eq!(groups[0].visible_count, 1);
        assert_eq!(groups[1].id, LayerGroupId::Amenities);
        let filtered = build_layer_groups(&rows, &visibility, "air");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].rows[0].key, "flights");
    }
}
