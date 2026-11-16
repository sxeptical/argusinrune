//! Centre map pane: slippy map, search trigger, legend, geo-pick hint.

use argus_core::layers::{LayerLegendItem, layer_by_key};
use argus_map::map_view_for;
use rune::{Cx, El, SurfaceFit, col, el, row, text};

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

    let mut map_el = map_view_for(&s.map, &s.overlays, cx, |a: &mut AppState| &mut a.map)
        .key("map/canvas").grow(1.0).min_h(0.0).w_full()
        .on_layout(|s: &mut AppState, rect: [f32; 4]| {
            if rect[2] > 1.0 && rect[3] > 1.0 {
                s.map.set_viewport((rect[2], rect[3]));
            }
        })
        .child(search_button())
        .child(map_hints(s));
    if let Some(popup) = hover_popup(s) {
        map_el = map_el.child(popup);
    }

    col().key("map-pane").grow(1.0).min_w(0.0).min_h(0.0).gap(4.0)
        .child(toolbar)
        .child(col().key("map-frame").grow(1.0).min_h(0.0).fill(MAP_BG).border(1.0, LINE)
            .child(map_el))
        .child(legend(s))
        .child(row().key("map/attribution").h(14.0).w_full().center_y()
            .child(text(s.map.attribution()).font(MICRO).color(FAINT).truncate()))
}

/// Compact hover tooltip for the overlay under the cursor. Content comes from
/// the argus-core popups/parks/mobility builders, matching the original
/// `Map.tsx` `showHoverPopup`; argus-map supplies the hovered overlay id and
/// pointer position.
fn hover_popup(s: &AppState) -> Option<El> {
    let id = s.map.hovered?;
    let target = s.overlay_targets.get(&id)?;
    let content = crate::hover::content_for(s, target)?;
    let (px, py) = s.map.pointer();
    let (vw, vh) = s.map.viewport;
    let w = 264.0;
    let h = (32.0 + content.lines.len() as f32 * 15.0
        + if content.photos.is_empty() { 0.0 } else { 42.0 }).clamp(48.0, 190.0);
    let x = (px + 14.0).min((vw - w - 6.0).max(4.0)).max(4.0);
    let y = (py - h * 0.5).min((vh - h - 6.0).max(4.0)).max(4.0);

    let mut card = col().key("map/hover-popup").w(w).h(h).min_h(0.0).absolute(x, y)
        .fill(OVERLAY).border(1.0, LINE_STRONG).radius(4.0).padding(8.0).gap(3.0)
        .child(text(&content.title).font(MICRO).color(content.title_color).truncate());

    if !content.photos.is_empty() {
        let mut strip = row().key("map/hover-popup/photos").h(34.0).gap(4.0);
        for (i, url) in content.photos.iter().take(3).enumerate() {
            let mut thumb = el().key(format!("map/hover-popup/photo/{i}")).w(74.0).h(34.0).fill(PAPER);
            if let Some(image) = s.images.image(url) {
                thumb = thumb.pixels_fit(image.pixels(), SurfaceFit::Fill)
                    .label(format!("{} photo {}", content.title, i + 1));
            }
            strip = strip.child(thumb);
        }
        card = card.child(strip);
    }

    for line in &content.lines {
        let color = match line.kind.as_str() {
            "hint" => FAINT,
            "message" | "name" => INK,
            _ => MUTED,
        };
        card = card.child(text(&line.text).font(TINY).color(color).max_lines(2));
    }
    Some(card)
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
