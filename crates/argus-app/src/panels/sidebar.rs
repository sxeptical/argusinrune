//! Left column: layer groups, sub-filters, weather and flights.

use argus_core::amenity_layers::visible_places;
use argus_core::geo::within_radius;
use argus_core::layers::{LayerGroupId, LayerRow, build_layer_groups};
use argus_core::types::ParkFeatureKind;
use rune::{Color, Cx, El, ScrollSpec, Shape, TextEdit, canvas, col, el, row, text, text_input};

use crate::msg::{Msg, dispatch, subfilter_summary};
use crate::panels::common::*;
use crate::state::{AirWindow, AppState};
use crate::theme::*;

fn count_for(s: &AppState, key: &str) -> usize {
    let snap = s.data.snapshot();
    let filter = s.geo_filter();
    match key {
        "flights" => within_radius(snap.flights.clone(), filter.as_ref(), |f| [f.longitude, f.latitude]).len(),
        "cameras" => within_radius(snap.cameras.clone(), filter.as_ref(), |c| [c.longitude, c.latitude]).len(),
        "busStops" => within_radius(snap.bus_stops.clone(), filter.as_ref(), |b| [b.longitude, b.latitude]).len(),
        "incidents" => within_radius(snap.incidents.clone(), filter.as_ref(), |i| [i.lng, i.lat]).len(),
        "carparks" => within_radius(snap.carparks.clone(), filter.as_ref(), |c| [c.lng, c.lat]).len(),
        "evChargers" => within_radius(snap.ev_chargers.clone(), filter.as_ref(), |e| [e.lng, e.lat]).len(),
        "mrt" => *argus_core::mrt_network::MRT_DISPLAY_LINE_COUNT,
        "parks" => s.static_data.as_ref().map(|d| {
            d.parks_geojson.features.iter().filter(|f| f.properties.kind == ParkFeatureKind::Park).count()
        }).unwrap_or(0),
        _ => s.registry.as_ref().and_then(|r| {
            r.layers.iter().find(|l| l.layer_key == key).map(|layer| {
                within_radius(visible_places(layer, &s.amenity_filters), filter.as_ref(), |p| [p.lng(), p.lat()]).len()
            })
        }).unwrap_or(0),
    }
}

fn eye(on: bool) -> El {
    text(if on { "◉" } else { "○" }).font(BODY).color(if on { SUCCESS } else { FAINT })
}

fn layer_row(s: &AppState, key: &str, label_str: &str, note: &str) -> El {
    let on = s.visibility.get(key).copied().unwrap_or(false);
    let msg = Msg::ToggleLayer(key.to_string());
    let mut out = row().key(format!("layer/{key}")).w_full().min_h(0.0).padding(6.0).gap(6.0)
        .fill(SURFACE_RAISED).border(1.0, LINE).center_y()
        .child(
            col().key(format!("layer/{key}/text")).grow(1.0).min_w(0.0).gap(1.0)
                .child(text(label_str).font(SMALL).color(if on { INK } else { MUTED }).truncate())
                .child(text(note).font(MICRO).color(FAINT).truncate()),
        )
        .child(text(count_for(s, key).to_string()).font(TINY_MONO).color(if on { SUCCESS } else { FAINT }))
        .child(
            row().key(format!("layer/{key}/eye")).h(22.0).w(24.0).center()
                .border(1.0, if on { SUCCESS_LINE } else { LINE })
                .on_press(move |s: &mut AppState| dispatch(s, msg.clone()))
                .child(eye(on)),
        );
    // Amenity sub-filter chips, collapsed by default like the TS app.
    if on {
        if let Some(layer) = s.registry.as_ref().and_then(|r| r.layers.iter().find(|l| l.layer_key == key)) {
            if let Some(filter_def) = layer.filter.as_ref() {
                let kind = layer.amenity_type;
                let open = s.layer_subfilter_open.contains(key);
                let subfilter_key = key.to_string();
                out = out.child(
                    row().key(format!("subfilter/{key}")).h(22.0).w_full().padding(4.0).gap(6.0)
                        .on_press(move |s: &mut AppState| dispatch(s, Msg::ToggleSubFilter(subfilter_key.clone())))
                        .child(text(format!("{}: {}", filter_def.label, subfilter_summary(s, kind)))
                            .font(MICRO).color(MUTED).truncate())
                        .child(text(if open { "▴" } else { "▾" }).font(MICRO).color(FAINT)),
                );
                if open {
                    let mut chips = row().key(format!("subfilter/{key}/chips")).w_full().gap(4.0).wrap();
                    for choice in filter_def.choices.iter().take(8) {
                        let selected = s.amenity_filters.get(&kind)
                            .and_then(|m| m.get(&choice.value)).copied().unwrap_or(true);
                        let msg = Msg::ToggleSubOption { kind, value: choice.value.clone() };
                        chips = chips.child(
                            row().key(format!("subfilter/{key}/{}", choice.value)).h(20.0)
                                .padding(5.0).center()
                                .fill(if selected { alpha(hex_color(&layer.color), 0.22) } else { SURFACE })
                                .border(1.0, if selected { hex_color(&layer.color) } else { LINE })
                                .on_press(move |s: &mut AppState| dispatch(s, msg.clone()))
                                .child(text(format!("{} {}", choice.label, choice.count)).font(MICRO)
                                    .color(if selected { INK } else { FAINT }).truncate()),
                        );
                    }
                    out = out.child(chips);
                }
            }
        }
    }
    // Wrap everything in a column so the chips sit below the row.
    col().key(format!("layerbox/{key}")).w_full().min_h(0.0).gap(0.0).child(out)
}

fn group_header(s: &AppState, id: LayerGroupId, label: &str, keys: &[String], count: usize, total: usize) -> El {
    let collapsed = s.collapsed_groups.contains(id.as_str());
    let toggle_msg = Msg::ToggleGroupCollapse(id.as_str().to_string());
    let vis_msg = Msg::ToggleGroupVisibility(keys.to_vec());
    let any_on = keys.iter().any(|k| s.visibility.get(k).copied().unwrap_or(false));
    row().key(format!("group/{id:?}")).w_full().h(24.0).padding(4.0).gap(6.0).center_y()
        .child(
            row().key(format!("group/{id:?}/collapse")).grow(1.0).min_w(0.0).gap(5.0).center_y()
                .on_press(move |s: &mut AppState| dispatch(s, toggle_msg.clone()))
                .child(text(if collapsed { "▸" } else { "▾" }).font(MICRO).color(MUTED))
                .child(text(label).font(MICRO).color(INK).truncate()),
        )
        .child(text(format!("{count}/{total}")).font(MICRO_MONO).color(MUTED))
        .child(
            row().key(format!("group/{id:?}/eye")).w(20.0).h(20.0).center()
                .on_press(move |s: &mut AppState| dispatch(s, vis_msg.clone()))
                .child(eye(any_on)),
        )
}

pub fn layers_sidebar(s: &AppState, _: &mut Cx<AppState>) -> El {
    let rows: Vec<LayerRow> = argus_core::layers::LAYERS.iter().map(|l| LayerRow {
        key: l.key.clone(), label: l.label.clone(), note: l.note.clone(),
    }).collect();
    let normalized = s.layer_filter.text.trim().to_lowercase();
    let groups = build_layer_groups(&rows, &s.visibility, &normalized);

    let mut body = col().key("layers/list").w_full().min_h(0.0).grow(1.0).gap(6.0)
        .scroll(ScrollSpec { stick_bottom: false }).padding(6.0);
    if groups.is_empty() {
        body = body.child(hint("No layers match your filter."));
    }
    for group in groups {
        let keys: Vec<String> = group.rows.iter().map(|r| r.key.clone()).collect();
        body = body.child(group_header(s, group.id, &group.label, &keys,
            group.visible_count, group.rows.len()));
        let collapsed = s.collapsed_groups.contains(group.id.as_str()) && normalized.is_empty();
        if !collapsed {
            for row_def in &group.rows {
                body = body.child(layer_row(s, &row_def.key, &row_def.label, &row_def.note));
            }
        }
    }
    for stat in [
        ("INBOUND FLIGHTS", count_for(s, "flights") / 3),
        ("OSINT FEED", s.data.snapshot().news.len()),
    ] {
        body = body.child(row().key(format!("stat/{}", stat.0)).w_full().padding(5.0).gap(6.0)
            .child(label(stat.0).grow(1.0))
            .child(value(stat.1.to_string(), INK)));
    }

    let filter_field = text_input(&s.layer_filter, TINY)
        .key("layer-filter").single_line().grow(1.0).min_w(0.0)
        .color(INK).placeholder("Filter layers…", FAINT).label("Filter layers")
        .on_edit(|s: &mut AppState| &mut s.layer_filter);

    let mut panel = panel("layers", "Layers", format!("{}/{}", s.visible_layer_count, s.layer_count))
        .grow(1.0).min_h(0.0);
    panel = panel.child(row().key("layers/filter").h(26.0).w_full().padding(5.0).gap(5.0)
        .fill(PAPER).border(1.0, LINE).center_y()
        .child(text("⌕").font(BODY).color(FAINT))
        .child(filter_field));
    panel = panel.child(body);

    col().key("sidebar").w(264.0).min_w(220.0).min_h(0.0).gap(8.0)
        .child(panel)
        .child(weather_panel(s))
        .child(flights_panel(s))
}

fn mini_trend(values: &[(f64, f64, f64)], kind: usize) -> Shape {
    let mut shape = Shape::new();
    if values.is_empty() {
        return shape.view_box(200.0, 28.0);
    }
    let recent: Vec<f64> = values.iter().rev().take(24).map(|v| v.2 - kind as f64 * 12.0).collect();
    let min = recent.iter().copied().fold(f64::INFINITY, f64::min);
    let max = recent.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let range = (max - min).max(1.0);
    let w = 200.0 / recent.len() as f32;
    for (i, v) in recent.iter().enumerate() {
        let h = 4.0 + ((v - min) / range) as f32 * 22.0;
        let x = i as f32 * w + 1.0;
        shape = shape.fill(
            vec![[x, 28.0], [x + w - 2.0, 28.0], [x + w - 2.0, 28.0 - h], [x, 28.0 - h]],
            Color { a: 0.75, ..INFO },
        );
    }
    shape.view_box(200.0, 28.0)
}

fn weather_panel(s: &AppState) -> El {
    let w = &s.data.snapshot().weather;
    let air_text = if s.air_window == AirWindow::OneHour {
        match w.pm25_one_hour {
            Some(v) => format!("1-hr PM2.5 · {} ({})", v.round() as i64, pm25_band_str(w.pm25_one_hour_band)),
            None => "1-hr PM2.5 · —".into(),
        }
    } else {
        match w.psi {
            Some(v) => format!("24-hr PSI · {} ({})", v.round() as i64, w.psi_status.as_str()),
            None => "24-hr PSI · —".into(),
        }
    };
    let air_color = if w.psi.unwrap_or(0.0) > 100.0 { WARNING } else { SUCCESS };
    let out = panel("weather", "Weather", w.last_updated.chars().take(16).collect::<String>())
        .h(212.0).min_h(0.0);
    let mut body = col().key("weather/body").w_full().min_h(0.0).gap(4.0).padding(6.0);
    body = body.child(row().w_full().gap(8.0).child(label("TEMPERATURE").grow(1.0))
        .child(value(format!("{}°C", fmt_num(w.temperature)), INK)));
    body = body.child(row().w_full().gap(8.0).child(label("HUMIDITY").grow(1.0))
        .child(value(format!("{}%", fmt_num(w.humidity)), INK)));
    body = body.child(row().w_full().gap(8.0).child(label("UV INDEX").grow(1.0))
        .child(value(format!("{} ({})", fmt_num(w.uv), w.uv_status.as_str()), if w.uv.unwrap_or(0.0) >= 6.0 { WARNING } else { INK })));
    body = body.child(row().w_full().gap(8.0).child(label("AIR QUALITY").grow(1.0))
        .child(value(air_text, air_color)));
    body = body.child(row().w_full().gap(6.0).child(press("air/window", if s.air_window == AirWindow::OneHour { "1H" } else { "24H" },
        Msg::SetAirWindow(if s.air_window == AirWindow::OneHour { AirWindow::TwentyFourHour } else { AirWindow::OneHour }), false))
        .child(press("air/pin", if s.air_pinned { "UNPIN" } else { "PIN MAP" }, Msg::ToggleAirPinned, s.air_pinned)));
    if !s.weather_history.is_empty() {
        body = body.child(canvas(mini_trend(&s.weather_history, 0)).key("weather/trend").w_full().h(28.0).fill(PAPER));
    }
    body = body.child(text(&w.forecast).font(TINY).color(MUTED).max_lines(2));
    out.child(scroll_body("weather", body))
}

fn pm25_band_str(b: argus_core::types::Pm25Band) -> &'static str {
    use argus_core::types::Pm25Band;
    match b {
        Pm25Band::Normal => "Normal",
        Pm25Band::Elevated => "Elevated",
        Pm25Band::High => "High",
        Pm25Band::VeryHigh => "Very High",
        Pm25Band::Unknown => "Unknown",
    }
}

fn fmt_num(v: Option<f64>) -> String {
    v.map(|n| if n.fract() == 0.0 { format!("{}", n as i64) } else { format!("{n:.1}") })
        .unwrap_or_else(|| "—".into())
}

fn flights_panel(s: &AppState) -> El {
    let snap = s.data.snapshot();
    let summary = argus_core::flights::summarize_flights(&snap.flights);
    let out = panel("flights", "Flights (SG airspace)", format!("{} tracks", snap.flights.len()));
    let mut body = col().key("flights/body").w_full().min_h(0.0).gap(4.0).padding(6.0);
    body = body.child(row().w_full().gap(6.0)
        .child(value(format!("IN {}", summary.inbound), SUCCESS).grow(1.0))
        .child(value(format!("OUT {}", summary.outbound), DANGER).grow(1.0))
        .child(value(format!("TRANSIT {}", summary.transit), INFO).grow(1.0)));
    if snap.flights.is_empty() {
        body = body.child(hint("No live aircraft in the current bounds."));
    }
    for flight in snap.flights.iter().take(6) {
        let sel = s.selection == Some(crate::state::Selection::Flight(flight.id.clone()));
        let msg = Msg::Select(crate::state::Selection::Flight(flight.id.clone()));
        body = body.child(
            row().key(format!("flight/{}", flight.id)).w_full().min_h(0.0).padding(5.0).gap(4.0)
                .fill(if sel { SURFACE_HOVER } else { SURFACE_RAISED })
                .on_press(move |s: &mut AppState| dispatch(s, msg.clone()))
                .child(text(&flight.callsign).font(TINY_MONO).color(INK).w(64.0))
                .child(text(crate::panels::dialogs::direction_label(flight.direction)).font(MICRO)
                    .color(match flight.direction { argus_core::types::FlightDirection::Inbound => SUCCESS,
                        argus_core::types::FlightDirection::Outbound => DANGER, _ => INFO }).w(52.0))
                .child(text(format!("{}ft", argus_core::format::format_altitude_feet(flight.altitude)))
                    .font(MICRO).color(MUTED).truncate()),
        );
    }
    out.child(body)
}

/// Dummy `TextEdit` factory for tests / scenes that need an empty field.
#[allow(dead_code)]
pub fn empty_edit() -> TextEdit {
    TextEdit::new("")
}

/// Colour helper re-export used by other panels.
#[allow(dead_code)]
pub fn tone(color: &str) -> Color {
    tone_color(color)
}

/// Keep `el` imported for future branched layouts.
#[allow(dead_code)]
fn _unused() -> El { el() }
