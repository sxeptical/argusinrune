//! Messages and pure-ish reducers. Every UI handler funnels through `dispatch`
//! so state transitions are testable without a window.

use argus_core::geo::GEO_FILTER_MIN_RADIUS_M;
use argus_core::layers::{all_layer_options_on, toggle_layer_option_selection};
use argus_core::types::{AmenityType, LngLat};
use argus_map::OverlayId;

use crate::live::FeedKind;
use crate::state::{AirWindow, AppState, MrtPickTarget, Selection, Target};
use crate::theme;

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    ToggleLayer(String),
    ToggleGroupCollapse(String),
    ToggleGroupVisibility(Vec<String>),
    ToggleSubFilter(String),
    ToggleSubOption {
        kind: AmenityType,
        value: String,
    },
    SetAllSubOptions {
        kind: AmenityType,
        all: bool,
    },
    SetGeoPicking(bool),
    SetGeoRadius(f64),
    ClearGeo,
    MapPickGeo(LngLat),
    MapClick(OverlayId),
    SetRouteStart(String),
    SetRouteEnd(String),
    SetRouteTarget(MrtPickTarget),
    ToggleRoutePicker(MrtPickTarget),
    PickMrtStation(String),
    SwapRoute,
    ResetRoute,
    Select(Selection),
    ClearSelection,
    OpenSearch(bool),
    SelectSearchResult(String),
    SelectCamera(String),
    CloseCamera,
    /// Re-fetch the LTA camera feed and its frames on demand.
    RefreshCameraImages,
    /// Mall store directory filters.
    SetMallStoreCategory(Option<String>),
    ToggleMallStoreHalal,
    /// Place photo gallery: pick a thumbnail, drop a broken image.
    SetPhotoIndex(usize),
    MarkPhotoFailed(String),
    ToggleBell,
    ToggleNotificationRail,
    ToggleNotificationRoad,
    ToggleNotificationWeather,
    ToggleAirPinned,
    ToggleAirExpanded,
    SetAirWindow(AirWindow),
    SelectStop(String),
    SetBusService(String),
    SetBusDirection(i64),
    ShowBusRoute(bool),
    OpenFeedback(bool),
    SetFeedbackCategory(usize),
    SendFeedback,
    TogglePanelCollapsed(String),
    DismissToast(String),
    DismissAllToasts,
}

/// Apply a message and refresh all derived state (overlays, search, signals).
pub fn dispatch(s: &mut AppState, msg: Msg) {
    match msg {
        Msg::ToggleLayer(key) => toggle_layer(s, &key),
        Msg::ToggleGroupCollapse(id) => {
            if !s.collapsed_groups.remove(&id) {
                s.collapsed_groups.insert(id);
            }
            s.prefs_dirty = true;
        }
        Msg::ToggleGroupVisibility(keys) => {
            let any_on = keys
                .iter()
                .any(|k| s.visibility.get(k).copied().unwrap_or(false));
            for key in keys {
                s.visibility.insert(key, !any_on);
            }
            s.prefs_dirty = true;
        }
        Msg::ToggleSubFilter(key) => {
            if !s.layer_subfilter_open.remove(&key) {
                s.layer_subfilter_open.insert(key);
            }
        }
        Msg::ToggleSubOption { kind, value } => {
            if let Some(layer) = s.registry.as_ref().and_then(|r| r.layer(kind)) {
                let all: Vec<String> = layer
                    .filter
                    .as_ref()
                    .map(|f| f.options.clone())
                    .unwrap_or_default();
                let current = s.amenity_filters.entry(kind).or_default();
                let next = toggle_layer_option_selection(current, &value, &all);
                *current = next;
                s.prefs_dirty = true;
            }
        }
        Msg::SetAllSubOptions { kind, all } => {
            if let Some(layer) = s.registry.as_ref().and_then(|r| r.layer(kind)) {
                let values: Vec<String> = layer
                    .filter
                    .as_ref()
                    .map(|f| f.options.clone())
                    .unwrap_or_default();
                let selection = if all {
                    all_layer_options_on(&values)
                } else {
                    Default::default()
                };
                s.amenity_filters.insert(kind, selection);
                s.prefs_dirty = true;
            }
        }
        Msg::SetGeoPicking(on) => {
            s.geo_picking = on;
            s.map.geo_filter_picking = on;
        }
        Msg::SetGeoRadius(m) => {
            s.geo_radius_m = m.max(GEO_FILTER_MIN_RADIUS_M);
            s.prefs_dirty = true;
        }
        Msg::ClearGeo => {
            s.geo_center = None;
            s.geo_picking = false;
            s.map.geo_filter_picking = false;
            s.prefs_dirty = true;
        }
        Msg::MapPickGeo(point) => {
            s.geo_center = Some(point);
            s.geo_picking = false;
            s.map.geo_filter_picking = false;
            s.prefs_dirty = true;
        }
        Msg::MapClick(id) => {
            if let Some(target) = s.overlay_targets.get(&id).cloned() {
                handle_map_target(s, target);
            }
        }
        Msg::SetRouteStart(station) => {
            s.route.start = station;
            s.prefs_dirty = true;
        }
        Msg::SetRouteEnd(station) => {
            s.route.end = station;
            s.prefs_dirty = true;
        }
        Msg::SetRouteTarget(target) => s.route.target = target,
        Msg::ToggleRoutePicker(target) => {
            s.route_picker = if s.route_picker == Some(target) {
                None
            } else {
                Some(target)
            };
            s.station_query = rune::TextEdit::new("");
        }
        Msg::PickMrtStation(station) => {
            // Picking from the panel's dropdown fills the same field the map
            // click target points at, then advances to the other field.
            pick_mrt_station(s, &station);
            s.route_picker = None;
        }
        Msg::SwapRoute => {
            std::mem::swap(&mut s.route.start, &mut s.route.end);
            s.prefs_dirty = true;
        }
        Msg::ResetRoute => {
            s.route.start.clear();
            s.route.end.clear();
            s.route.target = MrtPickTarget::Start;
            s.prefs_dirty = true;
        }
        Msg::Select(selection) => select(s, selection),
        Msg::ClearSelection => s.selection = None,
        Msg::OpenSearch(open) => {
            s.search_open = open;
            if !open {
                s.search_query = rune::TextEdit::new("");
            }
        }
        Msg::SelectSearchResult(id) => {
            if let Some(result) = s.search_results.iter().find(|r| r.id == id).cloned() {
                s.map
                    .fly_to(argus_map::LngLat::new(result.lng, result.lat), result.zoom);
                s.search_open = false;
                if let Some(sel) = crate::search::selection_for(&result) {
                    reveal_selection(s, &sel);
                    select(s, sel);
                }
            }
        }
        Msg::SelectCamera(id) => s.camera_dialog = Some(id),
        Msg::CloseCamera => s.camera_dialog = None,
        Msg::RefreshCameraImages => {
            // The camera feed is the source of truth: a refresh pulls fresh
            // presigned frame URLs, then the image store re-fetches the bytes.
            s.data.request(FeedKind::Cameras);
            let cameras: Vec<String> = s
                .data
                .snapshot()
                .cameras
                .iter()
                .map(|c| c.image_link.clone())
                .filter(|url| !url.is_empty())
                .collect();
            s.images.update(s.now, &[], &cameras);
        }
        Msg::SetMallStoreCategory(category) => s.mall_directory.category = category,
        Msg::ToggleMallStoreHalal => s.mall_directory.halal_only = !s.mall_directory.halal_only,
        Msg::SetPhotoIndex(index) => s.photo_gallery.active = index,
        Msg::MarkPhotoFailed(url) => s.photo_gallery.mark_failed(&url),

        Msg::ToggleBell => {
            s.bell_open = !s.bell_open;
        }
        Msg::ToggleNotificationRail => {
            s.notification_prefs.rail = !s.notification_prefs.rail;
            s.prefs_dirty = true;
        }
        Msg::ToggleNotificationRoad => {
            s.notification_prefs.road = !s.notification_prefs.road;
            s.prefs_dirty = true;
        }
        Msg::ToggleNotificationWeather => {
            s.notification_prefs.weather = !s.notification_prefs.weather;
            s.prefs_dirty = true;
        }
        Msg::ToggleAirPinned => {
            s.air_pinned = !s.air_pinned;
            s.prefs_dirty = true;
        }
        Msg::ToggleAirExpanded => s.air_expanded = !s.air_expanded,
        Msg::SetAirWindow(window) => s.air_window = window,
        Msg::SelectStop(code) => {
            s.selected_stop = Some(code);
            s.bus_route.selected_service = None;
            s.bus_route.direction = None;
            s.bus_route.shown = false;
        }
        Msg::SetBusService(service) => {
            let stop = s.selected_stop.clone();
            s.data.request_bus_route(service.clone(), stop);
            s.bus_route.selected_service = Some(service);
        }
        Msg::SetBusDirection(dir) => s.bus_route.direction = Some(dir),
        Msg::ShowBusRoute(shown) => {
            if shown && let Some(service) = s.bus_route.selected_service.clone() {
                let stop = s.selected_stop.clone();
                s.data.request_bus_route(service, stop);
            }
            s.bus_route.shown = shown;
        }
        Msg::OpenFeedback(open) => {
            s.feedback.open = open;
            if open {
                s.feedback.sent = false;
            }
        }
        Msg::SetFeedbackCategory(idx) => s.feedback.category = idx,
        Msg::SendFeedback => {
            let rating = s.feedback.category as i32 + 3; // 3..5, valid for FeedbackSubmission
            let message = s.feedback.message.text.clone();
            s.data.request_feedback(
                rating,
                if message.trim().is_empty() {
                    None
                } else {
                    Some(message)
                },
            );
            s.feedback.sent = true;
        }
        Msg::TogglePanelCollapsed(key) => {
            if !s.collapsed_groups.remove(&key) {
                s.collapsed_groups.insert(key);
            }
            s.prefs_dirty = true;
        }
        Msg::DismissToast(id) => {
            s.dismissed.insert(id.clone());
            s.toasts.retain(|t| t.id != id);
        }
        Msg::DismissAllToasts => {
            for t in &s.toasts {
                s.dismissed.insert(t.id.clone());
            }
            s.toasts.clear();
        }
    }
    s.recompute();
}

/// Toggle one layer; unit-tested as the core layer interaction.
pub fn toggle_layer(s: &mut AppState, key: &str) {
    let current = s.visibility.get(key).copied().unwrap_or(false);
    s.visibility.insert(key.to_string(), !current);
    s.prefs_dirty = true;
}

/// Clicking an MRT station fills the current pick target and advances it,
/// mirroring `useMrtPlanner.pickStation`.
pub fn pick_mrt_station(s: &mut AppState, station: &str) {
    match s.route.target {
        MrtPickTarget::Start => {
            s.route.start = station.to_string();
            s.route.target = MrtPickTarget::End;
        }
        MrtPickTarget::End => {
            s.route.end = station.to_string();
            s.route.target = MrtPickTarget::Start;
        }
    }
    s.prefs_dirty = true;
}

fn handle_map_target(s: &mut AppState, target: Target) {
    match target {
        Target::MrtStation(name) => pick_mrt_station(s, &name),
        Target::Amenity { kind, id } => select(s, Selection::Amenity { kind, id }),
        Target::Park { name, kind } => select(s, Selection::Park { name, kind }),
        Target::Flight(id) => select(s, Selection::Flight(id)),
        Target::Camera(id) => select(s, Selection::Camera(id)),
        Target::BusStop(code) => {
            dispatch(s, Msg::SelectStop(code.clone()));
            select(s, Selection::BusStop(code));
        }
        Target::Carpark(id) => select(s, Selection::Carpark(id)),
        Target::Ev(id) => select(s, Selection::Ev(id)),
        Target::Incident(id) => select(s, Selection::Incident(id)),
    }
}

pub fn select(s: &mut AppState, selection: Selection) {
    s.selection = Some(selection);
    s.camera_dialog = None;
}

/// A search hit must be drawn when its focus dialog opens: switch its layer on
/// and re-enable any sub-filter option that was hiding it.
pub fn reveal_selection(s: &mut AppState, selection: &Selection) {
    let (kind, id) = match selection {
        Selection::Amenity { kind, id } => (Some(*kind), Some(id.clone())),
        _ => (None, None),
    };
    if let (Some(kind), Some(id)) = (kind, id)
        && let Some(layer) = s.registry.as_ref().and_then(|r| r.layer(kind))
    {
        s.visibility.insert(layer.layer_key.clone(), true);
        if let Some(place) = layer.by_id.get(&id).and_then(|i| layer.places.get(*i))
            && let Some(option) = place.filter_option()
        {
            let entry = s.amenity_filters.entry(kind).or_default();
            if entry.get(&option).copied() == Some(false) {
                entry.insert(option, true);
            }
        }
    }
    if matches!(selection, Selection::Park { .. }) {
        s.visibility.insert("parks".into(), true);
    }
    s.prefs_dirty = true;
}

/// Sub-filter summary shown in the sidebar ("All", "2 selected", …).
pub fn subfilter_summary(s: &AppState, kind: AmenityType) -> String {
    let Some(layer) = s.registry.as_ref().and_then(|r| r.layer(kind)) else {
        return "All".into();
    };
    let Some(filter) = layer.filter.as_ref() else {
        return "All".into();
    };
    let selected = s.amenity_filters.get(&kind);
    let on: Vec<&str> = filter
        .choices
        .iter()
        .filter(|c| {
            selected
                .and_then(|m| m.get(&c.value))
                .copied()
                .unwrap_or(true)
        })
        .map(|c| c.label.as_str())
        .collect();
    if on.len() == filter.choices.len() {
        "All".into()
    } else if on.len() <= 2 {
        on.join(", ")
    } else {
        format!("{} selected", on.len())
    }
}

/// Source-health row label with the configured poll interval.
#[allow(dead_code)]
pub fn source_line(s: &AppState, kind: crate::live::FeedKind) -> String {
    format!(
        "{} · {}",
        s.source_status(kind).label(),
        s.feed_interval(kind)
    )
}

/// Colour used for a semantic category in the intel panel.
pub fn signal_color(signal: &argus_core::signals::SignalRow) -> rune::Color {
    match signal.tone {
        argus_core::signals::SignalTone::Danger => theme::DANGER,
        argus_core::signals::SignalTone::Warning => theme::WARNING,
        argus_core::signals::SignalTone::Info => theme::INFO,
        argus_core::signals::SignalTone::Muted => theme::MUTED,
    }
}
