//! Dashboard header, source counter, status dot and notification bell.

use rune::{Color, Cx, El, col, el, row, text};

use crate::msg::{Msg, dispatch};
use crate::panels::common::{dot, key_value, panel, press, rule};
use crate::state::AppState;
use crate::theme::*;

fn chip(label: &str, value: &str, color: Color) -> El {
    row().gap(5.0).center_y()
        .child(text(label).font(MICRO).color(FAINT).truncate())
        .child(text(value).font(TINY_MONO).color(color).truncate())
}

pub fn view(s: &AppState, _: &mut Cx<AppState>) -> El {
    let online = s.online_source_count;
    let active = s.active_source_count;
    let status = if online == 0 && active > 0 {
        "NO SIGNAL"
    } else if s.toasts.iter().any(|t| t.tone == argus_core::signals::SignalTone::Danger) {
        "DEGRADED"
    } else {
        "LIVE"
    };
    let status_color = if status == "LIVE" { SUCCESS } else { WARNING };

    let mut bell = row().key("bell/button").h(28.0).w(30.0).center()
        .fill(if s.bell_open { SURFACE_HOVER } else { Color { a: 0.0, ..SURFACE } })
        .border(1.0, LINE)
        .on_press(|s: &mut AppState| dispatch(s, Msg::ToggleBell))
        .child(text("◔").font(BODY).color(if s.bell_open { INK } else { MUTED }));
    if !s.toasts.is_empty() {
        bell = bell.child(
            el().key("bell/count").w(14.0).h(14.0).radius(7.0).fill(DANGER).center()
                .child(text(s.toasts.len().to_string()).font(MICRO_MONO).color(PAPER)),
        );
    }

    let mut header = row().key("header").h(56.0).w_full().padding(14.0).gap(16.0).center_y()
        .fill(SURFACE).border(1.0, LINE)
        .child(
            col().key("brand").gap(2.0).min_w(200.0)
                .child(text("ARGUS").font(TITLE).color(INK))
                .child(text("SINGAPORE SIGNAL MONITOR").font(MICRO).color(MUTED)),
        )
        .child(el().grow(1.0))
        .child(chip("", &s.clock_date, MUTED))
        .child(chip("", &s.clock_time, INK))
        .child(chip("SOURCES", &format!("{online}/{active}"), INK))
        .child(row().gap(6.0).center_y().child(dot(status_color)).child(text(status).font(MICRO).color(status_color)))
        .child(bell);

    if s.bell_open {
        header = header.child(notification_popup(s));
    }
    header
}

fn notification_popup(s: &AppState) -> El {
    let mut body = col().key("bell/popup").w(330.0).max_h(420.0).gap(4.0)
        .fill(SURFACE).border(1.0, LINE_STRONG).padding(8.0)
        .absolute(-330.0, 34.0);
    body = body.child(text("NOTIFICATIONS").font(LABEL).color(INK));
    if s.toasts.is_empty() {
        body = body.child(text("No new alerts since you opened ARGUS.").font(TINY).color(MUTED).max_lines(2));
    } else {
        for signal in &s.toasts {
            body = body.child(
                col().key(format!("bell/alert/{}", signal.id)).w_full().gap(2.0)
                    .child(text(signal.category.as_str().to_uppercase()).font(MICRO).color(DANGER))
                    .child(text(&signal.text).font(TINY).color(MUTED).max_lines(3)),
            );
        }
    }
    body = body.child(rule())
        .child(text("ALERT CATEGORIES").font(MICRO).color(MUTED));
    let prefs = &s.notification_prefs;
    body = body.child(key_value("bell/rail", "Rail", if prefs.rail { "ON" } else { "OFF" }, if prefs.rail { SUCCESS } else { MUTED }));
    body = body.child(key_value("bell/road", "Road", if prefs.road { "ON" } else { "OFF" }, if prefs.road { SUCCESS } else { MUTED }));
    body = body.child(key_value("bell/weather", "Air quality", if prefs.weather { "ON" } else { "OFF" }, if prefs.weather { SUCCESS } else { MUTED }));
    body.child(row().gap(6.0).child(
        press("bell/rail-toggle", "Toggle rail", Msg::ToggleNotificationRail, prefs.rail),
    ).child(
        press("bell/road-toggle", "Toggle road", Msg::ToggleNotificationRoad, prefs.road),
    ))
}

/// Header chip used by the loading screen as well.
#[allow(dead_code)]
pub fn chip_row(label: &str, value: &str) -> El {
    row().gap(6.0).child(text(label).font(MICRO).color(FAINT)).child(text(value).font(TINY_MONO).color(INK))
}

/// Thin status panel used at the top of the right column.
#[allow(dead_code)]
pub fn status_panel(s: &AppState) -> El {
    let mut out = panel("status", "System", format!("{} sources", s.active_source_count));
    out = out.child(row().w_full().padding(8.0).gap(10.0).center_y()
        .child(dot(if s.online_source_count > 0 { SUCCESS } else { WARNING }))
        .child(text(format!("{} of {} sources online", s.online_source_count, s.active_source_count)).font(TINY).color(MUTED)));
    out
}
