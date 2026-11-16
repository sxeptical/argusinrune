//! Bottom-row transit panels: MRT route planner and bus arrivals.

use argus_core::format::format_sg_time;
use argus_core::mrt_routing::MRT_STATION_NAMES;
use rune::{Cx, El, col, row, text, text_input};

use crate::msg::{Msg, dispatch};
use crate::panels::common::*;
use crate::state::{AppState, MrtPickTarget};
use crate::theme::*;

pub fn mrt_panel(s: &AppState, _: &mut Cx<AppState>) -> El {
    let out = panel(
        "mrt-router",
        "MRT Router",
        s.route
            .plan
            .as_ref()
            .map(|p| format!("{} min", p.estimated_minutes.round() as i64))
            .unwrap_or_else(|| "no route".into()),
    );
    let mut body = col()
        .key("mrt/body")
        .w_full()
        .min_h(0.0)
        .gap(5.0)
        .padding(6.0);

    body = body.child(station_field(
        s,
        MrtPickTarget::Start,
        "START",
        &s.route.start,
    ));
    if s.route_picker == Some(MrtPickTarget::Start) {
        body = body.child(station_picker(s));
    }
    body = body.child(station_field(s, MrtPickTarget::End, "END", &s.route.end));
    if s.route_picker == Some(MrtPickTarget::End) {
        body = body.child(station_picker(s));
    }

    if s.geo_filter().is_some() {
        body = body.child(hint("Station list scoped to the active radius filter."));
    }
    body = body.child(
        row()
            .gap(6.0)
            .child(press("route/swap", "SWAP", Msg::SwapRoute, false))
            .child(press("route/reset", "RESET", Msg::ResetRoute, false)),
    );
    body = body.child(
        row()
            .w_full()
            .padding(4.0)
            .gap(4.0)
            .fill(SURFACE_RAISED)
            .border(1.0, LINE)
            .child(label("MAP CLICK").grow(1.0))
            .child(press(
                "route/target-start",
                "START",
                Msg::SetRouteTarget(MrtPickTarget::Start),
                s.route.target == MrtPickTarget::Start,
            ))
            .child(press(
                "route/target-end",
                "END",
                Msg::SetRouteTarget(MrtPickTarget::End),
                s.route.target == MrtPickTarget::End,
            )),
    );

    match s.route.plan.as_ref() {
        None => body = body.child(hint("Select start and end stations to calculate a route.")),
        Some(plan) => {
            body = body.child(
                row().w_full().padding(4.0).gap(4.0).child(
                    text(format!(
                        "{} min · {} stops · {} transfers",
                        plan.estimated_minutes.round() as i64,
                        plan.stations.len().saturating_sub(1),
                        plan.transfers
                    ))
                    .font(TINY_MONO)
                    .color(INK)
                    .truncate(),
                ),
            );
            if plan.segments.is_empty() {
                body = body.child(hint(format!("You are already at {}.", plan.start)));
            }
            for seg in &plan.segments {
                body = body.child(
                    col()
                        .key(format!("route/seg/{}-{}-{}", seg.line, seg.from, seg.to))
                        .w_full()
                        .padding(4.0)
                        .gap(1.0)
                        .fill(SURFACE_RAISED)
                        .border(1.0, LINE)
                        .child(text(&seg.line).font(MICRO).color(SUCCESS).truncate())
                        .child(
                            text(format!("{} → {}", seg.from, seg.to))
                                .font(TINY)
                                .color(INK)
                                .truncate(),
                        )
                        .child(
                            text(format!("{} stops", seg.stops))
                                .font(MICRO)
                                .color(MUTED),
                        ),
                );
            }
        }
    }
    out.child(scroll_body("mrt", body))
}

fn station_field(s: &AppState, target: MrtPickTarget, label_str: &str, value: &str) -> El {
    let msg = Msg::ToggleRoutePicker(target);
    let open = s.route_picker == Some(target);
    row()
        .key(format!("route/field/{target:?}"))
        .w_full()
        .h(26.0)
        .padding(5.0)
        .gap(6.0)
        .fill(PAPER)
        .border(1.0, if open { LINE_STRONG } else { LINE })
        .center_y()
        .on_press(move |s: &mut AppState| dispatch(s, msg.clone()))
        .child(text(label_str).font(MICRO).color(FAINT).w(34.0))
        .child(
            text(if value.is_empty() {
                "Select station"
            } else {
                value
            })
            .font(TINY_MONO)
            .color(if value.is_empty() { FAINT } else { INK })
            .truncate()
            .grow(1.0),
        )
        .child(text(if open { "▴" } else { "▾" }).font(MICRO).color(MUTED))
}

fn station_picker(s: &AppState) -> El {
    let query = s.station_query.text.to_lowercase();
    let candidates: Vec<&String> = MRT_STATION_NAMES
        .iter()
        .filter(|name| query.is_empty() || name.to_lowercase().contains(&query))
        .take(10)
        .collect();
    let field = text_input(&s.station_query, TINY)
        .key("station/query")
        .single_line()
        .min_w(0.0)
        .grow(1.0)
        .color(INK)
        .placeholder("Station…", FAINT)
        .label("Station")
        .on_edit(|s: &mut AppState| &mut s.station_query);
    let mut out = col()
        .key("station/picker")
        .w_full()
        .max_h(150.0)
        .gap(2.0)
        .fill(PAPER)
        .border(1.0, LINE)
        .padding(5.0);
    out = out.child(row().h(22.0).gap(5.0).child(field));
    if candidates.is_empty() {
        out = out.child(hint("No matching station."));
    }
    for name in candidates {
        let msg = Msg::PickMrtStation(name.clone());
        out = out.child(
            row()
                .key(format!("station/{}", name))
                .h(21.0)
                .padding(4.0)
                .on_press(move |s: &mut AppState| dispatch(s, msg.clone()))
                .child(text(name).font(TINY).color(INK).truncate()),
        );
    }
    out
}

pub fn bus_panel(s: &AppState, _: &mut Cx<AppState>) -> El {
    let snap = s.data.snapshot();
    let scope = s
        .geo_filter()
        .map(|f| argus_core::geo::format_radius(f.radius_m));
    let out = panel(
        "bus",
        "Bus Arrivals",
        format!("{} stops", snap.bus_stops.len()),
    );
    let mut body = col()
        .key("bus/body")
        .w_full()
        .min_h(0.0)
        .gap(5.0)
        .padding(6.0);

    let field = text_input(&s.bus_search, TINY)
        .key("bus/search")
        .single_line()
        .min_w(0.0)
        .grow(1.0)
        .color(INK)
        .placeholder(
            scope.as_deref().unwrap_or("Search stop code or name"),
            FAINT,
        )
        .label("Search bus stops")
        .on_edit(|s: &mut AppState| &mut s.bus_search);
    body = body.child(
        row()
            .h(26.0)
            .w_full()
            .padding(5.0)
            .gap(5.0)
            .fill(PAPER)
            .border(1.0, LINE)
            .child(text("⌕").font(BODY).color(FAINT))
            .child(field),
    );

    let query = s.bus_search.text.trim().to_lowercase();
    if query.is_empty() {
        if let Some(code) = s.selected_stop.as_ref() {
            body = body.child(arrivals(s, code));
        } else {
            body = body.child(hint("Search for a stop, or tap one on the map."));
        }
    } else {
        let matches: Vec<_> = snap
            .bus_stops
            .iter()
            .filter(|stop| {
                stop.bus_stop_code.to_lowercase().contains(&query)
                    || stop.description.to_lowercase().contains(&query)
                    || stop.road_name.to_lowercase().contains(&query)
            })
            .take(6)
            .collect();
        if matches.is_empty() {
            body = body.child(hint("No stops match your search."));
        }
        for stop in matches {
            let msg = Msg::SelectStop(stop.bus_stop_code.clone());
            body = body.child(
                row()
                    .key(format!("bus/stop/{}", stop.bus_stop_code))
                    .w_full()
                    .min_h(0.0)
                    .padding(5.0)
                    .gap(4.0)
                    .fill(SURFACE_RAISED)
                    .border(1.0, LINE)
                    .on_press(move |s: &mut AppState| dispatch(s, msg.clone()))
                    .child(text(&stop.bus_stop_code).font(TINY_MONO).color(BUS).w(48.0))
                    .child(
                        text(&stop.description)
                            .font(TINY)
                            .color(INK)
                            .truncate()
                            .grow(1.0),
                    ),
            );
        }
    }
    out.child(scroll_body("bus", body))
}

fn arrivals(s: &AppState, code: &str) -> El {
    let snap = s.data.snapshot();
    let stop = snap.bus_stops.iter().find(|b| b.bus_stop_code == code);
    let mut out = col().key("bus/arrivals").w_full().min_h(0.0).gap(4.0);
    out = out.child(
        row()
            .w_full()
            .gap(4.0)
            .child(text(code).font(TINY_MONO).color(BUS))
            .child(
                text(stop.map_or("Selected stop", |b| b.description.as_str()))
                    .font(TINY)
                    .color(INK)
                    .truncate(),
            ),
    );
    if snap.bus_arrivals.is_empty() {
        out = out.child(hint(
            "Arrivals feed not configured. Interval: 15s once wired.",
        ));
    }
    for bus in snap.bus_arrivals.iter().take(8) {
        let active = s.bus_route.selected_service.as_deref() == Some(bus.service_no.as_str());
        let msg = Msg::SetBusService(bus.service_no.clone());
        out = out.child(
            row()
                .key(format!("bus/service/{}", bus.service_no))
                .w_full()
                .min_h(0.0)
                .padding(5.0)
                .gap(4.0)
                .fill(if active {
                    SURFACE_HOVER
                } else {
                    SURFACE_RAISED
                })
                .border(1.0, LINE)
                .on_press(move |s: &mut AppState| dispatch(s, msg.clone()))
                .child(
                    text(&bus.service_no)
                        .font(TINY_MONO)
                        .color(if active { BUS } else { INK })
                        .w(44.0),
                )
                .child(
                    text(format_sg_time(&bus.next_bus.estimated_arrival))
                        .font(MICRO)
                        .color(MUTED)
                        .grow(1.0),
                )
                .child(text(next_minutes(bus)).font(TINY_MONO).color(SUCCESS)),
        );
    }
    if s.bus_route.selected_service.is_some() {
        out = out.child(
            row()
                .gap(6.0)
                .child(press(
                    "bus/show-route",
                    "SHOW ROUTE",
                    Msg::ShowBusRoute(true),
                    s.bus_route.shown,
                ))
                .child(press(
                    "bus/clear-route",
                    "CLEAR",
                    Msg::ShowBusRoute(false),
                    !s.bus_route.shown,
                )),
        );
    }
    out
}

fn next_minutes(bus: &argus_core::types::BusArrival) -> String {
    // The original computes minutes from the ISO ETA; with no live feed we
    // show the upstream time string only.
    if bus.next_bus.estimated_arrival.is_empty() {
        "—".into()
    } else {
        format_sg_time(&bus.next_bus.estimated_arrival)
    }
}

/// Keep Cx in the public API of both panels for symmetry with `map::view`.
#[allow(dead_code)]
fn _cx(_: &mut Cx<AppState>) {}
