//! ARGUS — Singapore OSINT dashboard, native Rune app.
//!
//! The view is split into `panels/*`; all mutations go through
//! `msg::dispatch`, and all external data is read through the `DataSource`
//! boundary in `live.rs`. Scenes power the checked-in headless snaps:
//!
//! ```sh
//! cargo run -p argus-app -- --list-scenes
//! cargo run -p argus-app -- --snap docs/snaps/dashboard.png --scene dashboard
//! cargo run -p argus-app -- --tree out/tree.json --scene panels
//! ```

mod fixtures;
mod hover;
mod images;
mod live;
mod mall_directory;
mod photo_gallery;
mod msg;
mod overlays;
mod panels;
mod search;
mod state;
mod theme;

use std::collections::HashSet;

use argus_core::signals::{merge_active_signals, select_notifiable};
use rune::{AppSpec, Cx, El, Tx, col, row};

use msg::{Msg, dispatch};
use state::AppState;

fn view(s: &AppState, cx: &mut Cx<AppState>) -> El {
    if s.collapsed_groups.contains("boot/loading") {
        return panels::dialogs::loading_screen(s);
    }

    let bottom = row().key("bottom-panels").h(246.0).w_full().min_h(0.0).gap(8.0)
        .child(panels::transit::mrt_panel(s, cx).grow(1.0).min_w(0.0))
        .child(panels::transit::bus_panel(s, cx).grow(1.0).min_w(0.0))
        .child(panels::feeds::news_panel(s, cx).grow(1.0).min_w(0.0))
        .child(panels::feeds::cameras_panel(s, cx).grow(1.0).min_w(0.0));

    let center = col().key("center").grow(1.0).min_w(0.0).min_h(0.0)
        .child(panels::map::view(s, cx))
        .child(bottom);

    let right = col().key("right").w(296.0).min_w(260.0).min_h(0.0).gap(8.0)
        .child(panels::intel::intel_panel(s, cx).grow(1.0).min_h(0.0))
        .child(panels::intel::source_health(s, cx).h(232.0).min_h(0.0))
        .child(panels::intel::radius_panel(s, cx).h(196.0).min_h(0.0));

    let mut root = col().key("root").w_full().h_full().fill(theme::PAPER)
        .child(panels::header::view(s, cx))
        .child(
            row().key("body").grow(1.0).min_h(0.0).w_full().padding(8.0).gap(8.0)
                .child(panels::sidebar::layers_sidebar(s, cx))
                .child(center)
                .child(right),
        )
        .child(panels::dialogs::footer(s, cx));

    root = root.child(panels::intel::toasts(s, cx));
    if let Some(dialog) = panels::dialogs::view(s, cx) {
        root = root.child(dialog);
    }
    root
}

fn setup(s: &mut AppState, waker: rune::Waker) {
    s.data.setup(waker.clone());
    s.map.setup(waker.clone());
    s.images.setup(waker);
    // Headless renders a single frame; preload basemap tiles synchronously so
    // the snap shows the real map instead of empty tile slots. The `live` /
    // `cameras` scenes additionally preload feeds and images synchronously.
    if rune::is_headless() {
        s.map.preload_sync();
        if s.preload_feeds {
            s.data.preload_sync();
            s.recompute();
        }
        if s.preload_images {
            let (photos, cameras) = s.image_urls();
            let mut all = photos;
            all.extend(cameras);
            s.images.preload_sync(&all);
        }
    }
}

fn poll(s: &mut AppState, now: f64, tx: &mut Tx) -> bool {
    let mut changed = false;

    // One-second header clock; this also keeps the app waking for prefs saves.
    if now - s.now >= 1.0 {
        s.now = now;
        let new_time = state::sg_clock();
        if new_time != s.clock_time {
            s.clock_time = new_time;
            changed = true;
        }
        tx.wake_at(now + 1.0);
    }

    changed |= s.data.poll(now, tx);
    changed |= s.map.poll();

    // Camera frames refresh with the camera feed (presigned URLs can stay the
    // same while the JPEG changes); place photos are fetched once per URL.
    let (photos, cameras) = s.image_urls();
    changed |= s.images.update(now, &photos, &cameras);
    changed |= s.images.poll(now);
    // A broken photo URL drops out of the gallery once, like the TS `onError`.
    let broken: Vec<String> = photos
        .into_iter()
        .filter(|url| s.images.image_failed(url) && !s.photo_gallery.failed.contains(url))
        .collect();
    for url in broken {
        s.photo_gallery.mark_failed(&url);
        changed = true;
    }
    for msg in s.map.drain_messages() {
        match msg {
            argus_map::MapMsg::OverlayClicked(id) => dispatch(s, Msg::MapClick(id)),
            argus_map::MapMsg::GeoFilterPicked(point) => dispatch(s, Msg::MapPickGeo([point.lon, point.lat])),
            argus_map::MapMsg::CameraChanged { .. } => {}
        }
        changed = true;
    }

    // Sample a 24-point weather history every five minutes, like the TS hook.
    let weather = &s.data.snapshot().weather;
    if weather.temperature.is_some()
        && (s.weather_history.is_empty() || now - s.last_pref_save >= 300.0)
    {
        s.weather_history.push((
            weather.temperature.unwrap_or(0.0),
            weather.humidity.unwrap_or(0.0),
            weather.psi.unwrap_or(0.0),
        ));
        if s.weather_history.len() > 24 {
            s.weather_history.remove(0);
        }
        s.last_pref_save = now;
        changed = true;
    }

    // Toast derivation: only notifiable, currently-active signals survive.
    let scoped: HashSet<String> = s.signals.iter().map(|row| row.id.clone()).collect();
    let mut fresh = select_notifiable(&s.signals, &s.notification_prefs, &scoped);
    fresh.retain(|row| !s.dismissed.contains(&row.id));
    if let Some(next) = merge_active_signals(&s.toasts, &fresh, &s.signals) {
        s.toasts = next;
        changed = true;
    }

    // Preferences are flushed at most every two seconds, never per frame.
    if s.prefs_dirty && now - s.last_pref_save >= 2.0 {
        live::save_prefs(&s.prefs_file());
        s.prefs_dirty = false;
        s.last_pref_save = now;
    }
    changed
}

fn main() {
    rune::run(AppSpec::<AppState> {
        scenes: &[
            ("dashboard", state::scene_dashboard),
            ("layers-on", state::scene_layers_on),
            ("route-planner", state::scene_route_planner),
            ("poi-dialog", state::scene_poi_dialog),
            ("mall-directory", state::scene_mall_directory),
            ("photo-gallery", state::scene_photo_gallery),
            ("hover-popup", state::scene_hover_popup),
            ("panels", state::scene_panels),
            ("live", state::scene_live),
            ("cameras", state::scene_cameras),
            ("camera-dialog", state::scene_camera_dialog),
            ("loading", |s| { s.collapsed_groups.insert("boot/loading".into()); }),
        ],
        setup: Some(setup),
        poll: Some(poll),
        ..AppSpec::new("ARGUS", (1440.0, 900.0), state::init, view)
    });
}

#[cfg(test)]
mod tests;
