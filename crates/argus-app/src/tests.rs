//! Reducer / view smoke tests. No network, no window: `init()` only loads the
//! checked-in static bundles, and `LiveDataSource` has no fetchers installed.

use argus_core::types::AmenityType;
use rune::{Env, Runtime};

use super::{view, Msg};
use crate::msg::{dispatch, toggle_layer};
use crate::{search, state};
use crate::state::{AppState, MrtPickTarget, Selection, Target, scene_panels};

fn state() -> AppState {
    let mut s = state::init();
    scene_panels(&mut s);
    s
}

#[test]
fn toggling_a_layer_updates_visibility_and_prefs_flag() {
    let mut s = state();
    assert!(!s.visibility["parks"]);
    toggle_layer(&mut s, "parks");
    assert!(s.visibility["parks"]);
    assert!(s.prefs_dirty);
    assert!(s.overlays.polygons.len() > 0 || s.overlays.lines.len() > 0);
    toggle_layer(&mut s, "parks");
    assert!(!s.visibility["parks"]);
}

#[test]
fn group_toggle_flips_every_layer_in_the_group() {
    let mut s = state();
    dispatch(&mut s, Msg::ToggleGroupVisibility(vec!["malls".into(), "hawkers".into()]));
    // Any-off means "turn the whole group on" (mirrors the original header eye).
    assert!(s.visibility["malls"]);
    assert!(s.visibility["hawkers"]);
    dispatch(&mut s, Msg::ToggleGroupVisibility(vec!["malls".into(), "hawkers".into()]));
    assert!(!s.visibility["malls"]);
    assert!(!s.visibility["hawkers"]);
}

#[test]
fn selecting_an_overlay_opens_the_focus_dialog() {
    let mut s = state();
    s.registry.as_ref().unwrap();
    let target = s.overlay_targets.iter()
        .find_map(|(id, t)| matches!(t, Target::Flight(_)).then_some(*id))
        .expect("flight overlay exists in the fixture");
    dispatch(&mut s, Msg::MapClick(target));
    assert!(matches!(s.selection, Some(Selection::Flight(_))));
    dispatch(&mut s, Msg::ClearSelection);
    assert!(s.selection.is_none());
}

#[test]
fn mrt_route_flow_picks_stations_swaps_and_resets() {
    let mut s = state();
    assert!(s.route.plan.is_none());
    dispatch(&mut s, Msg::PickMrtStation("Raffles Place".into()));
    assert_eq!(s.route.start, "Raffles Place");
    assert_eq!(s.route.target, MrtPickTarget::End);
    dispatch(&mut s, Msg::PickMrtStation("Jurong East".into()));
    assert_eq!(s.route.end, "Jurong East");
    assert_eq!(s.route.target, MrtPickTarget::Start);
    let plan = s.route.plan.clone().expect("a route exists");
    assert_eq!(plan.start, "Raffles Place");
    assert!(!plan.segments.is_empty());
    assert!(!s.overlays.lines.is_empty(), "route is drawn on the map");
    dispatch(&mut s, Msg::SwapRoute);
    assert_eq!(s.route.start, "Jurong East");
    dispatch(&mut s, Msg::ResetRoute);
    assert!(s.route.plan.is_none());
}

#[test]
fn geo_filter_pick_scopes_search_and_renders_a_circle() {
    let mut s = state();
    assert!(s.overlays.circles.is_empty());
    dispatch(&mut s, Msg::MapPickGeo([103.84, 1.32]));
    assert_eq!(s.geo_center, Some([103.84, 1.32]));
    assert_eq!(s.overlays.circles.len(), 1);
    let before = s.search_results.len();
    dispatch(&mut s, Msg::SetGeoRadius(250.0));
    assert!(s.search_results.len() <= before);
    dispatch(&mut s, Msg::ClearGeo);
    assert!(s.geo_center.is_none());
    assert!(s.overlays.circles.is_empty());
}

#[test]
fn search_ranks_by_name_and_opens_a_selection() {
    let mut s = state();
    s.search_query = rune::TextEdit::new("Raffles");
    let hits = search::visible(&s, "Raffles");
    assert!(!hits.is_empty(), "fixture has searchable places / stops");
    assert!(hits.iter().any(|h| h.name.to_lowercase().contains("raffles")));
    let id = hits[0].id.clone();
    dispatch(&mut s, Msg::SelectSearchResult(id));
    assert!(s.selection.is_some());
}

#[test]
fn subfilter_toggle_changes_the_amenity_filter() {
    let mut s = state();
    let (value, count) = {
        let layer = s.registry.as_ref().unwrap().layers.iter()
            .find(|l| l.amenity_type == AmenityType::Gym).unwrap();
        (layer.filter.as_ref().unwrap().choices[0].value.clone(), layer.filter.as_ref().unwrap().choices.len())
    };
    dispatch(&mut s, Msg::ToggleSubOption { kind: AmenityType::Gym, value: value.clone() });
    let selected = s.amenity_filters[&AmenityType::Gym].clone();
    assert_eq!(selected.len(), count);
    // The first tap on an all-on filter solos that option.
    assert!(selected[&value]);
    assert!(selected.values().filter(|v| **v).count() == 1);
}

#[test]
fn full_view_renders_without_panicking() {
    let mut rt = Runtime::new(state::init(), view, (1440.0, 900.0), Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    let (x, y) = rt.find("header").expect("header is a keyed element");
    assert!(x > 0.0 && y > 0.0);
    assert!(rt.find("map/canvas").is_some(), "map is mounted");
    assert!(rt.find("panel/layers").is_some(), "layers panel is mounted");
}
