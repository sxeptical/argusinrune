//! ARGUS — Singapore OSINT, native shell (Phase 1).
//!
//! No features are ported yet. This is a dark tactical three-column frame:
//! header bar, layers sidebar, map placeholder (a grid drawn with Rune's
//! `Shape`/`canvas`), and a right column of empty panels.
//!
//! `cargo run -p argus-app` opens the window.
//! `cargo run -p argus-app -- --snap docs/snaps/shell.png` renders it headless.

use argus_core::Layer;
use argus_data::ApiClient;
use rune::tok::type_;
use rune::{AppSpec, Color, Cx, El, LineCap, Shape, canvas, col, el, row, text};

const BG: Color = Color { r: 0.043, g: 0.055, b: 0.070, a: 1.0 };
const HEADER: Color = Color { r: 0.055, g: 0.070, b: 0.087, a: 1.0 };
const PANEL: Color = Color { r: 0.060, g: 0.075, b: 0.094, a: 1.0 };
const PANEL_ALT: Color = Color { r: 0.078, g: 0.096, b: 0.118, a: 1.0 };
const MAP_BG: Color = Color { r: 0.030, g: 0.045, b: 0.055, a: 1.0 };
const GRID: Color = Color { r: 0.12, g: 0.21, b: 0.22, a: 0.55 };
const GRID_STRONG: Color = Color { r: 0.18, g: 0.38, b: 0.36, a: 0.75 };
const BORDER: Color = Color { r: 1.0, g: 1.0, b: 1.0, a: 0.06 };
const TEXT: Color = Color { r: 0.88, g: 0.92, b: 0.93, a: 1.0 };
const TEXT_DIM: Color = Color { r: 0.50, g: 0.58, b: 0.62, a: 1.0 };
const ACCENT: Color = Color { r: 0.20, g: 0.90, b: 0.65, a: 1.0 };
const ACCENT_DIM: Color = Color { r: 0.12, g: 0.50, b: 0.40, a: 1.0 };

struct State {
    layers: Vec<Layer>,
    selected: usize,
}

fn init() -> State {
    // No requests yet — the data crate supplies the seed layer list.
    let client = ApiClient::new("https://api.example.invalid");
    let mut layers = client.known_layers();
    for layer in &mut layers {
        layer.visible = true;
    }
    State { layers, selected: 0 }
}

fn section_label(label: &'static str) -> El {
    text(label).font(type_::SECTION).color(ACCENT_DIM)
}

fn header() -> El {
    let mark = el().key("mark").w(22.0).h(22.0).radius(5.0).fill(ACCENT)
        .center().child(text("A").font(type_::TITLE).color(BG));
    let status = row().key("status").gap(7.0).center()
        .child(el().w(7.0).h(7.0).radius(4.0).fill(ACCENT))
        .child(text("SYSTEM ONLINE").font(type_::MONO_CAPTION).color(ACCENT));
    row().key("header").h(56.0).w_full().padding(16.0).gap(12.0)
        .fill(HEADER).border(1.0, BORDER).center()
        .child(mark)
        .child(text("ARGUS — Singapore OSINT").font(type_::TITLE).color(TEXT))
        .child(el().grow(1.0))
        .child(status)
}

fn layers_sidebar(s: &State) -> El {
    let mut out = col().key("layers").w(248.0).min_h(0.0).fill(PANEL)
        .padding(16.0).gap(8.0).border(1.0, BORDER)
        .child(section_label("LAYERS"));
    for (i, layer) in s.layers.iter().enumerate() {
        let selected = i == s.selected;
        let dot = el().w(8.0).h(8.0).radius(4.0)
            .fill(if layer.visible { ACCENT } else { TEXT_DIM });
        out = out.child(
            row().key(format!("layer/{}", layer.id)).gap(10.0).center()
                .padding(8.0).radius(6.0)
                .fill(if selected { PANEL_ALT } else { Color { a: 0.0, ..PANEL } })
                .child(dot)
                .child(text(layer.label.as_str()).font(type_::BODY)
                    .color(if selected { TEXT } else { TEXT_DIM })),
        );
    }
    out.child(el().grow(1.0))
        .child(text("PHASE 1 — SHELL ONLY").font(type_::MONO_CAPTION).color(TEXT_DIM))
}

/// A lat/long-like grid plus a crosshair and a few site markers, drawn in a
/// fixed view box that Rune stretches to the canvas element's size.
fn map_grid() -> Shape {
    const W: f32 = 1000.0;
    const H: f32 = 800.0;
    let mut s = Shape::new();
    for i in 1..10 {
        let x = i as f32 * W / 10.0;
        s = s.line([x, 0.0], [x, H], 0.7, LineCap::Butt, GRID);
    }
    for i in 1..8 {
        let y = i as f32 * H / 8.0;
        s = s.line([0.0, y], [W, y], 0.7, LineCap::Butt, GRID);
    }
    s = s.line([W / 2.0, 0.0], [W / 2.0, H], 1.0, LineCap::Butt, GRID_STRONG)
        .line([0.0, H / 2.0], [W, H / 2.0], 1.0, LineCap::Butt, GRID_STRONG);
    for (x, y) in [(220.0, 300.0), (640.0, 250.0), (760.0, 560.0)] {
        s = s.line([x - 9.0, y], [x + 9.0, y], 1.5, LineCap::Round, ACCENT_DIM)
            .line([x, y - 9.0], [x, y + 9.0], 1.5, LineCap::Round, ACCENT_DIM);
    }
    s.view_box(W, H)
}

fn map_pane() -> El {
    col().key("map").grow(1.0).min_w(0.0).min_h(0.0)
        .child(
            row().key("map-toolbar").h(40.0).w_full().fill(PANEL)
                .padding(12.0).gap(12.0).border(1.0, BORDER).center()
                .child(text("MAP // SINGAPORE").font(type_::LABEL).color(TEXT))
                .child(el().grow(1.0))
                .child(text("1.2913 N, 103.8520 E").font(type_::MONO_CAPTION).color(TEXT_DIM)),
        )
        .child(
            canvas(map_grid()).key("map-canvas").grow(1.0).min_h(0.0).w_full().fill(MAP_BG),
        )
}

fn panels_column() -> El {
    let mut out = col().key("panels").w(320.0).min_h(0.0).fill(PANEL)
        .padding(16.0).gap(12.0).border(1.0, BORDER)
        .child(section_label("SITUATION"));
    for (key, title) in [("feed", "LIVE FEED"), ("signals", "SIGNALS"), ("sources", "SOURCES")] {
        out = out.child(
            col().key(key).w_full().gap(8.0).padding(12.0).radius(8.0).fill(PANEL_ALT)
                .child(text(title).font(type_::LABEL).color(ACCENT))
                .child(text("No data yet — Phase 1 shell").font(type_::BODY).color(TEXT_DIM)),
        );
    }
    out.child(el().grow(1.0))
        .child(text("ARGUS v0.1 · RUNE").font(type_::MONO_CAPTION).color(TEXT_DIM))
}

fn view(s: &State, _: &mut Cx<State>) -> El {
    col().key("root").w_full().h_full().fill(BG)
        .child(header())
        .child(
            row().key("body").grow(1.0).min_h(0.0).w_full()
                .child(layers_sidebar(s))
                .child(map_pane())
                .child(panels_column()),
        )
}

fn main() {
    rune::run(AppSpec::<State> {
        scenes: &[("shell", |_| {})],
        ..AppSpec::new("ARGUS", (1440.0, 900.0), init, view)
    });
}
