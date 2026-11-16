//! Modal layers: focus dialog, map search, camera preview, feedback form.

use argus_core::maps_links::format_coordinates;
use rune::{Color, Cx, El, ScrollSpec, col, el, row, text, text_input};

use crate::mall_directory::build_view;
use crate::msg::{Msg, dispatch};
use crate::photo_gallery::{active_index, caption, usable};
use crate::overlays::amenity_label;
use crate::panels::common::*;
use crate::search;
use crate::state::{AppState, Selection};
use crate::theme::*;

fn modal_backdrop(key: &str, card: El) -> El {
    el().key(key).absolute(0.0, 0.0).w_full().h_full()
        .fill(Color { r: 0.0, g: 0.0, b: 0.0, a: 0.72 })
        .child(card)
}

fn modal_card(key: &str, x: f32, y: f32, w: f32, h: f32) -> El {
    col().key(key).w(w).h(h).min_h(0.0).fill(SURFACE)
        .border(1.0, LINE_STRONG).padding(0.0).absolute(x, y)
}

fn dialog_header(title: &str, subtitle: &str, close: Msg) -> El {
    row().key("dialog/header").h(52.0).w_full().min_h(0.0).padding(12.0).gap(8.0).center_y()
        .fill(SURFACE_RAISED)
        .child(col().grow(1.0).min_w(0.0).gap(2.0)
            .child(text(subtitle).font(MICRO).color(MUTED).truncate())
            .child(text(title).font(DIALOG_TITLE).color(INK).truncate()))
        .child(press("dialog/close", "ESC", close, false))
}

pub fn view(s: &AppState, _: &mut Cx<AppState>) -> Option<El> {
    if let Some(card) = focus_dialog(s) {
        return Some(card);
    }
    if s.search_open {
        return Some(search_dialog(s));
    }
    if let Some(card) = camera_dialog(s) {
        return Some(card);
    }
    if s.feedback.open {
        return Some(feedback_dialog(s));
    }
    None
}

fn focus_dialog(s: &AppState) -> Option<El> {
    let selection = s.selection.as_ref()?;
    let (subtitle, title, body) = match selection {
        Selection::Amenity { kind, id } => {
            let found = s.registry.as_ref().and_then(|r| r.find(*kind, id));
            let Some((_, place)) = found else { return None };
            let mut body = col().key("focus/body").w_full().min_h(0.0).gap(4.0).padding(12.0);
            for line in place.describe() {
                body = body.child(row().w_full().gap(6.0)
                    .child(text(&line.text).font(TINY).color(if line.tone.is_some() { INK } else { MUTED }).max_lines(3)));
            }
            if let Some(address) = place.address() {
                body = body.child(key_value("focus/address", "Address", address, INK));
            }
            if let Some(coords) = format_coordinates(Some(place.lat()), Some(place.lng())) {
                body = body.child(key_value("focus/coords", "Coords", coords, INK));
            }
            // Also-here: other places sharing this spot.
            let others: Vec<_> = s.registry.as_ref()
                .and_then(|r| r.group_by_key.get(&argus_core::amenity_layers::amenity_display_key(kind.as_str(), id)))
                .map(|group| group.iter().filter(|o| o.id != *id).take(4).cloned().collect())
                .unwrap_or_default();
            if !others.is_empty() {
                body = body.child(rule_key("focus/rule/also-here")).child(label(format!("Also here ({})", others.len())));
                for other in others {
                    let target = other.key.rsplit_once(':').map(|(_, id)| id.to_string());
                    body = body.child(text(&other.key).font(TINY).color(MUTED).truncate());
                    if let Some(id) = target {
                        let _ = id; // the full registry key is shown; selection requires a typed kind
                    }
                }
            }
            if *kind == argus_core::types::AmenityType::Mall {
                body = body.child(rule_key("focus/rule/mall-dir")).child(mall_store_directory(s, id, place.name()));
            }
            if !place.photos().is_empty() {
                body = body.child(rule_key("focus/rule/photos")).child(photo_gallery(s, place.name()));
            }
            (amenity_label(*kind).to_string(), place.name().to_string(), body)
        }
        Selection::Flight(id) => {
            let flight = s.data.snapshot().flights.iter().find(|f| &f.id == id)?.clone();
            let mut body = col().key("focus/body").w_full().min_h(0.0).gap(4.0).padding(12.0);
            body = body.child(key_value("flight/dir", "Direction", direction_label(flight.direction), INK));
            body = body.child(key_value("flight/alt", "Altitude", argus_core::format::format_altitude_feet(flight.altitude), INK));
            body = body.child(key_value("flight/speed", "Speed", argus_core::format::format_speed_kmh(flight.velocity), INK));
            body = body.child(key_value("flight/track", "Track", flight.track.map(|t| format!("{}°", t.round())).unwrap_or_else(|| "N/A".into()), INK));
            ("Flight".into(), flight.callsign, body)
        }
        Selection::Park { name, kind } => {
            let feature = s.static_data.as_ref().and_then(|d| {
                d.parks_geojson.features.iter().find(|f| &f.properties.name == name)
            });
            let mut body = col().key("focus/body").w_full().min_h(0.0).gap(4.0).padding(12.0);
            if let Some(f) = feature {
                let p = &f.properties;
                let details = argus_core::parks::ParkFeatureDetails {
                    kind: p.kind,
                    name: p.name.clone(),
                    park: p.park.clone(),
                    kind_type: p.kind_type.clone(),
                    cycle: p.cycle,
                    area_ha: p.area_ha,
                    photos: &[],
                };
                for line in argus_core::parks::describe_park_feature(&details) {
                    body = body.child(text(&line.text).font(TINY).color(MUTED).max_lines(3));
                }
            }
            if !s.gallery_photos.is_empty() {
                body = body.child(rule_key("focus/rule/photos")).child(photo_gallery(s, name));
            }
            use argus_core::types::ParkFeatureKind;
            let label = match kind {
                ParkFeatureKind::Park => "Park",
                ParkFeatureKind::Pcn => "Park Connector",
                ParkFeatureKind::Trail => "Trail",
            };
            (label.into(), name.clone(), body)
        }
        Selection::Camera(id) => {
            let cam = s.data.snapshot().cameras.iter().find(|c| &c.camera_id == id)?;
            let body = col().key("focus/body").w_full().min_h(0.0).padding(12.0)
                .child(key_value("cam/location", "Location", cam.location.clone(), INK));
            ("Camera".into(), cam.location.clone(), body)
        }
        Selection::BusStop(code) => {
            let stop = s.data.snapshot().bus_stops.iter().find(|b| &b.bus_stop_code == code)?;
            let body = col().key("focus/body").w_full().min_h(0.0).padding(12.0)
                .child(key_value("stop/road", "Road", stop.road_name.clone(), INK))
                .child(key_value("stop/code", "Code", stop.bus_stop_code.clone(), INK));
            ("Bus Stop".into(), stop.description.clone(), body)
        }
        Selection::Carpark(id) => {
            let cp = s.data.snapshot().carparks.iter().find(|c| &c.id == id)?;
            let body = col().key("focus/body").w_full().min_h(0.0).padding(12.0)
                .child(key_value("cp/lots", "Lots", cp.car_lots.map(|v| v.to_string()).unwrap_or_else(|| "—".into()), SUCCESS));
            ("Carpark".into(), cp.name.clone(), body)
        }
        Selection::Ev(id) => {
            let ev = s.data.snapshot().ev_chargers.iter().find(|e| &e.id == id)?;
            let body = col().key("focus/body").w_full().min_h(0.0).padding(12.0)
                .child(key_value("ev/available", "Available", format!("{}/{}", ev.available, ev.total), SUCCESS));
            ("EV Charging".into(), ev.name.clone(), body)
        }
        Selection::Incident(id) => {
            let inc = s.data.snapshot().incidents.iter().find(|i| &i.id == id)?;
            let body = col().key("focus/body").w_full().min_h(0.0).padding(12.0)
                .child(text(&inc.message).font(TINY).color(MUTED).max_lines(4));
            ("Road Incident".into(), inc.incident_type.clone(), body)
        }
        Selection::MrtStation(name) => {
            let body = col().key("focus/body").w_full().min_h(0.0).padding(12.0)
                .child(hint("Pick this station for the MRT router from the map."));
            ("MRT Station".into(), name.clone(), body)
        }
    };

    let card = modal_card("focus/card", 450.0, 70.0, 540.0, 770.0)
        .child(dialog_header(&title, &format!("FOCUS · {subtitle}"), Msg::ClearSelection))
        .child(col().key("focus/scroll").w_full().min_h(0.0).grow(1.0).scroll(rune::ScrollSpec { stick_bottom: false })
            .child(body));
    Some(modal_backdrop("focus/backdrop", card))
}

/// Photo viewer for the focus dialog: one large image, three thumbnails and
/// the attribution required by the active photo's licence.
fn photo_gallery(s: &AppState, name: &str) -> El {
    let photos = usable(&s.gallery_photos, &s.photo_gallery);
    if photos.is_empty() {
        return el();
    }
    let active_i = active_index(&s.photo_gallery, photos.len());
    let active = photos[active_i];

    let mut large = el().key("gallery/large").w_full().h(112.0).min_h(0.0).fill(SURFACE_RAISED)
        .border(1.0, LINE);
    if let Some(image) = s.images.image(&active.thumb) {
        large = large.pixels_fit(image.pixels(), rune::SurfaceFit::Fill)
            .label(format!("{name}, photo {} of {}", active_i + 1, photos.len()));
    } else {
        large = large.child(hint("Loading photo…"));
    }

    let mut out = col().key("gallery").w_full().min_h(0.0).gap(6.0)
        .child(label("PHOTOS"))
        .child(large);

    if photos.len() > 1 {
        let mut thumbs = row().key("gallery/thumbs").w_full().gap(6.0);
        for (i, photo) in photos.iter().take(3).enumerate() {
            let is_active = i == active_i;
            let msg = Msg::SetPhotoIndex(i);
            let mut thumb = el().key(format!("gallery/thumb/{i}")).w(120.0).h(70.0)
                .min_w(0.0).fill(SURFACE_RAISED)
                .border(1.0, if is_active { INK } else { LINE })
                .on_press(move |s: &mut AppState| dispatch(s, msg.clone()));
            if let Some(image) = s.images.image(&photo.thumb) {
                thumb = thumb.pixels_fit(image.pixels(), rune::SurfaceFit::Fill)
                    .label(format!("Show photo {} of {}", i + 1, photos.len()));
            }
            thumbs = thumbs.child(thumb);
        }
        out = out.child(thumbs);
    }
    out.child(text(caption(active)).font(MICRO).color(MUTED).max_lines(2))
}

/// The mall store directory embedded in a mall's focus dialog (the TS app
/// fetched it with SWRImmutable; here `AppState::recompute` loads it from the
/// local bundle once per mall).
fn mall_store_directory(s: &AppState, mall_id: &str, mall_name: &str) -> El {
    let Some(data) = s.static_data.as_ref() else {
        return hint("Store directory unavailable.");
    };
    if s.mall_stores_for.as_deref() != Some(mall_id) {
        return hint("Loading store directory…");
    }
    if !data.mall_store_index.malls.contains_key(mall_id) {
        return hint("No store directory is available for this mall.");
    }
    let view = build_view(&s.mall_stores, &s.mall_directory);

    let mut head = col().key("mall-dir/head").w_full().min_h(0.0).gap(6.0).padding(6.0).fill(SURFACE_RAISED);
    let count = if view.shown != view.total {
        format!("{} of {} shops", view.shown, view.total)
    } else {
        format!("{} shops", view.total)
    };
    head = head.child(row().w_full().gap(6.0)
        .child(label("STORE DIRECTORY").grow(1.0))
        .child(text(count).font(MICRO_MONO).color(MUTED)));

    let field = text_input(&s.mall_directory.query, rune::TextStyle::new(13.0, 400.0))
        .key("mall-dir/query").single_line().w_full().min_h(24.0).color(INK)
        .placeholder(&format!("Search {mall_name} shops or units…"), FAINT)
        .label(format!("Search {mall_name} shops"))
        .on_edit(|s: &mut AppState| &mut s.mall_directory.query);
    head = head.child(row().key("mall-dir/filter").h(26.0).w_full().gap(6.0).fill(PAPER)
        .child(text("⌕").font(BODY).color(FAINT)).child(field));

    let mut chips = row().key("mall-dir/categories").w_full().gap(4.0).wrap();
    let all_msg = Msg::SetMallStoreCategory(None);
    chips = chips.child(press("mall-cat/all", format!("All ({})", view.total), all_msg,
        s.mall_directory.category.is_none()));
    for (name, n) in view.categories.iter().take(8) {
        let msg = Msg::SetMallStoreCategory(Some(name.clone()));
        chips = chips.child(press(&format!("mall-cat/{name}"), format!("{} ({n})", truncate_label(name, 32)),
            msg, s.mall_directory.category.as_deref() == Some(name.as_str())));
    }
    if view.has_halal {
        chips = chips.child(press("mall-cat/halal", "HALAL", Msg::ToggleMallStoreHalal,
            s.mall_directory.halal_only));
    }
    head = head.child(chips);

    let mut list = col().key("mall-dir/list").w_full().h(140.0).min_h(0.0).gap(2.0)
        .scroll(ScrollSpec { stick_bottom: false });
    if view.groups.is_empty() {
        list = list.child(hint("No shops match"));
    }
    for group in &view.groups {
        list = list.child(row().w_full().gap(6.0).padding(4.0).fill(SURFACE_RAISED)
            .child(text(&group.label).font(MICRO).color(INK).grow(1.0))
            .child(text(group.stores.len().to_string()).font(MICRO_MONO).color(MUTED)));
        for store in &group.stores {
            let mut row_el = row().key(format!("mall-store/{}", store.name)).w_full()
                .min_h(0.0).padding(4.0).gap(6.0).border(1.0, LINE)
                .child(col().grow(1.0).min_w(0.0).gap(1.0)
                    .child(text(&store.name).font(SMALL).color(INK).truncate())
                    .child(text(store.category.as_deref().unwrap_or("")).font(MICRO).color(MUTED).truncate()));
            if store.halal {
                row_el = row_el.child(text("HALAL").font(MICRO).color(SUCCESS));
            }
            if let Some(unit) = &store.unit {
                row_el = row_el.child(text(unit).font(MICRO_MONO).color(MUTED));
            }
            list = list.child(row_el);
        }
    }

    let fetched = &data.mall_store_index.fetched_at;
    col().key("mall-dir").w_full().min_h(0.0).gap(4.0)
        .child(head)
        .child(list)
        .child(text(format!("As listed by the mall operator ({fetched}) · curioputterings/singapore-mall-data (CC BY 4.0)"))
            .font(MICRO).color(FAINT).max_lines(2))
}

fn truncate_label(label: &str, max: usize) -> String {
    if label.chars().count() <= max {
        label.to_string()
    } else {
        let prefix: String = label.chars().take(max.saturating_sub(1)).collect();
        format!("{prefix}…")
    }
}

fn search_dialog(s: &AppState) -> El {
    let results = search::visible(s, &s.station_query.text);
    let query = &s.search_query;
    let field = text_input(query, rune::TextStyle::new(15.0, 400.0))
        .key("search/input").single_line().grow(1.0).min_w(0.0).color(INK)
        .placeholder("Search map locations…", FAINT).label("Search map locations")
        .on_edit(|s: &mut AppState| &mut s.search_query);
    let mut list = col().key("search/results").w_full().min_h(0.0).grow(1.0)
        .scroll(rune::ScrollSpec { stick_bottom: false }).padding(6.0).gap(3.0);
    if results.is_empty() {
        list = list.child(hint("No locations found"));
    }
    for result in results.iter().take(24) {
        let msg = Msg::SelectSearchResult(result.id.clone());
        list = list.child(
            row().key(format!("search/result/{}", result.id)).w_full().h(30.0).padding(8.0).gap(6.0)
                .fill(SURFACE_RAISED).on_press(move |s: &mut AppState| dispatch(s, msg.clone()))
                .child(text(&result.name).font(SMALL).color(INK).truncate().grow(1.0))
                .child(text(result_type_label(&result.result_type)).font(MICRO).color(MUTED)),
        );
    }
    let card = modal_card("search/card", 410.0, 220.0, 620.0, 460.0)
        .child(dialog_header("Search", "MAP SEARCH", Msg::OpenSearch(false)))
        .child(row().key("search/field").h(44.0).w_full().padding(10.0).gap(8.0).fill(PAPER)
            .child(text("⌕").font(rune::TextStyle::new(16.0, 400.0)).color(FAINT))
            .child(field))
        .child(list);
    modal_backdrop("search/backdrop", card)
}

fn camera_dialog(s: &AppState) -> Option<El> {
    let id = s.camera_dialog.as_ref()?;
    let cam = s.data.snapshot().cameras.iter().find(|c| &c.camera_id == id)?.clone();
    let mut preview = el().key("camera/frame").w_full().grow(1.0).min_h(0.0).fill(PAPER);
    if let Some(image) = s.images.image(&cam.image_link) {
        preview = preview.pixels_fit(image.pixels(), rune::SurfaceFit::Fill);
    } else {
        preview = preview.child(hint("Waiting for camera frame…"));
    }
    let card = modal_card("camera/card", 340.0, 130.0, 760.0, 620.0)
        .child(dialog_header(&cam.location, "TRAFFIC CAMERA", Msg::CloseCamera))
        .child(col().key("camera/preview").w_full().grow(1.0).min_h(0.0).padding(12.0).gap(8.0)
            .child(preview)
            .child(row().key("camera/links").gap(6.0).center_y()
                .child(text(format!("{:.5} N, {:.5} E", cam.latitude, cam.longitude)).font(TINY_MONO).color(INFO))
                .child(text("© LTA").font(MICRO).color(FAINT))
                .child(el().grow(1.0))
                .child(press("camera/refresh", "REFRESH", Msg::RefreshCameraImages, false))
                .child(press("camera/close", "CLOSE", Msg::CloseCamera, false))));
    Some(modal_backdrop("camera/backdrop", card))
}

fn feedback_dialog(s: &AppState) -> El {
    let categories = ["Bug", "Feedback", "Data issue"];
    let mut body = col().key("feedback/body").w_full().min_h(0.0).gap(6.0).padding(12.0);
    if s.feedback.sent {
        body = body.child(text("Thanks — your report was recorded locally.").font(SMALL).color(SUCCESS));
    } else {
        body = body.child(label("CATEGORY"));
        let mut cats = row().gap(6.0);
        for (i, c) in categories.iter().enumerate() {
            cats = cats.child(press("feedback/category", *c, Msg::SetFeedbackCategory(i), s.feedback.category == i));
        }
        body = body.child(cats);
        let field = text_input(&s.feedback.message, rune::TextStyle::new(13.0, 400.0))
            .key("feedback/message").w_full().min_h(90.0).color(INK)
            .placeholder("Describe what you saw…", FAINT).label("Feedback message")
            .on_edit(|s: &mut AppState| &mut s.feedback.message);
        body = body.child(field);
        body = body.child(row().gap(6.0)
            .child(press("feedback/send", "SEND", Msg::SendFeedback, false))
            .child(press("feedback/cancel", "CANCEL", Msg::OpenFeedback(false), false)));
    }
    let card = modal_card("feedback/card", 480.0, 300.0, 480.0, 300.0)
        .child(dialog_header("Feedback", "REPORT AN ISSUE", Msg::OpenFeedback(false)))
        .child(body);
    modal_backdrop("feedback/backdrop", card)
}

pub fn direction_label(d: argus_core::types::FlightDirection) -> &'static str {
    use argus_core::types::FlightDirection;
    match d {
        FlightDirection::Inbound => "INBOUND",
        FlightDirection::Outbound => "OUTBOUND",
        FlightDirection::Transit => "TRANSIT",
    }
}

fn result_type_label(t: &argus_core::types::SearchResultType) -> &'static str {
    use argus_core::types::SearchResultType;
    match t {
        SearchResultType::BusStop => "STOP",
        SearchResultType::Mrt => "MRT",
        SearchResultType::Park => "PARK",
        SearchResultType::Amenity(_) => "PLACE",
    }
}

/// Footer: a single right-aligned source link and a feedback entry point.
pub fn footer(_s: &AppState, _: &mut Cx<AppState>) -> El {
    row().key("footer").h(20.0).w_full().padding(6.0).gap(12.0).center_y()
        .child(el().grow(1.0))
        .child(text("ARGUS v0.1 · RUST/RUNE").font(MICRO).color(FAINT))
        .child(press("footer/feedback", "FEEDBACK", Msg::OpenFeedback(true), false))
        .child(text("github.com/sxeptical/ARGUS").font(MICRO).color(MUTED))
}

/// Boot screen with source status list (shown until the required feeds settle).
pub fn loading_screen(s: &AppState) -> El {
    let snap = s.data.snapshot();
    let settled = snap.sources.iter().filter(|x| x.status.is_settled()).count();
    let pct = if snap.sources.is_empty() { 0.0 } else { settled as f32 / snap.sources.len() as f32 };
    let mut out = col().key("loading").w_full().h_full().center().gap(12.0).fill(PAPER)
        .child(text("ARGUS").font(TITLE).color(INK))
        .child(text("SINGAPORE SIGNAL MONITOR").font(MICRO).color(MUTED))
        .child(text(format!("{}% · connecting to data sources", (pct * 100.0) as i64)).font(TINY_MONO).color(MUTED))
        .child(row().w(360.0).h(4.0).fill(LINE).child(el().w_frac(pct).h(4.0).fill(INFO)));
    for source in snap.sources.iter() {
        out = out.child(row().w(360.0).gap(8.0)
            .child(text(source.label).font(TINY).color(MUTED).grow(1.0))
            .child(text(source.status.label()).font(MICRO).color(MUTED)));
    }
    out
}
