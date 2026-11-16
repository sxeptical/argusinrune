# Argus — Rust / Rune rewrite

Phase 1 scaffold only. The Next.js/TypeScript tree stays in place until features are ported.
Target: native desktop app in [Rune](https://github.com/anomalyco/rune), with external APIs
called directly from Rust (`reqwest`), keys in local config, a custom map renderer, and no
Workers / rate limiting / CSP.

## Workspace layout

```
Cargo.toml              # workspace + Rune git rev + the five [patch.crates-io] entries
rust-toolchain.toml     # 1.99.1 (the toolchain Rune pins); edition 2024
crates/
  argus-core/           # domain types + logic, no UI and no I/O deps (serde only)
  argus-data/           # reqwest/tokio clients, local config keys (skeleton)
  argus-app/            # bin `argus`: the Rune window (AppSpec::new("ARGUS", ...))
docs/snaps/shell.png    # checked-in headless render of the shell (2x = 2880x1800)
```

Dependency direction: `argus-app -> {argus-core, argus-data, rune}`, `argus-data -> argus-core`.
`argus-core` must never import `rune`.

## Toolchain and pinned Rune rev

- Toolchain: **Rust 1.99.1** (`rust-toolchain.toml`, matching Rune's own pin). Install with
  `rustup toolchain install 1.99.1 --profile minimal --component clippy,rustfmt` if missing.
  Edition 2024, Cargo resolver 3.
- Rune **dev rev pinned**: `55ef5268f22038a95344b3e9d2b4a25ddba731b1`
  (commit date 2026-11-16, "Tx::scroll_key_to lands row/mark in one request..."). Use this
  same `rev` for `rune` and **every** `[patch.crates-io]` entry:
  `winit`, `parley_engine`, `wgpu-hal`, `accesskit_windows`, `accesskit_macos`.

  **Gotcha:** Cargo does not inherit `[patch]` from a dependency. If you update Rune, update all
  six revs together, or the build silently uses upstream `winit`/`wgpu-hal` and e.g. VoiceOver
  stops recognising Rune headings. See `crates/rune/README.md` → "Depending on Rune".
- On macOS the default Rune feature `metal-backend` is used (no wgpu software driver needed).
  We also enable Rune's `tooling` feature so **`--snap`/`--tree` work in release builds too**;
  debug builds always have headless tooling regardless of features.

## Build / run / verify

```sh
cargo build                                      # debug; ~1.5 min cold
cargo test --workspace                           # 2 unit tests + doctests, all green
cargo run -p argus-app                           # opens the window
cargo run -p argus-app -- --snap docs/snaps/shell.png
cargo run -p argus-app -- --list-scenes          # prints: shell
cargo run -p argus-app -- --tree out/tree.json   # element tree (key, role, label, rects, ...)
```

Phase 1 verification (2026-11-16):

- `cargo build` → `Finished dev profile ... in 1m 36s`, no errors.
- `cargo test --workspace` → `argus-core` 1 passed, `argus-data` 1 passed, 0 failed.
- `cargo run -p argus-app -- --snap docs/snaps/shell.png` →
  `rune: 1 frames at 60 fps, gpu init 202.0 ms, first frame 25.18 ms, ... 243 instances,
  layout nodes 39, glyphs 205`; PNG 2880×1800 (window is 1440×900, headless scale defaults to 2×).
  Viewed the PNG: header, left layers sidebar, gridded map, right panels all render as intended.

### Headless flags (same renderer, fixed virtual clock)

```
--snap out.png [--scene NAME] [--at 0.4] [--size 620x520] [--scale 2] [--theme dark|light]
--tree tree.json [--at T]
--list-scenes
--script script.json --snap poster.png --at mark:end+0.5
--script script.json --video demo.mp4 [--fps 60] [--duration 3|auto]
--trace run.jsonl
--first-pixels                 # startup benchmark marker
```

`--snap` writes at `--scale` (default **2**) times the points in `AppSpec::new`, so pass
`--scale 1` if you want a 1440×1800-style 1:1 image. `--size WxH` overrides the window size for
the render. Script events include `move/down/up/tap/double-click/drag`, `{"type":"key","name":...}`,
`{"type":"type","text":...}`, `{"type":"wheel","dx":0,"dy":-40}`, `{"type":"magnify","delta":0.1}`,
`{"type":"rotate","degrees":15}`, `{"type":"mark","name":"end","t":3.2}`, `{"type":"resize",...}`.
Positions are `x,y` in points or `key` (or a key path like `"inspector/label"`).

## Rune API notes for later phases

All snippets are from Rune at the pinned rev (`crates/rune/README.md`, `examples/hello`,
`examples/shapes`, `examples/live-values`, `crates/rune/src/tooling.rs`).

### App skeleton, view fn, state and handlers

```rust
fn view(s: &State, cx: &mut Cx<State>) -> El { /* build a tree of El */ }

fn main() {
    rune::run(AppSpec::<State> {
        scenes: &[("shell", |s| { /* mutate initial state */ })],
        setup: Some(setup),   // fn(&mut State, Waker) — called once after runtime creation
        poll: Some(poll),     // fn(&mut State, f64 seconds, &mut Tx) -> bool
        ..AppSpec::new("ARGUS", (1440.0, 900.0), init, view)
    });
}
```

- The prose in Rune's README calls the first-frame hook `on_ready`; the **actual AppSpec field is
  `setup: Option<fn(&mut S, Waker)>`** (see `crates/rune/src/tooling.rs`).
- Handlers run between frames with `&mut State`; then the view re-runs, the diff retargets property
  tracks. Hover/press only retarget tracks — they do not re-run the view.
- Common handlers: `.on_press(|s: &mut S| ...)`, `.on_press_tx(|s, tx| ...)`,
  `.on_double_press(...)`, `.on_drag(|s, drag, tx| ...)`,
  `.on_wheel(|s, ev, tx| ...)`, `.on_magnify(|s, ev, tx| ...)`,
  `.on_pointer_move_local(|s, ev, tx| ...)`, `.on_secondary_press(|s, point, tx| ...)`.
  `Tx::clock()` gives the frame time (virtual in headless); `tx.wake_at(due)` schedules a wake.
- Builders: `row()`, `col()`, `el()`, `text("...")`, `canvas(shape)`,
  `.child(..)`, `.key("...")`, `.w()/.h()/.size()/.w_full()/.h_full()`,
  `.grow(f)`, `.min_w()/.min_h()`, `.padding(f)`, `.gap(f)`, `.center()`,
  `.fill(Color)`, `.radius(f)`, `.border(w, Color)`, `.opacity(f)`,
  `.font(type_::TITLE)`, `.color(color)`.
- Keys are a single namespace; scope app keys (e.g. `"layer/flights"`) because rune-kit keys its
  own parts (`label`, `value`, `icon`, ...). Key paths let `Cx::layout_size(key)` and `Tx` scroll
  requests address descendants.

### Layout gotchas (from the Rune README — these bite)

- `.grow(g)` is `flex: g 1 0` with basis 0. In an auto-height column there is no free space, so a
  grown `.scroll(..)` viewport collapses to 0 pt. **Bound an ancestor height** (`.h(..)` /
  `.h_full()`) or set `.min_h(0.0)`. Our `root` is `.h_full()`, `body` is `.grow(1.0).min_h(0.0)`.
- Flex items default to min-content size, so nested columns overflow instead of shrinking. Put
  `.min_h(0.0)` on **every** column between the bounded ancestor and a scroll viewport.
- `.virtualize(row_height)` only works on a growth stack: call `.growth(GrowthSpec)` on the same
  element and make it the scroll viewport's first child. Use `virtual_rows(...)` to mount only
  visible rows.
- Debug builds warn once per call site when a scroll viewport in a `.grow()` container resolves to
  0 pt while content is taller; `.virtualize` without `.growth` panics with the element path.

### Background work / polling / wake (important — reqwest must not block the UI thread)

Rune keeps network work off the UI thread with `rune::Latest<T>`. A handler fires a request; the
closure runs on one of up to four shared worker threads; the answer lands in state in `poll` on the
UI thread, woken by the `Waker` from `setup`:

```rust
struct State { results: rune::Latest<Vec<Hit>>, hits: Vec<Hit> }

fn init() -> State { State { results: Latest::default().debounce(Duration::from_millis(60)), hits: vec![] } }
fn setup(s: &mut State, waker: rune::Waker) { s.results.set_waker(waker); }   // pending state + debounce stay
fn on_search(s: &mut State, q: String) { s.results.request(q, |q| index_search(&q)); }
fn poll(s: &mut State, _t: f64, _tx: &mut Tx) -> bool {
    let Some(hits) = s.results.take() else { return false };  // true only when visible state changed
    s.hits = hits; true
}
```

- `Latest` is **latest-wins**; `request_if_changed(&key, f)` avoids restarting unchanged work from
  `poll`; `request_opt` skips waking; `cancel()` on window close. The closure is **synchronous**,
  so `reqwest::blocking` (what `argus-data` uses) is enough — no async runtime has to cross the
  Rune boundary. `tokio` is available in `argus-data` if a client needs its own async runtime.
- Debug builds warn when a handler or `poll` spends 8 ms waiting. Start slow work in `setup` (after
  the first frame) and land late-arriving state with `tx.instant()` so launch stays still.
- Modes/phases should be one `enum State` + one `fn step(..) -> Effects<Effect>` from
  `rune::machine`, not clusters of booleans. Timers live on `Tx::clock`, become `Tx::wake_at(due)`,
  and `poll` feeds them back; `Seq::next()` / `Attempt` fence stale answers.
- High-frequency camera pan/zoom should live in `Live<f32>` / `Live<[f32;2]>` values bound with
  `.live_offset(&v)` / `.live_scale(&v)`, with the handler calling `tx.live_only()` to skip view +
  diff + layout (measured 13.7 ms → 2.0 ms/frame on 2k cards). `LiveText` updates a single label.

### Canvas / Shape / pointer input (for the custom map)

```rust
use rune::{Color, LineCap, Path, Shape, canvas};

let mut s = Shape::new();
s = s.line([0.0, 50.0], [100.0, 50.0], 1.5, LineCap::Round, INK)  // polylines / segments
     .stroke(points, 2.0, INK)                                    // open/closed polyline
     .fill(polygon, FILL)                                         // ear-clipped polygon
     .stroke_path(&path, 3.0, LineCap::Round, INK)
     .fill_path(&path, FILL)
     .view_box(1000.0, 800.0);                                    // stretched to element size
canvas(s).key("map-canvas").grow(1.0).min_h(0.0).w_full().fill(MAP_BG)
```

`Path::new().move_to(..).line_to(..).quad_to(..).cubic_to(..).close()` flattens Béziers. Shapes
have no atlas, so they can change every frame (`examples/shapes`). Input coordinate spaces matter:
`on_drag` gives local x/y and window-space `dx/dy`; `on_wheel`/`on_magnify`/`on_rotate` give
**local** `x,y`; `on_pointer_move`/`on_secondary_press` give **window** points;
`on_pointer_move_local` gives local points plus `window_x/window_y`. `on_press` gives no point.
`tx.live_only()` works in wheel/magnify/drag/pointer-move handlers.

### Theming tokens

```rust
use rune::tok::{self, type_};
let t = tok::theme(true);           // dark Theme: bg, card, card_border, text, text_secondary,
                                    // separator, control_track, control_off, indicator, knob,
                                    // hover, press, shadow, shadow_soft, edge, knob_shadow
tok::accent(0, true);               // 6 accents: blue, purple, pink, orange, green, graphite
tok::space::{S1,S2,S3,S6};  tok::radius::{SM,MD,LG};
tok::motion::SNAPPY;                // .motion(..) springs
type_::{BODY, LABEL, SECTION, TITLE, MONO_CAPTION}  // TextStyle; .font(..)
```

`Color { r, g, b, a }` literals are sRGB (what our shell uses); `rune::oklch(l,c,h)` /
`oklcha(l,c,h,a)` build OKLCH colours — Rune's tokens use OKLCH and alphas tuned for linear-light
blending. `Env { system_dark, scale }` drives system theme; `cx.system_reduced_motion()` reads the
host preference. We will likely define our own tactical palette constants in `argus-app/src/main.rs`
(already present) instead of relying on rune-kit's default look.

### Scenes, goldens and headless tests

- `AppSpec::scenes` are named state mutators: `&[("name", |s| s.on = true)]`. Render one with
  `--snap out.png --scene name`, or `--at 0.4` to advance the virtual clock before the snap.
  `--theme dark|light` overrides the system appearance for the render.
- **Golden workflow:** commit a PNG produced by `--snap` (we start with `docs/snaps/shell.png`),
  then regenerate with the same `--size/--scale/--theme/--scene/--at` and diff pixels. Add a
  per-feature script JSON for interactions and `--script .. --snap .. --at mark:end` for the
  "after interaction" golden. `--tree out.json` is the cheaper structural assertion (keys, roles,
  labels, presented rects) and is stable for layout tests.
- Unit tests use Rune's `Runtime` directly, no window:

  ```rust
  let mut rt = Runtime::new(State::default(), view, (1440.0, 900.0), Env { system_dark: true, scale: 2.0 });
  rt.poll = Some(poll);
  rt.frame(0.0);                         // first frame
  let (x, y) = rt.find("switch").unwrap();
  rt.pointer_down(x, y); rt.pointer_up(x, y);
  for _ in 0..60 { rt.frame(1.0 / 60.0); } // springs settle in @60fps; time is virtual
  let rect = rt.presented("knob").unwrap(); // [x,y,w,h] as actually drawn
  ```
- `rune::motion_gate` records real frames in `cargo test` and fails on pops/jumps/flicker; use it
  once animations land. `headless_run()` / `ScriptEvent` let a test read the script being played.
- Rune's own README section "Testing stories" covers the built-in playground (`App::playground`,
  `rt.open_playground(..)`) if we want a storybook for map layers/panels.

## Phase 1 gotchas recap

1. Repeat all five vendored `[patch.crates-io]` git entries at the Rune rev — they are not inherited.
2. Rune pins Rust 1.99.1; the repo has a `rust-toolchain.toml` so `cargo` selects it automatically.
3. `AppSpec`'s first-frame hook is `setup`, not `on_ready`.
4. `--snap` renders at 2× by default; the window in `AppSpec::new` is 1440×900 points.
5. `.grow()` flex children need a bounded ancestor and `.min_h(0.0)` or they collapse to zero.
6. No tests were added that assert the PNG bytes yet — only the structural `--tree` check and the
   visual inspection above. Add golden assertions in Phase 2 when real features exist.

## argus-map API (Phase 5)

New crate `crates/argus-map` (lib, depends on `rune` + `reqwest`; **no** `argus-core`
dependency — it defines its own small overlay types for the app to map domain data onto).
Source layout: `projection.rs` (Mercator), `camera.rs` (camera + bounds clamp), `tile.rs`
(background fetch + caches), `overlay.rs` (generic features, cull/cluster/hit-test),
`state.rs` (`MapState`/`MapMsg`), `view.rs` (`map_view`), `examples/map_demo.rs`.

### Raster vs vector decision

Rune **can** draw arbitrary raster images: `ImageCache::load(bytes)` → `Image::pixels()`
→ `El::pixels(&pixels)`, and `El::absolute(left, top)` positions them (every El is a
containing block for absolute children). So Phase 5 uses **raster tiles**, not MVT
decoding — overlays are drawn GPU-side with `Shape`/`canvas` in the same view.

- **Default source: Esri World Dark Gray Canvas (Base)**, a keyless 256px dark JPEG
  raster (`server.arcgisonline.com/.../World_Dark_Gray_Base/MapServer/tile/{z}/{y}/{x}`,
  note the `z/y/x` order). Attribution: `Tiles © Esri · Esri, HERE, Garmin, © OpenStreetMap contributors`.
- **CARTO `dark_all` / `dark_nolabels` are supported but need a key**: since 2026-09-25
  keyless raster requests return an "API KEY REQUIRED" watermark tile. Set
  `ARGUS_CARTO_KEY` (or `TileStore::with_key`) to use CARTO cleanly; it is free for
  reasonable non-commercial/commercial use. Attribution: `© OpenStreetMap contributors · © CARTO`.
  `TileSource::attribution()` / `MapState::attribution()` return the right string.
- Every request sends a descriptive `User-Agent` (`USER_AGENT`).
- Tiles are fetched on Rune worker threads via `rune::Latest` + `reqwest::blocking`,
  cached in a bounded in-memory LRU (256 decoded `Image` handles by default) and on
  disk under `cache_dir()` (`ARGUS_CACHE_DIR` → XDG/HOME platform cache → temp).
  While a child tile loads, the nearest cached ancestor is drawn scaled at its
  correct geographic rect (no Rune image-cropping needed); exact tiles paint on top.
  Failed tiles are not retried every frame (parent fallback covers the gap).

### Camera / input

`MapCamera`: lng/lat centre + fractional zoom + viewport (pts) + bounds/zoom limits.
Defaults match `Map.tsx` (`103.8198, 1.3521`, zoom `10.8`); clamp is
`SINGAPORE_BOUNDS` (`103.55–104.15 E`, `1.10–1.55 N`), zoom `9..=17`. When the
viewport is larger than the bounds the centre (not the midpoint) is clamped, so the
default centre is preserved. `screen_to_lnglat` / `lnglat_to_screen` handle input;
`zoom_at` keeps the lng/lat under the cursor fixed. Handlers on the map element:
drag-to-pan (`tx.live_only()` on move), wheel + trackpad-pinch zoom around cursor,
double-click zoom (`Drag::clicks >= 2`), hover hit-test (only invalidates on change).
Click vs drag is distinguished by travel from the *press origin*.

### Overlays and events

`overlays::Overlays` holds `points` (`MapPoint` with `PointStyle`/`Icon`), `lines`
(`Polyline`), `polygons` (`Polygon`) and `circles` (`RadiusCircle`, metres).
Generic `OverlayId = u64`; colours are `Rgba` (no Rune in this type). `drawn_points`
culls to the viewport and, below `ClusterOptions::max_cluster_zoom` (default 12),
collapses points into 44pt screen-grid clusters (count shown as a text label).
`hit_test` / `hit_test_polygon` return the app's id. `radius_circle_ring` samples a
ground-radius circle. `MapMsg` = `OverlayClicked(id)` / `CameraChanged` /
`GeoFilterPicked(lnglat)`; the app drains `MapState::drain_messages()` in `poll`.

### App integration

```rust,ignore
struct App { map: MapState, overlays: Overlays }
fn view(s: &App, cx: &mut rune::Cx<App>) -> rune::El {
    argus_map::map_view_for(&s.map, &s.overlays, cx, |a| &mut a.map)
}
fn setup(s: &mut App, w: rune::Waker) { s.map.setup(w); /* + preload_sync() for snaps */ }
fn poll(s: &mut App, _t: f64, _tx: &mut rune::Tx) -> bool {
    let changed = s.map.poll();
    for m in s.map.drain_messages() { /* route to app */ }
    changed
}
```

- If the app's Rune state type *is* `MapState`, use `map_view(state, overlays, cx)`.
- Set `MapState::viewport` to the map element's size (views run before layout). Call
  `set_overlays(&overlays)` whenever the hit-test mirror changes.
- `map_view` returns a `.w_full().h_full()` element; size it inside a bounded ancestor.

### Verification / snap

- `cargo test -p argus-map`: 23 unit tests pass (projection round-trip + known values,
  tile zoom/scale/coverage + ancestors, camera clamp/anchor/drag, LRU eviction,
  CARTO/Esri URL building, culling/clustering, point/polygon hit-test, state messages).
- `cargo build --workspace`: green.
- `cargo run -p argus-map --example map_demo -- --snap docs/snaps/map.png --scene singapore --theme dark`
  → 2000×1360 PNG showing the real Esri dark Singapore basemap, 4 sample markers,
  an MRT-like polyline, a park polygon, a radius circle and the attribution. The demo
  calls `MapState::preload_sync()` in `setup` so tiles are fetched/decoded before the
  single headless frame (otherwise Rune's `Latest`/image workers would settle after it).

Limitations: no smooth camera spring (moves are instant), no label rendering inside
`Shape` (cluster counts use separate `text` Els), vector tiles are not decoded, and
the radius circle is an equirectangular approximation (accurate over Singapore-scale
radii only).

## argus-core API (Phase 2)

`crates/argus-core` is the shared domain crate (serde + serde_json only, no UI and no
network deps). It ports `types/index.ts`, `types/schemas.ts`, the pure parts of
`lib/`, and the non-React computation in `app/hooks/`. All modules are public from
`argus_core::*`.

### Module map (TS → Rust)

| Rust module | TypeScript source |
| --- | --- |
| `types` | `types/index.ts` — all domain types (`WeatherData`, `HawkerCentre`, `Gym`, `School`, `MrtGeoJson`, `ParksGeoJson`, bus contracts, `MapSearchResult`, …). serde renames match `public/*.json` camelCase and upstream PascalCase. |
| `api_types` | `types/schemas.ts` — external API response shapes (LTA, Data.gov v1/v2, Aviationstack, OpenSky). `NumberOrString`, wrapper unions, `LtaTrafficImagesResponse::into_cameras`. |
| `geo` | `lib/geo-filter.ts` — `GeoFilter`, `distance_m`, `geometry_touches_radius`, `clip_line_to_radius`, `clip_geometry_to_radius`, `within_radius`, `build_geo_filter_{circle,mask}_geojson`, `format_radius`. |
| `mrt_network` | `lib/mrt-network.ts` — operational/LRT/future station lists, `display_stations`, `lta_line_display_name`. |
| `mrt_routing` | `lib/mrt-routing.ts` — `MrtRoutePlan`, `MrtRouteSegment`, Dijkstra `plan_mrt_route`, `is_routeable_mrt_station`, `MRT_STATION_NAMES`. |
| `geometry` | `lib/map-geometry.ts` + `lib/map-features.ts` — bus-route/MRT GeoJSON builders, `interpolate_stations`, `build_mrt_station_geojson`, `route_station_keys`, `build_route_geojson`. |
| `format` | `lib/formatters.ts` — `format_sg_time`, `format_sg_date_time`, `singapore_today`/`singapore_date_for` (fixed UTC+8, no tz crate), `format_speed_kmh`, `format_altitude_feet`, `group_thousands`. |
| `signals` | `lib/signals.ts` + pure `lib/notifications.ts` — `SignalRow`, `build_signal_digest`, `diff_signals`, `select_notifiable`, `merge_active_signals`, `build_browser_notifications`. |
| `air_quality` | `lib/air-quality.ts` — PSI/PM2.5 bands and tones. |
| `mobility` | `lib/mobility.ts` — carpark/EV tones and popup lines, `PopupLine`, `AvailabilityTone`. |
| `maps_links` | `lib/maps-links.ts` — `google_maps_url`, `format_coordinates`. |
| `bundle_validation` | `lib/bundle-validation.ts` — `is_place`, `is_addressed_place`, `is_photo_list`, URL/one-of checks. |
| `data` | `lib/data/*` + `lib/park-photos.ts` — `DataDir`, `StaticData::load`, all `validate_*` fns, `photos_for_park`, `park_labels`. |
| `amenities` | `lib/amenities.ts` — mall/hawker popups, `photo_credit`, `format_opening_hours`, `format_closure_range`, `describe_hawker_closure`. |
| `popups` | `lib/{gyms,schools,heritage,healthcare,hotels,libraries,pharmacies,supermarkets,community-clubs}.ts` — label maps + `describe_*` popup line builders. |
| `amenity_layers` | `lib/amenity-layers.ts` — `AmenityRegistry` (11 types), sub-filter choices, `parse_amenity_filters`, deterministic collision `display_coords`, `find`/`co_located`, search results. |
| `parks` | `lib/parks.ts` — `format_park_name`, `area_hectares`, `park_label_point`, `describe_park_feature`, `build_park_search_results`. |
| `mall_stores` | `lib/mall-stores.ts` — `normalize_unit`/`normalize_level`/`normalize_website`, `clean_store_row`, `match_store_list_mall`, grouping/sorting/filtering. |
| `layers` | `lib/layers.ts` + `buildLayerGroups` from `app/hooks/use-layer-list.ts` — layer registry, `toggle_layer_option_selection`, `build_layer_groups`. |
| `search` | `lib/map-search.ts` — bus-stop/MRT search results + `rank_results`. |
| `preferences` | `lib/ui-preferences.ts` + `lib/local-history.ts` (pure merge) — pref parsers, `merge_point_lists`, `merge_point_stores`. |
| `flights` | `lib/flights.ts` — `summarize_flights`. |
| `model` | Phase 1 `Layer` skeleton (kept for compatibility). |

### Data-loading approach

Paths are resolved against a **configurable `DataDir`** (not `include_bytes!`):

```rust,ignore
let dir = argus_core::data::DataDir::default(); // $ARGUS_DATA_DIR or <crate>/../../public
let data = argus_core::data::StaticData::load(&dir)?;
```

`DataDir::new(path)` points at any bundle directory. Every bundle is validated on
load (mirroring the TS `validate*Bundle` refinements — ISO dates, enum membership,
host-allowlisted photos, unique ids, closure join consistency), then deserialised.
`StaticData` pre-processes NParks names, computes park areas/labels and joins hawker
closures. `StaticData::load_mall_store_list(&dir, id)` loads a per-mall directory.

### Key types/fns UI + data phases should use

- `types::{WeatherData, SignalRow, FlightState, MrtGeoJson, MapSearchResult, …}`.
- `data::{StaticData, DataDir}` for all static bundles; `amenity_layers::AmenityRegistry::new(&data)`.
- `geo::{GeoFilter, distance_m, clip_geometry_to_radius, build_geo_filter_circle_geojson}`.
- `mrt_routing::{plan_mrt_route, is_routeable_mrt_station}`; `geometry::{build_route_geojson, …}`.
- `signals::{build_signal_digest, diff_signals, select_notifiable, merge_active_signals}`.
- `layers::{LAYERS, default_sensor_visibility, build_layer_groups}`.

### Skipped TypeScript modules/tests (intentionally out of Phase 2 scope)

Network / platform / React / authoring only: `lib/api-clients/*`, `lib/api-fetch.ts`,
`lib/errors.ts`, `lib/rate-limit.ts`, `lib/shared-rate-limit.ts`, `lib/route-utils.ts`,
`lib/csp.ts`, `lib/cache*.ts`, `lib/feedback*.ts`, `lib/visitor-country.ts`,
`lib/features.ts`, `lib/local-history.ts` storage primitives (only pure merges ported),
and browser-notification API calls in `lib/notifications.ts`. Corresponding skipped
tests: `tests/api-*`, `tests/cache*`, `tests/{rate-limit,shared-rate-limit}.test.ts`,
`tests/{csp,feedback,api-fetch,route-utils,version,ui-*}.test.ts`, `tests/version.test.ts`,
`tests/place-fields.test.ts` and `tests/place-photos.test.ts` (both cover
`scripts/lib/*.mjs`, authoring-time helpers, not `lib/`), plus the React-markup
assertions in `tests/layers.test.tsx` (its pure `buildLayerGroups`/toggle logic is ported).

### Verification (Phase 2)

- `cargo test -p argus-core`: **67 passed** (58 inline unit tests + 9 integration
  tests in `tests/port_tests.rs`), 0 failed.
- `cargo build --workspace`: green.

## argus-data API (Phase 3)

New implementation of `crates/argus-data` (lib). It ports `lib/api-clients/*`,
the `app/api/*/route.ts` producers, `lib/cache.ts`, `lib/errors.ts`,
`lib/feedback*.ts` and the config/keys handling. Cloudflare rate limiting and
the Workers L2 cache are intentionally dropped (no Workers in a desktop app).
`reqwest::blocking` + rustls is used throughout, with a descriptive
`User-Agent`; every call is synchronous so it fits directly inside a Rune
`Latest` closure. `argus-data` depends only on `argus-core` (+ serde/reqwest/
config crates), never on `rune`.

### Entry point

```rust,ignore
let client = argus_data::Client::new(); // .env + ~/.config/argus/config.toml
let stops = client.get_bus_stops()?;
let weather = client.get_weather()?;
let health = client.source_health(); // SourceHealth[] for the panel
```

`Client::with_config(...)` and `Client::with_transport(config, Arc<dyn Transport>)`
are the test seams. Each source module adds inherent methods to `Client`, so
one client owns config, HTTP, cache and health. The old Phase 1 `ApiClient`
skeleton is kept for `argus-app` compatibility.

### Module map (TS → Rust)

| Rust module | TypeScript source |
| --- | --- |
| `http` | `lib/api-clients/http.ts` — `HttpClient::get_json` / `get_text_limited`, timeouts, `ExternalApi`/`SchemaParse`/`Timeout` mapping. `Transport` seam replaces stubbed `fetch` in tests. |
| `error` | `lib/errors.ts` — `ExternalApi`, `SchemaParse`, `Timeout`, `BadRequest`, `Forbidden`, plus `NotConfigured` for missing keys. |
| `config` | process env + `.env` (dotenvy) + `~/.config/argus/config.toml` (toml/dirs, override `ARGUS_CONFIG_FILE`). Keys: `LTA_API_KEY`, `AVIATIONSTACK_API_KEY`, optional `DATABASE_URL`. Placeholders become `None`. |
| `cache` | `lib/cache.ts` — per-key TTL cache, in-flight producer dedup (condvar), 10s/60s negative caching, oldest-entry eviction, value-dependent TTL (weather partial = 30s), open-ended `set`/`peek` (flights last-good). No L2. |
| `lta` | `lib/api-clients/lta.ts` + bus-arrivals/bus-stops/cameras/carparks/ev-chargers/incidents/train-alerts routes — pagination (`$skip`), 404-as-empty, camera URL allowlist, EV batch link allowlist, all normalizers. |
| `bus_routes` | `lib/api-clients/bus-routes.ts` + bus-routes route — BusRouter/sgbusdata polyline decoding, pattern scoring, loop rotation, direction selection, route assembly. Falls back to stop chords when geometry is unavailable. |
| `weather` | `lib/api-clients/weather.ts` — seven data.gov.sg feeds fetched in parallel; PSI/PM2.5/UV/four-day normalization; partial snapshots cached 30s. |
| `news` | `lib/api-clients/news.ts` — Straits Times + CNA RSS/Atom, entity decoding, categories, Singapore relevance filter, 512 KiB body cap, dedup, newest-first, top 20. |
| `flights` | `lib/api-clients/flights.ts` — AviationStack (4 endpoints) → OpenSky fallback → last-good 10-minute snapshot; direction classification (airport codes, then bearing to Changi); km/h → m/s; top 120. |
| `feedback` | `lib/feedback*.ts`, `lib/feedback-store.ts`, Neon SQL — `FeedbackSubmission` validation and `INSERT` via Neon's SQL-over-HTTP endpoint. |
| `health` | `app/hooks/use-dashboard-sources.ts` — per-source `ok/stale/error/unconfigured/loading`, last-updated ms, TS labels. |
| `wire` | (new) upstream spellings that differ from `argus-core::api_types`: `CameraID`, `CarParkID`, EV batch camelCase; converts into core types before normalizing. |
| `util`, `time` | sha256 short ids; ISO 8601 / RFC 2822 parsing to replace `Date.parse`/`toISOString` without chrono. |

### Cache keys / TTLs (from the TS clients)

`bus-stops` 24h · `bus-arrivals-{id}` 15s · `traffic-cameras` 60s · `incidents`
60s · `train-service-alerts` 60s · `carpark-availability` 60s ·
`ev-charging-locations` 5m · `bus-routes-index` 24h · `weather` 5m (30s partial)
· `news` 15m · `flights-sg` 15s · `flights-sg-fallback` open-ended (10m read
window). `busrouter-geometry-unavailable` is negatively cached for 5m.

### Config and missing keys

Keys are read from env (including `.env` in cwd) first, then the TOML file;
top-level keys and `[api]` / `[keys]` tables are accepted. A missing or
placeholder key becomes `NotConfigured`, which makes that source report
`unconfigured` rather than failing with a network error. LTA-backed sources
return that error; flights degrade to OpenSky; feedback returns a clear error
and does not insert. `DATABASE_URL` is optional.

### Feedback

Neon's serverless driver is one POST to `https://<host>/sql` with
`Neon-Connection-String: <DATABASE_URL>` and a parameterised `INSERT INTO
feedback (rating, message, country, ...)`; the existing `scripts/sql/feedback.sql`
table is reused. Without a URL the call is a no-op returning `NotConfigured`
(no driver, no schema changes). The live smoke test only checks config presence;
it never inserts a row.

### Verification (Phase 3)

- `cargo test -p argus-data`: **58 passed** (41 unit + 17 fixture-driven
  integration tests in `tests/clients.rs`), 0 failed. 12 `#[ignore]` live smoke
  tests in `tests/live_smoke.rs`.
- Live smoke (`cargo test -p argus-data --test live_smoke -- --ignored --nocapture --test-threads=1`,
  2026-11-17, real keys from `.env`): all 12 pass — bus stops 5209, bus
  arrivals 1, bus routes 2 directions, cameras 8, incidents 36, carparks 2127,
  EV chargers 2821, weather 4-day entries 4, news 20, flights 5, train alerts 1;
  feedback config present. All sources reported `ok`.
- `cargo build --workspace`: green.

Limitations: no aggregate request deadline per route (per-request timeouts are
used instead), concurrency in LTA page walks is sequential (skip order and
short-page termination are preserved), no disk cache, the app must supply its
own `Latest`/poll wiring (argus-data is deliberately UI-agnostic), and the
Neon feedback path sends SQL but was not exercised against the live database.

## argus-app structure (Phase 4)

The full dashboard lives in `crates/argus-app`. `main.rs` only wires
`AppSpec` (scenes, setup, poll) and composes the three regions; everything else
is split by responsibility:

| Module | Port of | Responsibility |
| --- | --- | --- |
| `theme.rs` | `app/globals.css` | Near-black paper/surface/line palette, semantic colours, type styles, `#rrggbb` helper |
| `state.rs` | `use-dashboard-state`, `use-mrt-planner`, `use-persistent-state` | `AppState`, prefs load/save, derived recompute (overlays, search, signals, counts), scenes |
| `msg.rs` | component callbacks | `Msg` enum + reducers: layers/sub-filters, geo filter, route planner, selection, toasts, feedback |
| `live.rs` | `use-dashboard-sources` | `DataSource` boundary, `Latest`-per-feed polling, on-demand feeds, bus arrivals/routes, argus-data health |
| `fixtures.rs` | — | Deterministic, clearly labelled fixture snapshot for goldens/tests (never live data) |
| `overlays.rs` | `Map.tsx` source assembly | Domain → `argus-map` overlays (POIs, MRT lines/stations, parks, radius circle, route, selected marker) |
| `search.rs` | `MapSearch.tsx`, `lib/map-search.ts` | Radius-scoped search index, ranking, hit → selection |
| `panels/header.rs` | `DashboardHeader`, `NotificationBell` | Clock, source counter, status dot, alert bell/popover |
| `panels/sidebar.rs` | `LayersSidebar`, `LayerControls`, `WeatherPanel`, `FlightPanel` | Layer groups + eye toggles + sub-filter chips, weather/air, flights |
| `panels/map.rs` | `Map.tsx`, `MapSearch.tsx` | `map_view_for` host, search trigger, legend, geo-pick hint, attribution |
| `panels/transit.rs` | `MrtRoutePanel`, `BusPanel` | Station pickers, swap/reset, route summary/segments, stop search, arrivals, route draw |
| `panels/feeds.rs` | `NewsPanel`, `CameraPanel`, `TerminalPanel` | News items, camera thumbnails + preview dialog |
| `panels/intel.rs` | `IntelPanel`, `GeoFilterPanel`, `NotificationToasts` | Signals, source-health bars, radius control, toasts |
| `panels/dialogs.rs` | `FocusDialog`, `FeedbackWidget`, `LoadingScreen`, `DashboardFooter` | Focus/search/camera/feedback modals, footer, boot screen |
| `tests.rs` | — | Eight reducer + Rune-runtime smoke tests |

### Data wiring

`init()` constructs one `argus_data::Client` (keys from `.env`/config, never
rendered or logged) and `LiveDataSource::install_live_fetchers` installs a
`Latest` closure per feed. Each closure calls the client's blocking method on a
Rune worker thread; `poll` lands the answer on the UI thread and schedules the
next wake at the SWR interval: bus stops 30m, cameras/incidents/carparks/train
alerts 1m, EV/weather 5m, news 15m, flights 15s, bus arrivals 15s. Carparks and
EV are only requested while their layer is on. Source health is copied from
`Client::source_health()` (including `unconfigured`/`stale`) into the panel.
Bus-route geometry and feedback submission are also Latest requests, so no HTTP
runs in a handler. Preferences are flushed to `~/.config/argus/preferences.json`
(override `ARGUS_CONFIG_DIR`) at most every 2 s via write-then-rename.

### Scenes and snaps

`--list-scenes` prints: `dashboard`, `layers-on`, `route-planner`, `poi-dialog`,
`panels`, `live`, `loading`.

- `docs/snaps/dashboard.png` — default view, unconfigured feeds
- `docs/snaps/layers-on.png` — several static POI layers + 3 km radius circle
- `docs/snaps/route-planner.png` — plan + white route overlay on the map
- `docs/snaps/poi-dialog.png` — amenity focus dialog
- `docs/snaps/panels.png` — all panels with deterministic fixture data
- `docs/snaps/live.png` — **real network data** preloaded in `setup` (the
  `live` scene calls `DataSource::preload_sync`, like argus-map's tile preload).
  Verified 5,209 bus stops, 39 incidents, 2,127 carparks, 2,821 EV points,
  20 news items, 8 cameras and live weather; no key material in the snap.
- `docs/snaps/shell.png` — Phase 1 shell (kept for history)

Snaps were rendered at `--scale 1 --theme dark`, inspected visually and checked
with `--tree` JSON. Visual fixes applied after inspection: bounded/scrolling
layer and weather panels, centred modal cards, no duplicate map chip, unique
camera-row keys, toasts moved over the map so the right column stays readable.

### Verification

- `cargo build --workspace`: green, no warnings from `argus-app`.
- `cargo test -p argus-app`: 8 passed, 0 failed (layer toggle, group toggle,
  overlay click → focus dialog, MRT route flow, geo filter, search, sub-filter,
  full Rune `Runtime` view smoke test).
- `cargo run -p argus-app -- --snap docs/snaps/live.png --scene live`: real
  feeds render in a single headless frame.

### Remaining gaps

~~Camera images are drawn as placeholders~~, ~~no photo gallery in the focus
dialog~~ — both were closed in Phase 4b below. OneMap search was **never a
runtime feature**: `lib/map-search.ts` builds a static index from the MRT
GeoJSON + live bus stops and `MapSearch.tsx` only filters that prop (the only
"OneMap" references in the TS tree are authoring-time `coordsSource` labels in
the schools bundle). The Rust `search` module already covers it, so there is
nothing to port. Feedback's Neon insert is implemented by argus-data but was
not exercised against a live database. The `loading` scene exists for
structural checks; the desktop app renders the dashboard directly because
feeds settle asynchronously rather than gating a single boot screen.

## Phase 4b — remaining UI features (2026-11-17)

### Raster image pipeline

`argus-data/src/images.rs` adds `is_allowed_image_url` (camera hosts from
`lta::is_safe_camera_url` plus Wikimedia/NEA photo hosts) and
`Client::get_image_bytes` (8 MiB cap, descriptive UA, `HttpClient::get_bytes_limited`
transport seam). `argus-app/src/images.rs` owns an `ImageStore`: a `Latest` batch
fetches bytes on Rune workers, `ImageCache` decodes them, and handles are
cached by URL and painted with `El::pixels_fit(.., SurfaceFit::Fill)` (the
original's `object-cover`). `preload_sync` fetches + `Image::wait()`s in `setup`
for deterministic headless frames. Camera frames are re-fetched whenever the
60 s camera feed lands (presigned URLs can stay the same while the JPEG
changes); failed URLs get a 30 s negative-cache cooldown.

- Road Cameras panel shows real thumbnails; clicking opens a large preview
  with coords, © LTA attribution and a manual REFRESH button.
- Mall focus dialog embeds the store directory: query, category chips, halal
  toggle, level-grouped rows and the datastore credit. State and view model in
  `mall_directory.rs` (reset per mall, like the TS `key={mallId}`).
- Amenity/park photos render in the focus dialog with thumbnails and the
  per-photo attribution line from `photo_gallery.rs`; broken URLs drop out once
  (TS `onError` equivalent).
- Map hover popups use argus-map's `hovered` overlay id and pointer position,
  with title/lines/photos built by `hover.rs` from the same argus-core
  `PopupLine` builders the TS app used (incidents, carparks, EV, amenities,
  parks + small labels for flights/stops/stations/cameras).

Visual fix: the Flights panel called
`format!("{}ft", format_altitude_feet(..))` although the helper already
appends ` ft`, rendering `2,925 ftft` and truncating the column; it now uses
the helper directly. Audited every snap for other duplicated units / overlap.

### Scenes and snaps

`--list-scenes` now prints: `dashboard`, `layers-on`, `route-planner`,
`poi-dialog`, `mall-directory`, `photo-gallery`, `hover-popup`, `panels`,
`live`, `cameras`, `camera-dialog`, `loading`. All were rendered at
`--scale 1 --theme dark` into `docs/snaps/` and visually inspected. `cameras`,
`camera-dialog`, `poi-dialog`, `mall-directory`, `photo-gallery` and the
hover/photo scenes preload real images in `setup`; `panels` and `hover-popup`
use deterministic fixtures. `map.png` and `shell.png` are kept from earlier
phases. No keys or presigned camera URLs are rendered into a PNG.

### Verification

- `cargo build --workspace`: green, no warnings.
- `cargo test --workspace`: argus-core 58 + 9 integration, argus-data 43 + 17
  integration (12 live smoke ignored), argus-app 23, argus-map 23 — all green.
  New tests cover image dedupe/fresh/negative-cache, image URL allowlist,
  mall-directory filtering/reset, gallery clamp + attribution, hover popup
  content, and the new scenes' state + Rune `Runtime` smoke.
- `cargo run -p argus-app -- --snap docs/snaps/cameras.png --scene cameras`
  shows real decoded LTA JPEG frames.
