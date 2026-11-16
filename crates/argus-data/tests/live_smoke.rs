//! Live smoke tests. Ignored by default; run once with the real keys in
//! `../../.env`:
//!
//!   cargo test -p argus-data --test live_smoke -- --ignored --nocapture
//!
//! Each test prints only a row count and health status — never key values.

use argus_data::{Client, Result};

fn report<T>(label: &str, client: &Client, result: Result<T>, len: impl Fn(&T) -> usize) {
    match result {
        Ok(value) => eprintln!(
            "SMOKE {label}: OK rows={} health={:?}",
            len(&value),
            client.source(argus_data::Source::from_label(label)).status
        ),
        Err(err) => eprintln!(
            "SMOKE {label}: ERROR status={} message={}",
            err.status(),
            err.message()
        ),
    }
}

// `Source::from_label` keeps the report generic; add it as a small helper on
// the test side rather than exposing label parsing in the library.
trait FromLabel {
    fn from_label(label: &str) -> Self;
}
impl FromLabel for argus_data::Source {
    fn from_label(label: &str) -> Self {
        use argus_data::Source::*;
        match label {
            "bus-stops" => BusStops,
            "bus-arrivals" => BusArrivals,
            "bus-routes" => BusRoutes,
            "cameras" => Cameras,
            "incidents" => Incidents,
            "carparks" => Carparks,
            "ev-chargers" => EvChargers,
            "weather" => Weather,
            "news" => News,
            "flights" => Flights,
            "train-alerts" => TrainAlerts,
            _ => Feedback,
        }
    }
}

#[test]
#[ignore = "live network + real API keys"]
fn bus_stops() {
    let c = Client::new();
    report("bus-stops", &c, c.get_bus_stops(), |v| v.len());
}

#[test]
#[ignore = "live network + real API keys"]
fn bus_arrivals() {
    let c = Client::new();
    report("bus-arrivals", &c, c.get_bus_arrivals("01013"), |v| v.len());
}

#[test]
#[ignore = "live network + real API keys"]
fn bus_routes() {
    let c = Client::new();
    report("bus-routes", &c, c.get_bus_route("12", None), |v| {
        v.directions.len()
    });
}

#[test]
#[ignore = "live network + real API keys"]
fn cameras() {
    let c = Client::new();
    report("cameras", &c, c.get_traffic_cameras(), |v| v.len());
}

#[test]
#[ignore = "live network + real API keys"]
fn incidents() {
    let c = Client::new();
    report("incidents", &c, c.get_traffic_incidents(), |v| v.len());
}

#[test]
#[ignore = "live network + real API keys"]
fn carparks() {
    let c = Client::new();
    report("carparks", &c, c.get_carpark_availability(), |v| v.len());
}

#[test]
#[ignore = "live network + real API keys"]
fn ev_chargers() {
    let c = Client::new();
    report("ev-chargers", &c, c.get_ev_charging_locations(), |v| {
        v.len()
    });
}

#[test]
#[ignore = "live network + real API keys"]
fn weather() {
    let c = Client::new();
    report("weather", &c, c.get_weather(), |v| v.four_day.len());
}

#[test]
#[ignore = "live network + real API keys"]
fn news() {
    let c = Client::new();
    report("news", &c, c.get_news(), |v| v.len());
}

#[test]
#[ignore = "live network + real API keys"]
fn flights() {
    let c = Client::new();
    report("flights", &c, c.get_flights(), |v| v.len());
}

#[test]
#[ignore = "live network + real API keys"]
fn train_alerts() {
    let c = Client::new();
    report("train-alerts", &c, c.get_train_service_alerts(), |v| {
        v.len()
    });
}

#[test]
#[ignore = "live network + real API keys"]
fn feedback_config() {
    // Check config only: do not insert a row into the live database.
    let c = Client::new();
    eprintln!(
        "SMOKE feedback: configured={}",
        c.config().database_url().is_ok()
    );
}
