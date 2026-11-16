//! Right column: intelligence digest, source health, radius filter.

use argus_core::geo::{GEO_FILTER_MAX_RADIUS_M, GEO_FILTER_MIN_RADIUS_M, GEO_FILTER_RADIUS_STEP_M};
use argus_core::signals::SignalTone;
use rune::{Cx, El, col, el, row, text};

use crate::msg::{Msg, signal_color};
use crate::panels::common::*;
use crate::state::AppState;
use crate::theme::*;

pub fn intel_panel(s: &AppState, _: &mut Cx<AppState>) -> El {
    let out = panel("intel", "Intelligence", format!("{} signals", s.signals.len()));
    let mut body = col().key("intel/body").w_full().min_h(0.0).gap(3.0).padding(6.0);
    if s.signals.is_empty() {
        body = body.child(text("NO ACTIVE SIGNALS").font(TINY).color(MUTED));
        body = body.child(hint("Rail, road and weather feeds nominal."));
    } else {
        let mut road = 0;
        for signal in s.signals.iter() {
            if signal.category.as_str() == "road" {
                road += 1;
                if road > 8 { continue; }
            }
            body = body.child(row().key(format!("intel/{}", signal.id)).w_full().min_h(0.0).gap(5.0)
                .child(text(signal.category.as_str().to_uppercase()).font(MICRO)
                    .color(signal_color(signal)).w(44.0))
                .child(text(&signal.text).font(TINY).color(MUTED).truncate().grow(1.0)));
        }
        if s.signals.len() > 8 {
            body = body.child(hint(format!("+{} additional signals", s.signals.len().saturating_sub(8))));
        }
    }
    out.child(scroll_body("intel", body))
}

pub fn source_health(s: &AppState, _: &mut Cx<AppState>) -> El {
    let snap = s.data.snapshot();
    let out = panel("sources", "Source Health",
        format!("{}/{}", s.online_source_count, s.active_source_count));
    let mut body = col().key("sources/body").w_full().min_h(0.0).gap(4.0).padding(6.0);
    for source in snap.sources.iter() {
        let (pct, c) = match &source.status {
            crate::live::SourceStatus::Ok => (1.0, SUCCESS),
            crate::live::SourceStatus::Stale => (0.7, WARNING),
            crate::live::SourceStatus::Loading => (0.5, WARNING),
            crate::live::SourceStatus::Error(_) => (0.0, DANGER),
            crate::live::SourceStatus::Unconfigured => (0.0, FAINT),
            crate::live::SourceStatus::Disabled => (0.0, FAINT),
            crate::live::SourceStatus::Queued => (0.0, MUTED),
        };
        body = body.child(
            col().key(format!("source/{}", source.key as u8)).w_full().min_h(0.0).gap(2.0)
                .child(row().w_full().gap(4.0)
                    .child(text(source.label).font(TINY).color(MUTED).truncate().grow(1.0))
                    .child(text(source.status.label()).font(MICRO).color(c)))
                .child(row().w_full().h(3.0).fill(LINE)
                    .child(el().w_frac(pct).h(3.0).fill(c))),
        );
    }
    out.child(scroll_body("sources", body))
}

pub fn radius_panel(s: &AppState, _: &mut Cx<AppState>) -> El {
    let filter = s.geo_filter();
    let badge = if s.geo_picking {
        "Picking".to_string()
    } else {
        filter.as_ref().map_or_else(|| "Off".to_string(), |f| argus_core::geo::format_radius(f.radius_m))
    };
    let out = panel("geo", "Radius Filter", badge);
    let mut body = col().key("geo/body").w_full().min_h(0.0).gap(6.0).padding(6.0);
    body = body.child(row().gap(6.0)
        .child(press("geo/pick", if filter.is_some() { "MOVE CENTRE" } else { "SET CENTRE" },
            Msg::SetGeoPicking(!s.geo_picking), s.geo_picking))
        .child(press("geo/clear", "CLEAR", Msg::ClearGeo, false)));
    body = body.child(row().w_full().gap(6.0).center_y()
        .child(label("RADIUS").grow(1.0))
        .child(text(argus_core::geo::format_radius(s.geo_radius_m)).font(TINY_MONO).color(INFO)));
    body = body.child(row().w_full().h(4.0).fill(LINE)
        .child(el().w_frac(((s.geo_radius_m - GEO_FILTER_MIN_RADIUS_M)
            / (GEO_FILTER_MAX_RADIUS_M - GEO_FILTER_MIN_RADIUS_M)) as f32).h(4.0).fill(INFO)));
    body = body.child(row().gap(5.0)
        .child(press("geo/minus", "−250m", Msg::SetGeoRadius(s.geo_radius_m - GEO_FILTER_RADIUS_STEP_M), false))
        .child(press("geo/plus", "+250m", Msg::SetGeoRadius(s.geo_radius_m + GEO_FILTER_RADIUS_STEP_M), false)));
    body = body.child(hint(if s.geo_picking {
        "Click anywhere on the map to place the centre."
    } else if filter.is_some() {
        "Only feeds inside the circle are shown. Lines stay whole."
    } else {
        "Set a centre to limit the map and panels to a radius."
    }));
    out.child(body)
}

/// Toasts, fixed at the top right with at most three visible (mirrors
/// `NotificationToasts.tsx`).
pub fn toasts(s: &AppState, _: &mut Cx<AppState>) -> El {
    // Top-right *of the map*, so the right-hand panels stay readable.
    let mut out = col().key("toasts").w(320.0).gap(6.0).absolute(790.0, 110.0);
    for signal in s.toasts.iter().take(3) {
        let color = signal_color(signal);
        let msg = Msg::DismissToast(signal.id.clone());
        out = out.child(
            col().key(format!("toast/{}", signal.id)).w_full().gap(3.0).padding(9.0)
                .fill(SURFACE_RAISED).border(1.0, color)
                .child(text(signal.category.as_str().to_uppercase()).font(MICRO).color(color))
                .child(text(&signal.text).font(TINY).color(MUTED).max_lines(3))
                .child(row().h(20.0).child(press("toast/dismiss", "DISMISS", msg.clone(), false))),
        );
    }
    if s.toasts.len() > 3 {
        out = out.child(row().key("toasts/more").w_full().padding(6.0).fill(SURFACE_RAISED)
            .child(text(format!("+{} more", s.toasts.len() - 3)).font(MICRO).color(MUTED).grow(1.0))
            .child(press("toasts/dismiss-all", "DISMISS ALL", Msg::DismissAllToasts, false)));
    }
    out
}

/// Human title for a signal category (used by the bell + toasts).
#[allow(dead_code)]
pub fn signal_title(tone: SignalTone) -> &'static str {
    match tone {
        SignalTone::Danger => "Critical",
        SignalTone::Warning => "Warning",
        SignalTone::Info => "Update",
        SignalTone::Muted => "Info",
    }
}
