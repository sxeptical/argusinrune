//! Colour / type tokens transcribed from `app/globals.css`.
//!
//! The original dashboard is a neutral near-black workbench: paper/surface
//! greys, hairline borders and semantic green / amber / red / blue accents.
//! Every value here is straight sRGB so it matches Rune's `Color` literal.

use rune::{Color, TextStyle};

const fn rgb(r: f32, g: f32, b: f32) -> Color {
    Color { r, g, b, a: 1.0 }
}

pub fn alpha(c: Color, a: f32) -> Color {
    Color { a, ..c }
}

// Surfaces (oklch(0.08 …) → sRGB approximations of globals.css).
pub const PAPER: Color = rgb(0.051, 0.051, 0.051);
pub const SURFACE: Color = rgb(0.086, 0.086, 0.086);
pub const SURFACE_RAISED: Color = rgb(0.122, 0.122, 0.122);
pub const SURFACE_HOVER: Color = rgb(0.161, 0.161, 0.161);
pub const LINE: Color = rgb(0.225, 0.225, 0.225);
pub const LINE_STRONG: Color = rgb(0.361, 0.361, 0.361);
pub const INK: Color = rgb(0.957, 0.957, 0.957);
pub const MUTED: Color = rgb(0.655, 0.655, 0.655);
pub const FAINT: Color = rgb(0.475, 0.475, 0.475);
pub const OVERLAY: Color = Color { r: 0.031, g: 0.031, b: 0.031, a: 0.88 };

// Semantics.
pub const SUCCESS: Color = rgb(0.33, 0.84, 0.55);
#[allow(dead_code)]
pub const SUCCESS_SOFT: Color = rgb(0.11, 0.24, 0.16);
pub const SUCCESS_LINE: Color = rgb(0.30, 0.52, 0.37);
pub const WARNING: Color = rgb(0.92, 0.72, 0.32);
pub const DANGER: Color = rgb(0.91, 0.40, 0.28);
pub const INFO: Color = rgb(0.46, 0.74, 0.93);
pub const BUS: Color = rgb(0.50, 0.94, 0.68);

// Map palette (the basemap is Esri dark grey; overlays are brighter).
pub const MAP_BG: Color = rgb(0.055, 0.065, 0.070);
#[allow(dead_code)]
pub const MAP_GRID: Color = Color { r: 0.35, g: 0.38, b: 0.40, a: 0.35 };

// Text styles. The TS app uses 9–14px mono/sans with wide uppercase tracking.
pub const MICRO: TextStyle = TextStyle::new(9.0, 500.0).tracking(0.9);
pub const MICRO_MONO: TextStyle = TextStyle::new(9.0, 500.0).mono().tracking(0.6);
pub const TINY: TextStyle = TextStyle::new(10.0, 400.0).tracking(0.3);
pub const TINY_MONO: TextStyle = TextStyle::new(10.0, 500.0).mono().tracking(0.2);
pub const SMALL: TextStyle = TextStyle::new(11.0, 400.0);
pub const SMALL_MONO: TextStyle = TextStyle::new(11.0, 400.0).mono();
pub const BODY: TextStyle = TextStyle::new(12.0, 400.0);
pub const BODY_MONO: TextStyle = TextStyle::new(12.0, 400.0).mono();
pub const LABEL: TextStyle = TextStyle::new(11.5, 600.0).tracking(1.1);
#[allow(dead_code)]
pub const HEADING: TextStyle = TextStyle::new(12.0, 600.0).tracking(1.2);
pub const TITLE: TextStyle = TextStyle::new(15.0, 600.0).tracking(2.2);
#[allow(dead_code)]
pub const VALUE: TextStyle = TextStyle::new(15.0, 500.0).mono();
pub const DIALOG_TITLE: TextStyle = TextStyle::new(16.0, 500.0).mono();

/// Colour for a semantic tone name used by argus-core (`text-success`, `bg-danger`, …).
pub fn tone_color(class: &str) -> Color {
    if class.contains("danger") || class.contains("outbound") {
        DANGER
    } else if class.contains("warning") || class.contains("mrt") {
        WARNING
    } else if class.contains("info") || class.contains("transit") {
        INFO
    } else if class.contains("success") || class.contains("inbound") || class.contains("park") {
        SUCCESS
    } else if class.contains("bus") {
        BUS
    } else if class.contains("faint") {
        FAINT
    } else if class.contains("muted") {
        MUTED
    } else {
        INK
    }
}

/// `#rrggbb` (the amenity registry's colours) → Rune colour.
pub fn hex_color(hex: &str) -> Color {
    let h = hex.trim_start_matches('#');
    if h.len() < 6 {
        return INK;
    }
    let chan = |a: usize, b: usize| {
        u8::from_str_radix(&h[a..b], 16).ok().map(|v| v as f32 / 255.0).unwrap_or(1.0)
    };
    rgb(chan(0, 2), chan(2, 4), chan(4, 6))
}
