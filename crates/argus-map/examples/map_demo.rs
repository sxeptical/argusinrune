//! Argus map demo: Singapore raster basemap + sample overlays.
//!
//! ```sh
//! cargo run -p argus-map --example map_demo -- --snap docs/snaps/map.png
//! cargo run -p argus-map --example map_demo -- --list-scenes
//! ```
//!
//! Tiles are preloaded synchronously in `setup` so the headless snap shows the
//! real CARTO basemap rather than a placeholder (see `MapState::preload_sync`).

use argus_map::{
    DEFAULT_CENTER, DEFAULT_ZOOM, Icon, LngLat, MapPoint, MapState, Overlays, PointStyle, Polygon,
    Polyline, RadiusCircle, Rgba, map_view_for,
};
use rune::{AppSpec, Cx, El};

struct Demo {
    map: MapState,
    overlays: Overlays,
}

fn samples() -> Overlays {
    let marker = |id, lon, lat, color: Rgba| {
        MapPoint::new(
            id,
            LngLat::new(lon, lat),
            PointStyle {
                radius: 7.0,
                color,
                stroke: Some(Rgba::new(0.04, 0.05, 0.06, 1.0)),
                icon: Icon::Circle,
                label: None,
            },
        )
    };
    let green = Rgba::new(0.20, 0.90, 0.65, 1.0);
    let cyan = Rgba::new(0.31, 0.72, 0.86, 1.0);
    let orange = Rgba::new(0.89, 0.70, 0.25, 1.0);
    let violet = Rgba::new(0.54, 0.61, 0.96, 1.0);

    let mut ov = Overlays::new();
    ov = ov.with_point(marker(1, 103.9940, 1.3644, cyan)); // Changi
    ov = ov.with_point(marker(2, 103.8300, 1.2494, orange)); // Sentosa
    ov = ov.with_point(marker(3, 103.8198, 1.4369, green)); // Woodlands
    ov = ov.with_point(marker(4, 103.8651, 1.3144, violet)); // Botanic Gardens

    // An MRT-like north-south line.
    ov = ov.with_line(Polyline {
        id: 100,
        points: vec![
            LngLat::new(103.8198, 1.4369),
            LngLat::new(103.8280, 1.3700),
            LngLat::new(103.8320, 1.3040),
            LngLat::new(103.8600, 1.2820),
        ],
        color: Rgba::new(0.94, 0.35, 0.40, 0.95),
        width: 3.0,
    });

    // A park polygon near the Botanic Gardens.
    ov = ov.with_polygon(Polygon {
        id: 200,
        points: vec![
            LngLat::new(103.860, 1.310),
            LngLat::new(103.872, 1.310),
            LngLat::new(103.872, 1.320),
            LngLat::new(103.860, 1.320),
        ],
        fill: Rgba::new(0.45, 0.85, 0.60, 0.22),
        stroke: Some((Rgba::new(0.45, 0.85, 0.60, 0.7), 1.0)),
    });

    // Geo-filter radius circle centred on the default camera.
    ov = ov.with_circle(RadiusCircle {
        id: 300,
        center: DEFAULT_CENTER,
        radius_m: 3000.0,
        stroke: Rgba::new(0.45, 0.73, 0.85, 0.9),
        fill: Rgba::new(0.45, 0.73, 0.85, 0.08),
    });
    ov
}

fn init() -> Demo {
    let viewport = (1000.0, 680.0);
    let mut map = MapState::new(DEFAULT_CENTER, DEFAULT_ZOOM, viewport);
    // Keep the four sample markers distinct at the default zoom.
    map.cluster.max_cluster_zoom = 0.0;
    let overlays = samples();
    map.set_overlays(&overlays);
    Demo { map, overlays }
}

fn view(s: &Demo, cx: &mut Cx<Demo>) -> El {
    map_view_for(&s.map, &s.overlays, cx, |d| &mut d.map)
}

fn setup(s: &mut Demo, waker: rune::Waker) {
    s.map.setup(waker);
    // Headless snaps render one frame; prime tiles now so they are present.
    s.map.preload_sync();
}

fn poll(s: &mut Demo, _t: f64, _tx: &mut rune::Tx) -> bool {
    let changed = s.map.poll();
    for msg in s.map.drain_messages() {
        eprintln!("map event: {msg:?}");
    }
    changed
}

fn main() {
    rune::run(AppSpec::<Demo> {
        scenes: &[("singapore", |_| {})],
        setup: Some(setup),
        poll: Some(poll),
        ..AppSpec::new("ARGUS Map", (1000.0, 680.0), init, view)
    });
}
