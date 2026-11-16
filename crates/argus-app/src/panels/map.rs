//! Centre map pane: slippy map, search trigger, legend, geo-pick hint.

use argus_core::layers::{LayerLegendItem, layer_by_key};
use argus_map::map_view_for;
use rune::{Cx, El, col, el, row, text};

use crate::msg::{Msg, dispatch};
use crate::panels::common::{dot, press};
use crate::state::AppState;
use crate::theme::{self, *};

pub fn view(s: &AppState, cx: &mut Cx<AppState>) -> El {
    let c = &s.map.camera;
    let coords = format!("{:.4} N, {:.4} E", c.center.lat, c.center.lon);
    let mut toolbar = row().key("map/toolbar").h(34.0).w_full().padding(9.0).gap(10.0)
        .fill(SURFACE).border(1.0, LINE).center_y()
        .child(text("LIVE MAP").font(LABEL).color(INK))
        .child(text("SINGAPORE").font(MICRO).color(MUTED))
        .child(el().grow(1.0));
    if let Some(filter) = s.geo_filter() {
        toolbar = toolbar.child(text(format!("{} RADIUS", argus_core::geo::format_radius(filter.radius_m)))
            .font(MICRO_MONO).color(INFO));
    }
    toolbar = toolbar.child(text(coords).font(MICRO_MONO).color(MUTED));

    let map_el = map_view_for(&s.map, &s.overlays, cx, |a: &mut AppState| &mut a.map)
        .key("map/canvas").grow(1.0).min_h(0.0).w_full()
        .on_layout(|s: &mut AppState, rect: [f32; 4]| {
            if rect[2] > 1.0 && rect[3] > 1.0 {
                s.map.set_viewport((rect[2], rect[3]));
            }
        })
        .child(search_button())
        .child(map_hints(s));

    col().key("map-pane").grow(1.0).min_w(0.0).min_h(0.0).gap(4.0)
        .child(toolbar)
        .child(col().key("map-frame").grow(1.0).min_h(0.0).fill(MAP_BG).border(1.0, LINE)
            .child(map_el))
        .child(legend(s))
        .child(row().key("map/attribution").h(14.0).w_full().center_y()
            .child(text(s.map.attribution()).font(MICRO).color(FAINT).truncate()))
}

fn search_button() -> El {
    row().key("map/search-trigger").h(32.0).w(32.0).absolute(10.0, 10.0).center()
        .fill(OVERLAY).border(1.0, LINE)
        .on_press(|s: &mut AppState| dispatch(s, Msg::OpenSearch(true)))
        .child(text("⌕").font(BODY).color(INK))
}

fn map_hints(s: &AppState) -> El {
    let mut hints = col().key("map/hints").w_full().padding(8.0).gap(6.0).absolute(0.0, 0.0);
    if s.geo_filter().is_some() && !s.geo_picking {
        hints = hints.child(row().key("map/radius-chip").h(24.0).padding(8.0).gap(8.0).center_y()
            .fill(OVERLAY).border(1.0, INFO)
            .child(text(format!("{} RADIUS", argus_core::geo::format_radius(s.geo_radius_m)))
                .font(MICRO).color(INFO)));
    }
    if s.geo_picking {
        hints = hints.child(row().key("map/pick-hint").h(26.0).padding(8.0).gap(8.0).center_y()
            .fill(OVERLAY).border(1.0, INFO)
            .child(text("CLICK MAP TO SET CENTRE").font(MICRO).color(INFO))
            .child(press("pick/cancel", "ESC", Msg::SetGeoPicking(false), false)));
    }
    if let Some(label) = s.hovered_label.as_deref() {
        hints = hints.child(row().key("map/hover").h(22.0).padding(8.0).fill(OVERLAY).border(1.0, LINE)
            .child(text(label).font(MICRO).color(INK).truncate()));
    }
    hints
}

fn legend(s: &AppState) -> El {
    let mut out = row().key("map/legend").h(22.0).w_full().padding(6.0).gap(12.0)
        .fill(OVERLAY).border(1.0, LINE);
    let mut any = false;
    for layer in argus_core::layers::LAYERS.iter() {
        if !s.visibility.get(&layer.key).copied().unwrap_or(false) {
            continue;
        }
        for item in &layer.legend {
            any = true;
            let (color, name) = match item {
                LayerLegendItem::Dot { tone, label } => (theme::tone_color(tone), label.as_str()),
                LayerLegendItem::Swatch { tone, label, .. } => (theme::tone_color(tone), label.as_str()),
            };
            out = out.child(row().gap(4.0).center_y().child(dot(color)).child(text(name).font(MICRO).color(MUTED)));
        }
    }
    if !any {
        out = out.child(text("NO LAYERS ENABLED").font(MICRO).color(FAINT));
    }
    out
}

/// The layer-group label for a layer key (used by the search scope label).
#[allow(dead_code)]
pub fn group_label(key: &str) -> &'static str {
    layer_by_key(key).map_or("", |l| match l.group {
        argus_core::layers::LayerGroupId::Transport => "Transport",
        argus_core::layers::LayerGroupId::Ground => "Roads & Parking",
        argus_core::layers::LayerGroupId::Amenities => "Amenities",
    })
}
