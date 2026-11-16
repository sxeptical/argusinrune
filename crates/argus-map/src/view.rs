//! The Rune view: draw tiles + overlays and wire input handlers.
//!
//! Two entry points:
//!
//! * [`map_view`] — for an app whose Rune state type **is** [`MapState`].
//! * [`map_view_for`] — for an app that embeds `MapState` in its own state;
//!   pass a `fn(&mut S) -> &mut MapState` lens.
//!
//! Both return a full-size element; the caller sizes it (`.w_full().h_full()`
//! or `.grow(1.0)` inside a bounded ancestor). The `viewport` stored in state
//! must match that element's size in points — Rune builds views before layout.

use std::collections::HashSet;

use rune::tok::type_;
use rune::{Color, Cx, El, LineCap, Shape, canvas, el, text};

use crate::camera::MapCamera;
use crate::overlay::{DrawnPoint, Icon, Overlays, radius_circle_ring};
use crate::projection::{TILE_SIZE, TileCoord};
use crate::state::MapState;

const MAP_BG: Color = Color {
    r: 0.035,
    g: 0.048,
    b: 0.058,
    a: 1.0,
};
const LABEL_COLOR: Color = Color {
    r: 0.86,
    g: 0.92,
    b: 0.94,
    a: 1.0,
};
const LABEL_BG: Color = Color {
    r: 0.02,
    g: 0.03,
    b: 0.04,
    a: 0.72,
};
const SELECT_COLOR: Color = Color {
    r: 1.0,
    g: 0.85,
    b: 0.30,
    a: 1.0,
};

/// For apps whose state is exactly `MapState`.
pub fn map_view(state: &MapState, overlays: &Overlays, cx: &mut Cx<MapState>) -> El {
    map_view_for(state, overlays, cx, |s| s)
}

/// Generic form. `lens` reaches the `MapState` inside the app state `S`, so
/// handlers downcast correctly to the app's Rune state type.
pub fn map_view_for<S: 'static>(
    state: &MapState,
    overlays: &Overlays,
    _cx: &mut Cx<S>,
    lens: fn(&mut S) -> &mut MapState,
) -> El {
    let (vw, vh) = state.viewport;
    let mut root = el()
        .key("argus-map")
        .w_full()
        .h_full()
        .fill(MAP_BG)
        .clip_rounded(0.0)
        .on_drag(move |s: &mut S, d, tx| {
            let m = lens(s);
            match d.phase {
                rune::Phase::Down => {
                    m.pointer_down(d.x, d.y, d.clicks);
                    if m.geo_filter_picking || d.clicks >= 2 {
                        tx.instant();
                    }
                }
                rune::Phase::Move => {
                    m.pointer_drag(d.x, d.y);
                    tx.live_only(); // pan is a camera change, no layout needed
                }
                rune::Phase::Up => {
                    m.pointer_up(d.x, d.y, d.cancelled(), d.clicks);
                    tx.instant();
                }
            }
        })
        .on_wheel(move |s: &mut S, ev, tx| {
            let m = lens(s);
            m.wheel(ev.x, ev.y, ev.dy);
            tx.instant();
            true // consume: the map owns the wheel
        })
        .on_magnify(move |s: &mut S, ev, tx| {
            let m = lens(s);
            m.magnify(ev.x, ev.y, ev.delta);
            tx.instant();
            true
        })
        .on_pointer_move_local(move |s: &mut S, ev, tx| {
            let m = lens(s);
            if m.hover(ev.x, ev.y) {
                tx.instant(); // only re-run the view when the hit target changes
            }
        });

    // Tiles: fallback ancestors first, exact tiles on top.
    root = root.child(tile_layer(state));

    // Overlay geometry (polygons, lines, circles, points) in one Shape.
    let shape = overlay_shape(state, overlays);
    root = root.child(
        canvas(shape)
            .key("argus-map/overlays")
            .absolute(0.0, 0.0)
            .size(vw, vh),
    );

    // Cluster count labels (text cannot live in a Shape).
    let drawn = overlays.drawn_points(&state.camera, state.cluster);
    for (label_i, dp) in drawn.iter().filter(|d| d.count > 1).enumerate() {
        root = root.child(
            text(format!("{}", dp.count))
                .key(format!("argus-map/cluster/{label_i}"))
                .font(type_::MONO_CAPTION)
                .color(LABEL_COLOR)
                .absolute(dp.screen.0 - 8.0, dp.screen.1 - 7.0)
                .padding(3.0)
                .radius(6.0)
                .fill(LABEL_BG),
        );
    }

    // Attribution (CARTO / OSM terms) bottom-right.
    root.child(
        text(state.attribution())
            .key("argus-map/attribution")
            .font(type_::MONO_CAPTION)
            .color(LABEL_COLOR)
            .absolute((vw - 400.0).max(4.0), (vh - 20.0).max(4.0))
            .padding(3.0)
            .radius(4.0)
            .fill(LABEL_BG),
    )
}

/// Compute the screen rect a tile (or ancestor) occupies at the camera zoom.
fn tile_rect(camera: &MapCamera, t: &TileCoord) -> (f32, f32, f32) {
    let scale = TILE_SIZE * 2f64.powf(camera.zoom - t.z as f64);
    let wx = t.x as f64 * scale;
    let wy = t.y as f64 * scale;
    let (sx, sy) = camera.world_to_screen(wx, wy);
    (sx, sy, scale as f32)
}

/// Build the tile element tree. A missing child tile is covered by drawing its
/// nearest cached ancestor at the ancestor's full rect (geographically aligned);
/// exact tiles are painted on top. This needs no image cropping in Rune.
fn tile_layer(state: &MapState) -> El {
    let out = el()
        .key("argus-map/tiles")
        .w_full()
        .h_full()
        .clip_rounded(0.0);
    draw_tiles(out, state)
}

/// Internal: consult the tile cache and append tile Els.
fn draw_tiles(mut out: El, state: &MapState) -> El {
    let camera = &state.camera;
    let cov =
        crate::projection::tile_coverage(camera.world_center(), camera.zoom, state.viewport, 1);
    // Immutable snapshots: `TileStore::ready_image` / `ready_ancestor` do not
    // disturb the LRU order while the view reads them.
    let store = state.tile_store();
    let mut exact: Vec<(TileCoord, rune::Image)> = Vec::new();
    let mut fallback: Vec<(TileCoord, rune::Image)> = Vec::new();
    let mut covered: HashSet<TileCoord> = HashSet::new();

    for t in &cov.tiles {
        if let Some(img) = store.ready_image(t)
            && img.is_ready()
        {
            exact.push((*t, img.clone()));
            continue;
        }
        if let Some((anc, img)) = store.ready_ancestor(t)
            && img.is_ready()
            && covered.insert(anc)
        {
            fallback.push((anc, img.clone()));
        }
    }

    for (t, img) in fallback {
        let (x, y, size) = tile_rect(camera, &t);
        out = out.child(
            el().key(format!("argus-map/tile-fallback/{}/{}/{}", t.z, t.x, t.y))
                .absolute(x, y)
                .size(size, size)
                .pixels(img.pixels()),
        );
    }
    for (t, img) in exact {
        let (x, y, size) = tile_rect(camera, &t);
        out = out.child(
            el().key(format!("argus-map/tile/{}/{}/{}", t.z, t.x, t.y))
                .absolute(x, y)
                .size(size, size)
                .pixels(img.pixels()),
        );
    }
    out
}

/// Build one Shape containing polygons, lines, the radius circle and points.
fn overlay_shape(state: &MapState, overlays: &Overlays) -> Shape {
    let camera = &state.camera;
    let mut s = Shape::new();

    for poly in &overlays.polygons {
        let pts: Vec<[f32; 2]> = poly
            .points
            .iter()
            .map(|p| camera.lnglat_to_screen(*p).into())
            .collect();
        s = s.fill(pts.clone(), poly.fill.to_rune());
        if let Some((color, width)) = poly.stroke {
            s = s.stroke_with(pts, true, width, LineCap::Round, color.to_rune());
        }
    }

    for line in &overlays.lines {
        let pts: Vec<[f32; 2]> = line
            .points
            .iter()
            .map(|p| camera.lnglat_to_screen(*p).into())
            .collect();
        s = s.stroke(pts, line.width, line.color.to_rune());
    }

    for circle in &overlays.circles {
        let ring = radius_circle_ring(circle.center, circle.radius_m, 72);
        let pts: Vec<[f32; 2]> = ring
            .iter()
            .map(|p| camera.lnglat_to_screen(*p).into())
            .collect();
        s = s.fill(pts.clone(), circle.fill.to_rune());
        s = s.stroke_with(pts, true, 1.5, LineCap::Round, circle.stroke.to_rune());
    }

    let drawn: Vec<DrawnPoint> = overlays.drawn_points(camera, state.cluster);
    for dp in &drawn {
        let is_selected = dp.ids.iter().any(|id| state.selected == Some(*id));
        let is_hovered = dp.ids.iter().any(|id| state.hovered == Some(*id));
        s = push_point(s, dp, is_selected, is_hovered);
    }
    s.view_box(state.viewport.0, state.viewport.1)
}

fn push_point(mut s: Shape, dp: &DrawnPoint, selected: bool, hovered: bool) -> Shape {
    let (x, y) = dp.screen;
    let r = dp.style.radius;
    let fill = if selected {
        SELECT_COLOR
    } else {
        dp.style.color.to_rune()
    };
    match dp.style.icon {
        Icon::Circle => {
            let ring: Vec<[f32; 2]> = circle_pts(x, y, r, 16);
            s = s.fill(ring.clone(), fill);
            if let Some(stroke) = dp.style.stroke {
                s = s.stroke_with(ring, true, 1.0, LineCap::Round, stroke.to_rune());
            }
        }
        Icon::Square => {
            let pts = vec![
                [x - r, y - r],
                [x + r, y - r],
                [x + r, y + r],
                [x - r, y + r],
            ];
            s = s.fill(pts, fill);
        }
        Icon::Diamond => {
            let pts = vec![[x, y - r], [x + r, y], [x, y + r], [x - r, y]];
            s = s.fill(pts, fill);
        }
        Icon::Cross => {
            s = s.line([x - r, y], [x + r, y], 2.0, LineCap::Round, fill);
            s = s.line([x, y - r], [x, y + r], 2.0, LineCap::Round, fill);
        }
        Icon::Pin => {
            let ring = circle_pts(x, y - r * 0.3, r, 16);
            s = s.fill(ring, fill);
            s = s.line([x, y], [x, y + r * 1.4], 2.0, LineCap::Round, fill);
        }
    }
    if selected || hovered {
        s = s.stroke_with(
            circle_pts(x, y, r + 3.0, 20),
            true,
            1.5,
            LineCap::Round,
            SELECT_COLOR,
        );
    }
    s
}

fn circle_pts(x: f32, y: f32, r: f32, segments: usize) -> Vec<[f32; 2]> {
    (0..segments)
        .map(|i| {
            let a = 2.0 * std::f64::consts::PI * i as f64 / segments as f64;
            [x + r * a.cos() as f32, y + r * a.sin() as f32]
        })
        .collect()
}
