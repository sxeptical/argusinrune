//! Data-source boundary for the dashboard.
//!
//! Panels never call HTTP clients themselves: they read [`DataSnapshot`] from
//! `AppState::data`. This module owns scheduling (Rune `Latest` on worker
//! threads + `poll`/`wake_at` on the UI thread) and the source-health statuses.
//!
//! `init()` installs the real `argus-data::Client` via `install_live_fetchers`;
//! every feed is a `Latest` closure on a Rune worker thread using the client's
//! blocking `reqwest` methods. Source health is read from the client's
//! `HealthRegistry` (including `unconfigured` / `stale`), not guessed here.
//! The `dashboard` / `panels` scenes swap in a no-fetch source or fixture data
//! so goldens stay deterministic without network access; the `live` scene
//! calls `preload_sync()` (like argus-map's tile preload) to render real data.

use std::sync::Arc;

use argus_core::preferences::{
    parse_geo_filter_center, parse_geo_filter_radius, parse_mrt_station, parse_notification_prefs,
    parse_sensor_visibility,
};
use argus_core::signals::NotificationPrefs;
use argus_core::types::{
    BusArrival, BusRouteResponse, BusStop, CarparkAvailability, EvChargingLocation, FlightState,
    LngLat, NewsItem, Pm25Band, PsiStatus, RegionalAirReadings, TrafficCamera, TrafficIncident,
    TrainServiceAlert, UvStatus, WeatherData,
};
use argus_data::{Client as DataClient, FeedbackSubmission, HealthStatus};
use rune::{Latest, Tx, Waker};

/// Which upstream feed a request belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FeedKind {
    BusStops,
    Cameras,
    Incidents,
    Carparks,
    EvChargers,
    Weather,
    News,
    Flights,
    TrainAlerts,
    BusArrivals,
}

impl FeedKind {
    /// Matches `argus_data::Source::key()` so health rows line up.
    #[allow(dead_code)]
    pub fn key(self) -> &'static str {
        match self {
            FeedKind::BusStops => "bus stops",
            FeedKind::Cameras => "cameras",
            FeedKind::Incidents => "incidents",
            FeedKind::Carparks => "carparks",
            FeedKind::EvChargers => "ev chargers",
            FeedKind::Weather => "weather",
            FeedKind::News => "news",
            FeedKind::Flights => "flights",
            FeedKind::TrainAlerts => "train alerts",
            FeedKind::BusArrivals => "bus arrivals",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceStatus {
    Queued,
    Loading,
    Ok,
    Stale,
    Error(String),
    Unconfigured,
    Disabled,
}

impl SourceStatus {
    pub fn is_settled(&self) -> bool {
        matches!(
            self,
            SourceStatus::Ok
                | SourceStatus::Stale
                | SourceStatus::Error(_)
                | SourceStatus::Unconfigured
                | SourceStatus::Disabled
        )
    }
    pub fn is_ok(&self) -> bool {
        matches!(self, SourceStatus::Ok)
    }
    pub fn label(&self) -> &'static str {
        match self {
            SourceStatus::Queued => "Queued",
            SourceStatus::Loading => "Syncing",
            SourceStatus::Ok => "Online",
            SourceStatus::Stale => "Stale",
            SourceStatus::Error(_) => "Offline",
            SourceStatus::Unconfigured => "Unconfigured",
            SourceStatus::Disabled => "Disabled",
        }
    }
}

#[derive(Clone, Debug)]
pub struct SourceState {
    pub key: FeedKind,
    pub label: &'static str,
    pub status: SourceStatus,
}

/// Everything panels read. No panel performs I/O.
#[derive(Clone, Debug)]
pub struct DataSnapshot {
    pub bus_stops: Vec<BusStop>,
    pub cameras: Vec<TrafficCamera>,
    pub carparks: Vec<CarparkAvailability>,
    pub ev_chargers: Vec<EvChargingLocation>,
    pub incidents: Vec<TrafficIncident>,
    pub train_alerts: Vec<TrainServiceAlert>,
    pub flights: Vec<FlightState>,
    pub news: Vec<NewsItem>,
    pub bus_arrivals: Vec<BusArrival>,
    pub bus_routes: Option<BusRouteResponse>,
    pub feedback_status: Option<String>,
    pub weather: WeatherData,
    pub sources: Vec<SourceState>,
}

pub fn default_weather() -> WeatherData {
    WeatherData {
        temperature: None,
        humidity: None,
        psi: None,
        psi_status: PsiStatus::Unknown,
        psi_regions: RegionalAirReadings::default(),
        pm25_one_hour: None,
        pm25_one_hour_band: Pm25Band::Unknown,
        pm25_one_hour_regions: RegionalAirReadings::default(),
        uv: None,
        uv_status: UvStatus::Unknown,
        four_day: Vec::new(),
        forecast: "Waiting for weather feed…".into(),
        last_updated: String::new(),
    }
}

impl Default for DataSnapshot {
    fn default() -> Self {
        let sources = FEED_DEFS
            .iter()
            .map(|d| SourceState {
                key: d.kind,
                label: d.label,
                status: SourceStatus::Unconfigured,
            })
            .collect();
        Self {
            bus_stops: Vec::new(),
            cameras: Vec::new(),
            carparks: Vec::new(),
            ev_chargers: Vec::new(),
            incidents: Vec::new(),
            train_alerts: Vec::new(),
            flights: Vec::new(),
            news: Vec::new(),
            bus_arrivals: Vec::new(),
            bus_routes: None,
            feedback_status: None,
            weather: default_weather(),
            sources,
        }
    }
}

/// A feed definition mirrors one SWR `SourceDefinition` in
/// `app/hooks/use-dashboard-sources.ts`.
#[derive(Clone, Copy, Debug)]
pub struct FeedDef {
    pub kind: FeedKind,
    pub label: &'static str,
    /// SWR `refreshInterval`, in seconds.
    pub interval_s: f64,
    /// Fetched only while the matching layer is switched on.
    pub on_demand: bool,
}

/// Intervals are copied verbatim from `use-dashboard-sources.ts`; keep in sync
/// if the original dashboard's polling changes.
pub const FEED_DEFS: &[FeedDef] = &[
    FeedDef {
        kind: FeedKind::BusStops,
        label: "LTA Bus Network",
        interval_s: 1800.0,
        on_demand: false,
    },
    FeedDef {
        kind: FeedKind::Cameras,
        label: "Traffic Cameras",
        interval_s: 60.0,
        on_demand: false,
    },
    FeedDef {
        kind: FeedKind::Incidents,
        label: "Road Incidents",
        interval_s: 60.0,
        on_demand: false,
    },
    FeedDef {
        kind: FeedKind::Carparks,
        label: "Carpark Availability",
        interval_s: 60.0,
        on_demand: true,
    },
    FeedDef {
        kind: FeedKind::EvChargers,
        label: "EV Charging Points",
        interval_s: 300.0,
        on_demand: true,
    },
    FeedDef {
        kind: FeedKind::Weather,
        label: "Weather Grid",
        interval_s: 300.0,
        on_demand: false,
    },
    FeedDef {
        kind: FeedKind::News,
        label: "OSINT Stream",
        interval_s: 900.0,
        on_demand: false,
    },
    FeedDef {
        kind: FeedKind::Flights,
        label: "Airspace Feed",
        interval_s: 15.0,
        on_demand: false,
    },
    FeedDef {
        kind: FeedKind::TrainAlerts,
        label: "Train Service Alerts",
        interval_s: 60.0,
        on_demand: false,
    },
    FeedDef {
        kind: FeedKind::BusArrivals,
        label: "Bus Arrivals",
        interval_s: 15.0,
        on_demand: true,
    },
];

pub fn def(kind: FeedKind) -> &'static FeedDef {
    FEED_DEFS
        .iter()
        .find(|d| d.kind == kind)
        .expect("feed definition")
}

type Fetch<T> = Arc<dyn Fn() -> Result<T, String> + Send + Sync + 'static>;

/// One latest-wins, interval-polled feed. The answer lands on the UI thread in
/// `poll`; it never blocks a view or handler.
struct Feed<T: Send + 'static> {
    def: &'static FeedDef,
    latest: Latest<Result<T, String>>,
    fetch: Option<Fetch<T>>,
    due: f64,
    commit: fn(&mut DataSnapshot, T),
}

impl<T: Send + Clone + 'static> Feed<T> {
    fn new(def: &'static FeedDef, commit: fn(&mut DataSnapshot, T)) -> Self {
        Self {
            def,
            latest: Latest::default(),
            fetch: None,
            due: 0.0,
            commit,
        }
    }

    fn setup(&self, waker: Waker) {
        self.latest.set_waker(waker);
    }

    fn is_enabled(&self, on_demand: &OnDemand) -> bool {
        if !self.def.on_demand {
            return true;
        }
        match self.def.kind {
            FeedKind::Carparks => on_demand.carparks,
            FeedKind::EvChargers => on_demand.ev,
            _ => true,
        }
    }

    fn request(&self) {
        if let Some(fetch) = self.fetch.as_ref() {
            let fetch = Arc::clone(fetch);
            self.latest.request((), move |_| fetch());
        }
    }

    /// Land a new answer, start the next interval, and wake for it.
    /// Returns true only when visible snapshot state changed.
    fn poll(
        &mut self,
        now: f64,
        tx: &mut Tx,
        snap: &mut DataSnapshot,
        on_demand: &OnDemand,
    ) -> bool {
        let mut changed = false;
        if let Some(reply) = self.latest.take() {
            match reply {
                Ok(value) => {
                    (self.commit)(snap, value.clone());
                    self.set_status(snap, SourceStatus::Ok);
                    let _ = value;
                    changed = true;
                }
                Err(message) => {
                    self.set_status(snap, SourceStatus::Error(message));
                    changed = true;
                }
            }
        }
        if self.fetch.is_none() || !self.is_enabled(on_demand) {
            if !self.is_enabled(on_demand) {
                self.set_status(snap, SourceStatus::Disabled);
            }
            return changed;
        }
        if now >= self.due {
            self.set_status(snap, SourceStatus::Loading);
            self.request();
            self.due = now + self.def.interval_s;
            tx.wake_at(self.due);
            changed = true;
        }
        changed
    }

    fn set_status(&self, snap: &mut DataSnapshot, status: SourceStatus) {
        if let Some(s) = snap.sources.iter_mut().find(|s| s.key == self.def.kind) {
            s.status = status;
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct OnDemand {
    pub carparks: bool,
    pub ev: bool,
}

type ArrivalsFetch = Arc<dyn Fn(&str) -> Result<Vec<BusArrival>, String> + Send + Sync + 'static>;
type RouteFetch =
    Arc<dyn Fn(&str, Option<&str>) -> Result<BusRouteResponse, String> + Send + Sync + 'static>;

/// Panels talk to this boundary, never to `argus-data` directly, so a live
/// implementation can be swapped in without touching any panel.
#[allow(dead_code)]
pub trait DataSource: Send {
    fn setup(&mut self, waker: Waker);
    fn poll(&mut self, now: f64, tx: &mut Tx) -> bool;
    fn snapshot(&self) -> &DataSnapshot;
    fn snapshot_mut(&mut self) -> &mut DataSnapshot;
    fn set_on_demand(&mut self, on_demand: OnDemand);
    /// Request an out-of-band refresh (used when a panel is first opened).
    fn request(&mut self, kind: FeedKind);
    /// Change the stop that the 15 s arrivals poll addresses.
    fn set_arrivals_stop(&mut self, stop: Option<String>);
    /// Fetch a bus service's geometry on demand (never on a timer).
    fn request_bus_route(&mut self, service: String, stop: Option<String>);
    /// Submit the feedback form off the UI thread.
    fn request_feedback(&mut self, rating: i32, message: Option<String>);
    /// Synchronous first fetch for the headless `live` scene, mirroring
    /// `argus-map`'s `preload_sync`. The client's TTL cache dedupes the first
    /// background poll.
    fn preload_sync(&mut self);
    fn cancel(&self);
}

/// The production implementation: interval-polled `Latest` feeds backed by
/// `argus-data::Client`, whose methods use `reqwest::blocking` on Rune's worker
/// threads. Without `install_live_fetchers` it degrades to the honest
/// "Unconfigured" state used by the default scenes and unit tests.
pub struct LiveDataSource {
    snap: DataSnapshot,
    on_demand: OnDemand,
    client: Option<Arc<DataClient>>,
    bus_stops: Feed<Vec<BusStop>>,
    cameras: Feed<Vec<TrafficCamera>>,
    incidents: Feed<Vec<TrafficIncident>>,
    carparks: Feed<Vec<CarparkAvailability>>,
    ev_chargers: Feed<Vec<EvChargingLocation>>,
    weather: Feed<WeatherData>,
    news: Feed<Vec<NewsItem>>,
    flights: Feed<Vec<FlightState>>,
    train_alerts: Feed<Vec<TrainServiceAlert>>,
    arrivals: Latest<Result<(String, Vec<BusArrival>), String>>,
    arrivals_fetch: Option<ArrivalsFetch>,
    arrivals_stop: Option<String>,
    arrivals_due: f64,
    route: Latest<Result<BusRouteResponse, String>>,
    route_fetch: Option<RouteFetch>,
    route_key: Option<(String, Option<String>)>,
    feedback: Latest<Result<(), String>>,
}

impl Default for LiveDataSource {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for LiveDataSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveDataSource")
            .field("sources", &self.snap.sources)
            .finish()
    }
}

impl LiveDataSource {
    pub fn new() -> Self {
        let get = |kind| def(kind);
        Self {
            snap: DataSnapshot::default(),
            on_demand: OnDemand::default(),
            client: None,
            bus_stops: Feed::new(get(FeedKind::BusStops), |s, v| s.bus_stops = v),
            cameras: Feed::new(get(FeedKind::Cameras), |s, v| s.cameras = v),
            incidents: Feed::new(get(FeedKind::Incidents), |s, v| s.incidents = v),
            carparks: Feed::new(get(FeedKind::Carparks), |s, v| s.carparks = v),
            ev_chargers: Feed::new(get(FeedKind::EvChargers), |s, v| s.ev_chargers = v),
            weather: Feed::new(get(FeedKind::Weather), |s, v| s.weather = v),
            news: Feed::new(get(FeedKind::News), |s, v| s.news = v),
            flights: Feed::new(get(FeedKind::Flights), |s, v| s.flights = v),
            train_alerts: Feed::new(get(FeedKind::TrainAlerts), |s, v| s.train_alerts = v),
            arrivals: Latest::default(),
            arrivals_fetch: None,
            arrivals_stop: None,
            arrivals_due: 0.0,
            route: Latest::default(),
            route_fetch: None,
            route_key: None,
            feedback: Latest::default(),
        }
    }

    /// Wire every feed to `argus-data`. The client is `Arc`-shared so all
    /// `Latest` closures hit the same TTL cache and health registry.
    pub fn install_live_fetchers(&mut self, client: Arc<DataClient>) {
        let err = |e: argus_data::Error| e.to_string();
        let c = Arc::clone(&client);
        self.bus_stops.fetch = Some(Arc::new(move || c.get_bus_stops().map_err(err)));
        let c = Arc::clone(&client);
        self.cameras.fetch = Some(Arc::new(move || c.get_traffic_cameras().map_err(err)));
        let c = Arc::clone(&client);
        self.incidents.fetch = Some(Arc::new(move || c.get_traffic_incidents().map_err(err)));
        let c = Arc::clone(&client);
        self.carparks.fetch = Some(Arc::new(move || c.get_carpark_availability().map_err(err)));
        let c = Arc::clone(&client);
        self.ev_chargers.fetch = Some(Arc::new(move || c.get_ev_charging_locations().map_err(err)));
        let c = Arc::clone(&client);
        self.weather.fetch = Some(Arc::new(move || c.get_weather().map_err(err)));
        let c = Arc::clone(&client);
        self.news.fetch = Some(Arc::new(move || c.get_news().map_err(err)));
        let c = Arc::clone(&client);
        self.flights.fetch = Some(Arc::new(move || c.get_flights().map_err(err)));
        let c = Arc::clone(&client);
        self.train_alerts.fetch = Some(Arc::new(move || c.get_train_service_alerts().map_err(err)));

        let c = Arc::clone(&client);
        self.arrivals_fetch = Some(Arc::new(move |stop| c.get_bus_arrivals(stop).map_err(err)));
        let c = Arc::clone(&client);
        self.route_fetch = Some(Arc::new(move |service, stop| {
            c.get_bus_route(service, stop).map_err(err)
        }));
        self.client = Some(client);
        for source in &mut self.snap.sources {
            source.status = SourceStatus::Loading;
        }
    }

    /// Source health comes from argus-data's registry, not from our own guesses.
    fn sync_health(&mut self) -> bool {
        let Some(client) = self.client.as_ref() else {
            return false;
        };
        let health = client.source_health();
        let mut changed = false;
        for row in &mut self.snap.sources {
            let kind = row.key;
            let new_status = if !self.on_demand.carparks && kind == FeedKind::Carparks
                || !self.on_demand.ev && kind == FeedKind::EvChargers
            {
                SourceStatus::Disabled
            } else {
                health
                    .iter()
                    .find(|h| h.source.key() == kind.key())
                    .map(|h| match h.status {
                        HealthStatus::Ok => SourceStatus::Ok,
                        HealthStatus::Stale => SourceStatus::Stale,
                        HealthStatus::Error => {
                            SourceStatus::Error(h.message.clone().unwrap_or_default())
                        }
                        HealthStatus::Unconfigured => SourceStatus::Unconfigured,
                        HealthStatus::Loading => SourceStatus::Loading,
                    })
                    .unwrap_or_else(|| row.status.clone())
            };
            if new_status != row.status {
                row.status = new_status;
                changed = true;
            }
        }
        changed
    }

    fn poll_route(&mut self) -> bool {
        let Some(reply) = self.route.take() else {
            return false;
        };
        match reply {
            Ok(route) => {
                self.snap.bus_routes = Some(route);
                true
            }
            Err(message) => {
                self.snap.feedback_status = Some(message);
                false
            }
        }
    }

    fn poll_feedback(&mut self) -> bool {
        let Some(reply) = self.feedback.take() else {
            return false;
        };
        match reply {
            Ok(()) => self.snap.feedback_status = Some("Thanks — report submitted.".into()),
            Err(message) => self.snap.feedback_status = Some(message),
        }
        true
    }

    /// Seed a snapshot without touching the network (used by scenes / tests).
    pub fn seed(&mut self, snap: DataSnapshot) {
        self.snap = snap;
        for source in &mut self.snap.sources {
            if source.status == SourceStatus::Queued {
                source.status = SourceStatus::Ok;
            }
        }
    }

    fn poll_arrivals(&mut self, now: f64, tx: &mut Tx) -> bool {
        let Some(fetch) = self.arrivals_fetch.clone() else {
            return false;
        };
        let Some(stop) = self.arrivals_stop.clone() else {
            return false;
        };
        let mut changed = false;
        if let Some(reply) = self.arrivals.take() {
            match reply {
                Ok((code, arrivals))
                    if code == self.arrivals_stop.as_deref().unwrap_or_default() =>
                {
                    self.snap.bus_arrivals = arrivals;
                    self.set_arrivals_status(SourceStatus::Ok);
                    changed = true;
                }
                Err(message) => {
                    self.set_arrivals_status(SourceStatus::Error(message));
                    changed = true;
                }
                _ => {}
            }
        }
        if now >= self.arrivals_due {
            self.set_arrivals_status(SourceStatus::Loading);
            let latest = &self.arrivals;
            latest.request(stop.clone(), move |code| {
                fetch(&code).map(|rows| (code, rows))
            });
            self.arrivals_due = now + def(FeedKind::BusArrivals).interval_s;
            tx.wake_at(self.arrivals_due);
        }
        changed
    }

    fn set_arrivals_status(&mut self, status: SourceStatus) {
        if let Some(s) = self
            .snap
            .sources
            .iter_mut()
            .find(|s| s.key == FeedKind::BusArrivals)
        {
            s.status = status;
        }
    }
}

impl DataSource for LiveDataSource {
    fn setup(&mut self, waker: Waker) {
        self.bus_stops.setup(waker.clone());
        self.cameras.setup(waker.clone());
        self.incidents.setup(waker.clone());
        self.carparks.setup(waker.clone());
        self.ev_chargers.setup(waker.clone());
        self.weather.setup(waker.clone());
        self.news.setup(waker.clone());
        self.flights.setup(waker.clone());
        self.train_alerts.setup(waker.clone());
        self.arrivals.set_waker(waker.clone());
        self.route.set_waker(waker.clone());
        self.feedback.set_waker(waker);
    }

    fn poll(&mut self, now: f64, tx: &mut Tx) -> bool {
        let mut changed = false;
        changed |= self
            .bus_stops
            .poll(now, tx, &mut self.snap, &self.on_demand);
        changed |= self.cameras.poll(now, tx, &mut self.snap, &self.on_demand);
        changed |= self
            .incidents
            .poll(now, tx, &mut self.snap, &self.on_demand);
        changed |= self.carparks.poll(now, tx, &mut self.snap, &self.on_demand);
        changed |= self
            .ev_chargers
            .poll(now, tx, &mut self.snap, &self.on_demand);
        changed |= self.weather.poll(now, tx, &mut self.snap, &self.on_demand);
        changed |= self.news.poll(now, tx, &mut self.snap, &self.on_demand);
        changed |= self.flights.poll(now, tx, &mut self.snap, &self.on_demand);
        changed |= self
            .train_alerts
            .poll(now, tx, &mut self.snap, &self.on_demand);
        changed |= self.poll_arrivals(now, tx);
        changed |= self.poll_route();
        changed |= self.poll_feedback();
        changed |= self.sync_health();
        changed
    }

    fn snapshot(&self) -> &DataSnapshot {
        &self.snap
    }
    fn snapshot_mut(&mut self) -> &mut DataSnapshot {
        &mut self.snap
    }
    fn set_on_demand(&mut self, on_demand: OnDemand) {
        self.on_demand = on_demand;
    }
    fn request(&mut self, kind: FeedKind) {
        if kind == FeedKind::BusArrivals {
            self.arrivals_due = 0.0;
            return;
        }
        // A foreground refresh simply moves the next due time forward; the
        // feed's own `Latest` dedupes work that is already in flight.
        match kind {
            FeedKind::BusStops => self.bus_stops.due = 0.0,
            FeedKind::Cameras => self.cameras.due = 0.0,
            FeedKind::Incidents => self.incidents.due = 0.0,
            FeedKind::Carparks => self.carparks.due = 0.0,
            FeedKind::EvChargers => self.ev_chargers.due = 0.0,
            FeedKind::Weather => self.weather.due = 0.0,
            FeedKind::News => self.news.due = 0.0,
            FeedKind::Flights => self.flights.due = 0.0,
            FeedKind::TrainAlerts => self.train_alerts.due = 0.0,
            FeedKind::BusArrivals => {}
        }
    }
    fn set_arrivals_stop(&mut self, stop: Option<String>) {
        if self.arrivals_stop != stop {
            self.arrivals_stop = stop;
            self.arrivals_due = 0.0;
            self.snap.bus_arrivals.clear();
            self.arrivals.cancel();
        }
    }
    fn request_bus_route(&mut self, service: String, stop: Option<String>) {
        let key = (service, stop);
        if self.route_key.as_ref() == Some(&key) {
            return;
        }
        self.route_key = Some(key.clone());
        self.snap.bus_routes = None;
        if let Some(fetch) = self.route_fetch.clone() {
            let latest = &self.route;
            latest.request(key.clone(), move |(service, stop)| {
                fetch(&service, stop.as_deref())
            });
        }
    }
    fn request_feedback(&mut self, rating: i32, message: Option<String>) {
        let Some(client) = self.client.clone() else {
            self.snap.feedback_status = Some("Feedback storage is not configured.".into());
            return;
        };
        self.feedback.request((), move |_| {
            let submission = FeedbackSubmission::validate(rating, message)
                .map_err(|e: argus_data::Error| e.to_string())?;
            client
                .submit_feedback(submission)
                .map_err(|e: argus_data::Error| e.to_string())
        });
    }
    fn preload_sync(&mut self) {
        let Some(client) = self.client.clone() else {
            return;
        };
        // Each call populates argus-data's TTL cache; the first background
        // poll then reuses it instead of hitting the upstream twice.
        if let Ok(v) = client.get_bus_stops() {
            self.snap.bus_stops = v;
        }
        if let Ok(v) = client.get_traffic_cameras() {
            self.snap.cameras = v;
        }
        if let Ok(v) = client.get_traffic_incidents() {
            self.snap.incidents = v;
        }
        if let Ok(v) = client.get_carpark_availability() {
            self.snap.carparks = v;
        }
        if let Ok(v) = client.get_ev_charging_locations() {
            self.snap.ev_chargers = v;
        }
        if let Ok(v) = client.get_weather() {
            self.snap.weather = v;
        }
        if let Ok(v) = client.get_news() {
            self.snap.news = v;
        }
        if let Ok(v) = client.get_flights() {
            self.snap.flights = v;
        }
        if let Ok(v) = client.get_train_service_alerts() {
            self.snap.train_alerts = v;
        }
        self.sync_health();
    }
    fn cancel(&self) {
        self.arrivals.cancel();
        self.route.cancel();
        self.feedback.cancel();
    }
}

/// The state file is small; parse with argus-core's pure parsers where they
/// exist and fall back to defaults for anything malformed.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct PrefsFile {
    #[serde(default)]
    pub visibility: serde_json::Value,
    #[serde(default)]
    pub geo_center: serde_json::Value,
    #[serde(default)]
    pub geo_radius: serde_json::Value,
    #[serde(default)]
    pub mrt_start: serde_json::Value,
    #[serde(default)]
    pub mrt_end: serde_json::Value,
    #[serde(default)]
    pub notifications: serde_json::Value,
    #[serde(default)]
    pub air_pinned: bool,
    #[serde(default)]
    pub panels_collapsed: Vec<String>,
}

pub fn prefs_path() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("ARGUS_CONFIG_DIR") {
        return std::path::PathBuf::from(dir).join("preferences.json");
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    std::path::PathBuf::from(home)
        .join(".config")
        .join("argus")
        .join("preferences.json")
}

pub fn load_prefs() -> PrefsFile {
    let Ok(text) = std::fs::read_to_string(prefs_path()) else {
        return PrefsFile::default();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

pub fn save_prefs(prefs: &PrefsFile) {
    let Ok(text) = serde_json::to_string_pretty(prefs) else {
        return;
    };
    let path = prefs_path();
    if let Some(parent) = path.parent()
        && std::fs::create_dir_all(parent).is_err()
    {
        return;
    }
    // Write-then-rename: a crash never leaves a half-written preferences file.
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, text).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

pub fn parse_visibility(value: &serde_json::Value) -> std::collections::HashMap<String, bool> {
    let defaults = argus_core::layers::default_sensor_visibility();
    let mut out: std::collections::HashMap<String, bool> = defaults.into_iter().collect();
    if let Some(stored) =
        parse_sensor_visibility(value, &argus_core::layers::default_sensor_visibility())
    {
        out.extend(stored);
    }
    out
}

pub fn parse_center(value: &serde_json::Value) -> Option<LngLat> {
    parse_geo_filter_center(value).ok().flatten()
}

pub fn parse_radius(value: &serde_json::Value) -> f64 {
    parse_geo_filter_radius(value).unwrap_or(argus_core::geo::GEO_FILTER_DEFAULT_RADIUS_M)
}

pub fn parse_station(value: &serde_json::Value) -> String {
    parse_mrt_station(value).unwrap_or_default()
}

pub fn parse_notifications(value: &serde_json::Value) -> NotificationPrefs {
    parse_notification_prefs(value).unwrap_or_default()
}
