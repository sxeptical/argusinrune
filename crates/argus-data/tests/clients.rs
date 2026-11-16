//! Fixture-driven tests for the HTTP clients. No network is used: a small
//! `Transport` stub returns canned responses keyed by URL, mirroring the TS
//! tests that stubbed `globalThis.fetch`.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use argus_core::api_types::LtaTrainServiceAlertsResponse;
use argus_data::{
    ArgusConfig, Client, Error, HttpRequest, HttpResponse, Result, Transport,
    health::{HealthStatus, Source},
    lta::{
        is_safe_camera_url, normalize_carpark_availability, normalize_ev_charging_locations,
        normalize_train_service_alerts,
    },
    news::{is_singapore_relevant, parse_rss_items},
    weather::assemble_weather,
    wire::{WireCarparkResponse, WireEvBatchFile, WireTrafficImagesResponse},
};

struct FnTransport<F>(F)
where
    F: Fn(&HttpRequest) -> Result<HttpResponse> + Send + Sync;

impl<F> Transport for FnTransport<F>
where
    F: Fn(&HttpRequest) -> Result<HttpResponse> + Send + Sync,
{
    fn get(&self, request: &HttpRequest) -> Result<HttpResponse> {
        (self.0)(request)
    }

    fn post(&self, request: &HttpRequest, _body: &[u8]) -> Result<HttpResponse> {
        (self.0)(request)
    }
}

fn ok(body: impl Into<Vec<u8>>) -> Result<HttpResponse> {
    Ok(HttpResponse {
        status: 200,
        body: body.into(),
    })
}

fn fail(status: u16) -> Result<HttpResponse> {
    Ok(HttpResponse {
        status,
        body: vec![],
    })
}

fn config() -> ArgusConfig {
    ArgusConfig::new(
        Some("test-lta-key".into()),
        Some("test-avia-key".into()),
        None,
    )
}

fn client<F>(handler: F) -> Client
where
    F: Fn(&HttpRequest) -> Result<HttpResponse> + Send + Sync + 'static,
{
    Client::with_transport(config(), Arc::new(FnTransport(handler)))
}

// ---------- LTA fixtures ----------

#[test]
fn grouped_camera_fixture_filters_unsafe_hosts() {
    let payload: WireTrafficImagesResponse =
        serde_json::from_str(include_str!("fixtures/lta_cameras_grouped.json")).unwrap();
    let cameras: Vec<_> = payload
        .into_cameras()
        .into_iter()
        .filter(|c| is_safe_camera_url(&c.image_link))
        .collect();
    assert_eq!(cameras.len(), 2);
    assert_eq!(cameras[0].camera_id, "A001");
    assert_eq!(cameras[1].camera_id, "B002");
}

#[test]
fn camera_404_is_an_empty_feed() {
    let client = client(|_| fail(404));
    let cameras = client.get_traffic_cameras().unwrap();
    assert!(cameras.is_empty());
    assert_eq!(client.source(Source::Cameras).status, HealthStatus::Ok);
}

#[test]
fn grouped_camera_payload_via_client() {
    let body = include_str!("fixtures/lta_cameras_grouped.json")
        .as_bytes()
        .to_vec();
    let client = client(move |_| ok(body.clone()));
    let cameras = client.get_traffic_cameras().unwrap();
    assert_eq!(cameras.len(), 2);
    assert_eq!(cameras[0].location, "Camera A001");
}

#[test]
fn train_alert_fixture_maps_codes_and_stations() {
    let payload: LtaTrainServiceAlertsResponse =
        serde_json::from_str(include_str!("fixtures/lta_train_alerts.json")).unwrap();
    let alerts = normalize_train_service_alerts(&payload);
    assert_eq!(alerts.len(), 1);
    assert_eq!(
        alerts[0].status,
        argus_core::types::TrainAlertStatus::Disrupted
    );
    assert_eq!(
        alerts[0].message,
        "No train service between Buona Vista and Raffles Place"
    );
    assert_eq!(
        alerts[0].affected_lines,
        [
            "East West Line",
            "North East Line",
            "Punggol LRT East Loop",
            "XYZ"
        ]
    );
    assert_eq!(
        alerts[0].affected_stations,
        ["EW21", "EW22", "NE1", "NE7", "PE1", "XY1"]
    );
    assert_eq!(alerts[0].start_time.as_deref(), Some("2026-09-09T08:15:00"));
    assert_eq!(alerts[0].end_time, None);
}

#[test]
fn carpark_fixture_merges_rows() {
    let payload: WireCarparkResponse =
        serde_json::from_str(include_str!("fixtures/lta_carparks.json")).unwrap();
    let rows: Vec<_> = payload
        .value
        .into_iter()
        .map(argus_core::api_types::RawCarparkAvailability::from)
        .collect();
    let lots = normalize_carpark_availability(&rows);
    assert_eq!(lots.len(), 2);
    assert_eq!(lots[0].id, "HDB:TPMN");
    assert_eq!(lots[0].car_lots, Some(421));
    assert_eq!(lots[0].motorcycle_lots, Some(12));
    assert_eq!(lots[1].id, "LTA:1");
    assert_eq!(lots[1].heavy_lots, Some(3));
}

#[test]
fn ev_batch_fixture_normalizes_and_links_are_checked() {
    let file: WireEvBatchFile =
        serde_json::from_str(include_str!("fixtures/lta_ev_batch.json")).unwrap();
    let file: argus_core::api_types::LtaEvBatchFile = file.into();
    let locations = normalize_ev_charging_locations(&file.ev_locations_data);
    assert_eq!(locations.len(), 1);
    assert_eq!(locations[0].available, 2);
    assert_eq!(locations[0].occupied, 1);
    assert_eq!(locations[0].total, 4);
    assert_eq!(locations[0].max_power_kw, Some(180.0));
    assert!(locations[0].has_dc);

    let link = "https://dmprod-datasets.s3.ap-southeast-1.amazonaws.com/ev-batch/EVBatch.json";
    let batch = format!(r#"{{"value":[{{"Link":"{link}"}}]}}"#).into_bytes();
    let file_body = include_str!("fixtures/lta_ev_batch.json")
        .as_bytes()
        .to_vec();
    let calls = Arc::new(std::sync::Mutex::new(Vec::<(String, Vec<String>)>::new()));
    let calls2 = Arc::clone(&calls);
    let client = client(move |req| {
        calls2.lock().unwrap().push((
            req.url.clone(),
            req.headers.iter().map(|(k, _)| k.clone()).collect(),
        ));
        if req.url.contains("/EVCBatch") {
            ok(batch.clone())
        } else {
            ok(file_body.clone())
        }
    });
    let locations = client.get_ev_charging_locations().unwrap();
    assert_eq!(locations.len(), 1);
    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert!(
        calls[0]
            .1
            .iter()
            .any(|h| h.eq_ignore_ascii_case("AccountKey"))
    );
    assert!(
        !calls[1]
            .1
            .iter()
            .any(|h| h.eq_ignore_ascii_case("AccountKey"))
    );
}

#[test]
fn untrusted_ev_batch_link_is_rejected() {
    let client = client(|_| ok(r#"{"value":[{"Link":"https://evil.example/EVBatch.json"}]}"#));
    let err = client.get_ev_charging_locations().unwrap_err();
    assert_eq!(err.status(), 502);
    assert_eq!(
        client.source(Source::EvChargers).status,
        HealthStatus::Error
    );
}

#[test]
fn bus_stop_pagination_uses_skip_offsets() {
    let skips = Arc::new(std::sync::Mutex::new(Vec::new()));
    let skips2 = Arc::clone(&skips);
    let client = client(move |req| {
        let skip: usize = req
            .url
            .split("$skip=")
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        skips2.lock().unwrap().push(skip);
        let count = if skip == 0 { 500 } else { 2 };
        let rows: Vec<_> = (0..count)
            .map(|i| {
                serde_json::json!({
                    "BusStopCode": format!("{:05}", skip + i),
                    "RoadName": "Test Road",
                    "Description": "Test Stop",
                    "Latitude": 1.3,
                    "Longitude": 103.8,
                })
            })
            .collect();
        ok(serde_json::json!({ "value": rows }).to_string())
    });
    let stops = client.get_bus_stops().unwrap();
    assert_eq!(stops.len(), 502);
    assert_eq!(*skips.lock().unwrap(), [0, 500]);
}

// ---------- weather fixtures ----------

#[test]
fn weather_fixtures_report_worst_region() {
    let psi = serde_json::from_str::<argus_core::api_types::DataGovPsiResponse>(include_str!(
        "fixtures/weather_psi.json"
    ))
    .unwrap();
    let pm25 = serde_json::from_str::<argus_core::api_types::DataGovPm25Response>(include_str!(
        "fixtures/weather_pm25.json"
    ))
    .unwrap();
    let (data, partial) = assemble_weather(None, Some(&psi), None, None, None, Some(&pm25), None);
    assert!(partial);
    assert_eq!(data.psi, Some(146.0));
    assert_eq!(data.psi_status, argus_core::types::PsiStatus::Unhealthy);
    assert_eq!(data.pm25_one_hour, Some(195.0));
    assert_eq!(data.pm25_one_hour_band, argus_core::types::Pm25Band::High);
    assert_eq!(data.pm25_one_hour_regions.central, None);
}

#[test]
fn weather_all_feeds_failing_is_an_error() {
    let client = client(|_| fail(503));
    let err = client.get_weather().unwrap_err();
    assert_eq!(err.status(), 502);
    assert_eq!(client.source(Source::Weather).status, HealthStatus::Error);
}

#[test]
fn weather_partial_snapshot_via_client() {
    let psi = include_str!("fixtures/weather_psi.json")
        .as_bytes()
        .to_vec();
    let pm25 = include_str!("fixtures/weather_pm25.json")
        .as_bytes()
        .to_vec();
    let client = client(move |req| {
        if req.url.contains("/psi") {
            ok(psi.clone())
        } else if req.url.contains("/pm25") {
            ok(pm25.clone())
        } else {
            fail(503)
        }
    });
    let data = client.get_weather().unwrap();
    assert_eq!(data.psi, Some(146.0));
    assert_eq!(data.temperature, None);
    assert_eq!(data.forecast, "No forecast available");
}

// ---------- flights fixtures ----------

#[test]
fn aviationstack_fixture_normalizes_directions_and_speed() {
    let response: argus_core::api_types::AviationStackResponse =
        serde_json::from_str(include_str!("fixtures/aviationstack.json")).unwrap();
    let flights: Vec<_> = response
        .data
        .unwrap()
        .iter()
        .filter_map(argus_data::flights::from_aviation_stack)
        .collect();
    assert_eq!(flights.len(), 2);
    assert_eq!(flights[0].icao24, "76C0A1");
    assert_eq!(
        flights[0].direction,
        argus_core::types::FlightDirection::Inbound
    );
    assert!((flights[0].velocity.unwrap() - 800.0 / 3.6).abs() < 1e-6);
    assert_eq!(
        flights[1].direction,
        argus_core::types::FlightDirection::Outbound
    );
}

#[test]
fn opensky_fixture_filters_ground_aircraft() {
    let response: argus_core::api_types::OpenSkyResponse =
        serde_json::from_str(include_str!("fixtures/opensky.json")).unwrap();
    let flights: Vec<_> = response
        .states
        .unwrap()
        .iter()
        .filter_map(argus_data::flights::from_open_sky)
        .collect();
    assert_eq!(flights.len(), 1);
    assert_eq!(flights[0].callsign, "SIA321");
}

#[test]
fn flights_fallback_chain_and_last_good_snapshot() {
    let avia = include_str!("fixtures/aviationstack.json")
        .as_bytes()
        .to_vec();
    let sky = include_str!("fixtures/opensky.json").as_bytes().to_vec();
    let serve_empty = Arc::new(AtomicUsize::new(0));
    let open_sky_calls = Arc::new(AtomicUsize::new(0));
    let flag = Arc::clone(&serve_empty);
    let sky_empty = Arc::new(AtomicUsize::new(0));
    let sky_flag = Arc::clone(&sky_empty);
    let calls_open_sky = Arc::clone(&open_sky_calls);
    let client = client(move |req| {
        if req.url.contains("aviationstack") {
            if flag.load(Ordering::SeqCst) == 0 {
                ok(avia.clone())
            } else {
                ok(r#"{"data":[]}"#)
            }
        } else {
            calls_open_sky.fetch_add(1, Ordering::SeqCst);
            if sky_flag.load(Ordering::SeqCst) == 0 {
                ok(sky.clone())
            } else {
                ok(r#"{"time":1,"states":[]}"#)
            }
        }
    });
    let first = client.get_flights().unwrap();
    assert_eq!(first.len(), 2);
    assert_eq!(open_sky_calls.load(Ordering::SeqCst), 0);

    serve_empty.store(1, Ordering::SeqCst);
    client.cache().clear(Some("flights-sg"));
    let second = client.get_flights().unwrap();
    assert_eq!(second.len(), 1); // OpenSky fallback, ground row dropped
    assert_eq!(open_sky_calls.load(Ordering::SeqCst), 1);

    // Both providers empty now → last-good snapshot (the OpenSky result).
    sky_empty.store(1, Ordering::SeqCst);
    client.cache().clear(Some("flights-sg"));
    let third = client.get_flights().unwrap();
    assert_eq!(third, second);
}

#[test]
fn missing_aviation_key_falls_back_without_error() {
    let sky = include_str!("fixtures/opensky.json").as_bytes().to_vec();
    let client = Client::with_transport(
        ArgusConfig::new(None, None, None),
        Arc::new(FnTransport(move |req| {
            if req.url.contains("opensky") {
                ok(sky.clone())
            } else {
                fail(401)
            }
        })),
    );
    let flights = client.get_flights().unwrap();
    assert_eq!(flights.len(), 1);
    let health = client.source(Source::Flights);
    assert_eq!(health.status, HealthStatus::Ok);
    assert_eq!(
        health.message.as_deref(),
        Some("AVIATIONSTACK_API_KEY not configured")
    );
}

// ---------- news fixture ----------

#[test]
fn rss_fixture_parses_and_filters() {
    let items = parse_rss_items(include_str!("fixtures/news.xml"), "The Straits Times");
    // Two complete items; the one without a link is dropped.
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].title, "Singapore budget debate & plans");
    assert!(items[0].published_at.starts_with("2026-01-01"));
    assert!(is_singapore_relevant(&items[0]));
}

// ---------- feedback ----------

#[test]
fn feedback_submit_via_neon_http_endpoint() {
    use std::sync::Mutex;
    type SeenRequests = Arc<Mutex<Vec<(String, Vec<u8>)>>>;
    let seen: SeenRequests = Arc::new(Mutex::new(vec![]));
    let seen2 = Arc::clone(&seen);
    let database_url =
        "postgresql://user:pass@ep-test.ap-southeast-1.aws.neon.tech/argus?sslmode=require";
    let client = Client::with_transport(
        ArgusConfig::new(None, None, Some(database_url.to_string())),
        Arc::new(FnTransport(move |req| {
            seen2.lock().unwrap().push((req.url.clone(), vec![]));
            ok("{}")
        })),
    );
    let submission = argus_data::FeedbackSubmission::validate(5, Some("Great".into())).unwrap();
    client.submit_feedback(submission).unwrap();
    assert_eq!(
        seen.lock().unwrap()[0].0,
        "https://ep-test.ap-southeast-1.aws.neon.tech/sql"
    );
    assert_eq!(client.source(Source::Feedback).status, HealthStatus::Ok);

    let unconfigured = Client::with_transport(
        ArgusConfig::new(None, None, None),
        Arc::new(FnTransport(|_| ok("{}"))),
    );
    assert!(matches!(
        unconfigured.submit_feedback(argus_data::FeedbackSubmission::validate(3, None).unwrap()),
        Err(Error::NotConfigured { .. })
    ));
}
