//! App state, initialisation and derived (recomputed) values.
//!
//! Handlers in `msg.rs` mutate this state and call `recompute()` so the view
//! stays a pure `fn(&AppState, &mut Cx<AppState>) -> El`.

use std::collections::{HashMap, HashSet};

use argus_core::amenity_layers::{AmenityFilters, AmenityRegistry};
use argus_core::data::{DataDir, StaticData};
use argus_core::geo::{GeoFilter, GEO_FILTER_DEFAULT_RADIUS_M};
use argus_core::mrt_routing::MrtRoutePlan;
use argus_core::signals::{NotificationPrefs, SignalRow};
use argus_core::types::{AmenityType, LngLat, MapSearchResult, ParkFeatureKind};
use argus_map::{MapState, Overlays, OverlayId, DEFAULT_CENTER, DEFAULT_ZOOM};
use rune::TextEdit;

use crate::live::{
    def, load_prefs, parse_center, parse_notifications, parse_radius, parse_station,
    parse_visibility, DataSource, FeedKind, LiveDataSource, OnDemand, SourceStatus,
};

/// What a map click / search hit locked onto; drives the focus dialog.
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub enum Selection {
    Flight(String),
    Park { name: String, kind: ParkFeatureKind },
    Amenity { kind: AmenityType, id: String },
    Camera(String),
    BusStop(String),
    Carpark(String),
    Ev(String),
    Incident(String),
    MrtStation(String),
}

impl Selection {
    /// Stable key used as the dialog's Rune element key, so switching
    /// selection remounts the dialog content.
    #[allow(dead_code)]
    pub fn key(&self) -> String {
        match self {
            Selection::Flight(id) => format!("flight/{id}"),
            Selection::Park { name, .. } => format!("park/{name}"),
            Selection::Amenity { kind, id } => format!("amenity/{kind:?}/{id}"),
            Selection::Camera(id) => format!("camera/{id}"),
            Selection::BusStop(id) => format!("stop/{id}"),
            Selection::Carpark(id) => format!("carpark/{id}"),
            Selection::Ev(id) => format!("ev/{id}"),
            Selection::Incident(id) => format!("incident/{id}"),
            Selection::MrtStation(name) => format!("mrt/{name}"),
        }
    }
}

/// A map overlay click resolves to one of these.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Amenity { kind: AmenityType, id: String },
    Park { name: String, kind: ParkFeatureKind },
    MrtStation(String),
    Flight(String),
    Camera(String),
    BusStop(String),
    Carpark(String),
    Ev(String),
    Incident(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MrtPickTarget {
    Start,
    End,
}

#[derive(Clone, Debug)]
pub struct RouteState {
    pub start: String,
    pub end: String,
    pub target: MrtPickTarget,
    pub plan: Option<MrtRoutePlan>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AirWindow {
    OneHour,
    TwentyFourHour,
}

#[derive(Clone, Debug, Default)]
pub struct BusRouteState {
    pub selected_service: Option<String>,
    pub direction: Option<i64>,
    pub shown: bool,
}

#[derive(Clone, Debug)]
pub struct FeedbackState {
    pub open: bool,
    pub sent: bool,
    pub category: usize,
    pub message: TextEdit,
}

impl Default for FeedbackState {
    fn default() -> Self {
        Self { open: false, sent: false, category: 0, message: TextEdit::new("") }
    }
}

pub struct AppState {
    /// Feed snapshots + background polling. Panels only read this boundary.
    pub data: Box<dyn DataSource>,
    pub static_data: Option<StaticData>,
    pub registry: Option<AmenityRegistry>,

    // Layer controls.
    pub visibility: HashMap<String, bool>,
    pub amenity_filters: AmenityFilters,
    pub layer_filter: TextEdit,
    pub collapsed_groups: HashSet<String>,
    pub layer_subfilter_open: HashSet<String>,

    // Radius filter.
    pub geo_center: Option<LngLat>,
    pub geo_radius_m: f64,
    pub geo_picking: bool,

    // Map.
    pub map: MapState,
    pub overlays: Overlays,
    pub overlay_targets: HashMap<OverlayId, Target>,
    pub hovered_label: Option<String>,

    // Search.
    pub search_open: bool,
    pub search_query: TextEdit,
    pub search_results: Vec<MapSearchResult>,

    // Route planner / transit.
    pub route: RouteState,
    pub route_picker: Option<MrtPickTarget>,
    pub station_query: TextEdit,
    pub bus_route: BusRouteState,
    pub bus_search: TextEdit,
    pub selected_stop: Option<String>,

    // Selection / dialogs.
    pub selection: Option<Selection>,
    pub camera_dialog: Option<String>,
    pub bell_open: bool,
    pub feedback: FeedbackState,

    // Notifications.
    pub notification_prefs: NotificationPrefs,
    pub toasts: Vec<SignalRow>,
    pub dismissed: HashSet<String>,

    // Weather / air quality.
    pub air_window: AirWindow,
    pub air_pinned: bool,
    pub air_expanded: bool,
    pub weather_history: Vec<(f64, f64, f64)>, // temp, humidity, psi

    // Derived values (recomputed after every mutation).
    pub signals: Vec<SignalRow>,
    pub online_source_count: usize,
    pub active_source_count: usize,
    pub visible_layer_count: usize,
    pub layer_count: usize,
    pub clock_date: String,
    pub clock_time: String,
    pub now: f64,

    // Preferences persistence.
    pub prefs_dirty: bool,
    pub last_pref_save: f64,
    /// Set by the `live` scene: fetch every feed synchronously in `setup`.
    pub preload_feeds: bool,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("visible_layer_count", &self.visible_layer_count)
            .field("selection", &self.selection)
            .field("signals", &self.signals.len())
            .finish_non_exhaustive()
    }
}

pub fn init() -> AppState {
    let prefs = load_prefs();
    let visibility = parse_visibility(&prefs.visibility);
    let static_data = match StaticData::load(&DataDir::default()) {
        Ok(data) => Some(data),
        Err(err) => {
            eprintln!("argus: static bundles unavailable: {err}");
            None
        }
    };
    let registry = static_data.as_ref().map(AmenityRegistry::new);
    let geo_center = parse_center(&prefs.geo_center);
    let mut geo_radius_m = parse_radius(&prefs.geo_radius);
    if !(argus_core::geo::GEO_FILTER_MIN_RADIUS_M..=argus_core::geo::GEO_FILTER_MAX_RADIUS_M)
        .contains(&geo_radius_m)
    {
        geo_radius_m = GEO_FILTER_DEFAULT_RADIUS_M;
    }
    let start = parse_station(&prefs.mrt_start);
    let end = parse_station(&prefs.mrt_end);

    let mut map = MapState::new(DEFAULT_CENTER, DEFAULT_ZOOM, (860.0, 520.0));
    map.cluster.max_cluster_zoom = 11.0;
    if let Some(center) = geo_center {
        map.camera.center = argus_map::LngLat::new(center[0], center[1]);
    }

    let mut s = AppState {
        data: Box::new(LiveDataSource::new()),
        static_data,
        registry,
        visibility,
        amenity_filters: HashMap::new(),
        layer_filter: TextEdit::new(""),
        collapsed_groups: prefs.panels_collapsed.into_iter().collect(),
        layer_subfilter_open: HashSet::new(),
        geo_center,
        geo_radius_m,
        geo_picking: false,
        map,
        overlays: Overlays::new(),
        overlay_targets: HashMap::new(),
        hovered_label: None,
        search_open: false,
        search_query: TextEdit::new(""),
        search_results: Vec::new(),
        route: RouteState { start, end, target: MrtPickTarget::Start, plan: None },
        route_picker: None,
        station_query: TextEdit::new(""),
        bus_route: BusRouteState::default(),
        bus_search: TextEdit::new(""),
        selected_stop: None,
        selection: None,
        camera_dialog: None,
        bell_open: false,
        feedback: FeedbackState::default(),
        notification_prefs: parse_notifications(&prefs.notifications),
        toasts: Vec::new(),
        dismissed: HashSet::new(),
        air_window: AirWindow::OneHour,
        air_pinned: prefs.air_pinned,
        air_expanded: false,
        weather_history: Vec::new(),
        signals: Vec::new(),
        online_source_count: 0,
        active_source_count: 0,
        visible_layer_count: 0,
        layer_count: argus_core::layers::LAYERS.len(),
        clock_date: argus_core::format::singapore_today(),
        clock_time: String::new(),
        now: 0.0,
        prefs_dirty: false,
        last_pref_save: 0.0,
        preload_feeds: false,
    };
    // Real feeds. `.env` / config.toml are loaded by argus-data::Client::new;
    // keys never reach the UI or the snapshot.
    let mut data = LiveDataSource::new();
    data.install_live_fetchers(std::sync::Arc::new(argus_data::Client::new()));
    s.data = Box::new(data);
    s.recompute();
    s.clock_time = sg_clock();
    s.recompute();
    s
}

/// `HH:MM:SS` in Singapore (fixed UTC+8), matching the original header clock.
pub fn sg_clock() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let local = secs.rem_euclid(86_400) + 8 * 3600;
    format!("{:02}:{:02}:{:02}", (local / 3600) % 24, (local / 60) % 60, local % 60)
}

impl AppState {
    pub fn geo_filter(&self) -> Option<GeoFilter> {
        self.geo_center.map(|center| GeoFilter { center, radius_m: self.geo_radius_m })
    }

    pub fn on_demand(&self) -> OnDemand {
        OnDemand {
            carparks: self.visibility.get("carparks").copied().unwrap_or(false),
            ev: self.visibility.get("evChargers").copied().unwrap_or(false),
        }
    }

    /// Recompute everything the view reads from derived state. Called by the
    /// reducer after each mutation and by `poll` when feed data lands.
    pub fn recompute(&mut self) {
        self.data.set_on_demand(self.on_demand());
        if let Some(stop) = self.selected_stop.clone() {
            self.data.set_arrivals_stop(Some(stop));
        } else {
            self.data.set_arrivals_stop(None);
        }

        let plan = if self.route.start.is_empty() || self.route.end.is_empty() {
            None
        } else {
            argus_core::mrt_routing::plan_mrt_route(&self.route.start, &self.route.end)
        };
        self.route.plan = plan;

        self.search_results = crate::search::build_results(self);

        let snap = self.data.snapshot();
        self.signals = argus_core::signals::build_signal_digest(
            &argus_core::signals::DigestInput {
                train_alerts: &snap.train_alerts,
                incidents: &snap.incidents,
                weather: &snap.weather,
            },
        );
        self.online_source_count = snap.sources.iter().filter(|s| s.status.is_ok()).count();
        self.active_source_count = snap
            .sources
            .iter()
            .filter(|s| s.status != SourceStatus::Disabled)
            .count();
        self.visible_layer_count = self
            .visibility
            .iter()
            .filter(|(k, v)| **v && argus_core::layers::layer_by_key(k).is_some())
            .count();
        self.layer_count = argus_core::layers::LAYERS.len();

        let (overlays, targets) = crate::overlays::build(self);
        self.overlays = overlays;
        self.overlay_targets = targets;
        self.map.set_overlays(&self.overlays);
    }

    /// Serialise the pieces of UI state the original persists to localStorage.
    pub fn prefs_file(&self) -> crate::live::PrefsFile {
        crate::live::PrefsFile {
            visibility: serde_json::to_value(&self.visibility).unwrap_or_default(),
            geo_center: serde_json::to_value(self.geo_center).unwrap_or_default(),
            geo_radius: serde_json::to_value(self.geo_radius_m).unwrap_or_default(),
            mrt_start: serde_json::to_value(&self.route.start).unwrap_or_default(),
            mrt_end: serde_json::to_value(&self.route.end).unwrap_or_default(),
            notifications: serde_json::to_value(&self.notification_prefs).unwrap_or_default(),
            air_pinned: self.air_pinned,
            panels_collapsed: self.collapsed_groups.iter().cloned().collect(),
        }
    }

    #[allow(dead_code)]
    pub fn source_status(&self, kind: FeedKind) -> SourceStatus {
        self.data
            .snapshot()
            .sources
            .iter()
            .find(|s| s.key == kind)
            .map(|s| s.status.clone())
            .unwrap_or(SourceStatus::Unconfigured)
    }

    #[allow(dead_code)]
    pub fn feed_interval(&self, kind: FeedKind) -> String {
        let secs = def(kind).interval_s as i64;
        if secs >= 60 && secs % 60 == 0 {
            format!("{}m", secs / 60)
        } else {
            format!("{secs}s")
        }
    }
}

/// Scenes are named state mutators for headless snaps (see `main.rs`).
pub fn scene_dashboard(s: &mut AppState) {
    s.data = Box::new(LiveDataSource::new());
    s.recompute();
}

pub fn scene_layers_on(s: &mut AppState) {
    s.data = Box::new(LiveDataSource::new());
    for key in ["mrt", "busStops", "cameras", "incidents", "carparks", "evChargers", "parks", "malls", "hawkers"] {
        s.visibility.insert(key.into(), true);
    }
    s.geo_center = Some([103.8460, 1.3200]);
    s.geo_radius_m = 3000.0;
    s.map.camera.center = argus_map::LngLat::new(103.8460, 1.3200);
    s.map.camera.zoom = 11.6;
    s.recompute();
}

pub fn scene_route_planner(s: &mut AppState) {
    s.data = Box::new(LiveDataSource::new());
    s.route.start = "Raffles Place".into();
    s.route.end = "Jurong East".into();
    s.route.target = MrtPickTarget::End;
    s.recompute();
}

pub fn scene_poi_dialog(s: &mut AppState) {
    s.data = Box::new(LiveDataSource::new());
    s.visibility.insert("malls".into(), true);
    let id = s
        .registry
        .as_ref()
        .and_then(|r| r.layers.iter().find(|l| l.amenity_type == AmenityType::Mall))
        .and_then(|l| l.places.first())
        .map(|p| p.id().to_string());
    s.selection = id.map(|id| Selection::Amenity { kind: AmenityType::Mall, id });
    s.recompute();
}

/// Real-network scene: one synchronous preload in `setup` so the single
/// headless frame contains live data (mirrors argus-map's `preload_sync`).
pub fn scene_live(s: &mut AppState) {
    let mut data = LiveDataSource::new();
    data.install_live_fetchers(std::sync::Arc::new(argus_data::Client::new()));
    s.data = Box::new(data);
    s.preload_feeds = true;
    s.recompute();
}

pub fn scene_panels(s: &mut AppState) {
    let mut data = LiveDataSource::new();
    data.seed(crate::fixtures::demo_snapshot());
    s.data = Box::new(data);
    s.visibility.insert("mrt".into(), true);
    s.visibility.insert("busStops".into(), true);
    s.visibility.insert("cameras".into(), true);
    s.bus_search = TextEdit::new("Bukit");
    s.weather_history = (0..24)
        .map(|i| (28.5 + (i as f64 * 0.17).sin() * 2.0, 70.0 + i as f64 * 0.3, 52.0 + i as f64 * 0.2))
        .collect();
    s.recompute();
}
