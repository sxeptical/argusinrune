//! Shared Rune element builders for the dashboard panels.
#![allow(dead_code)]

use rune::{Color, El, ScrollSpec, col, el, row, text};

use crate::msg::{Msg, dispatch};
use crate::state::AppState;
use crate::theme::*;

/// The `IntelPanel` from the TS app: hairline-bordered section, pinned header,
/// optional scrollable body.
pub fn panel(key: &str, title: &str, badge: impl Into<String>) -> El {
    col().key(format!("panel/{key}")).w_full().min_h(0.0)
        .fill(SURFACE).border(1.0, LINE)
        .child(
            row().key(format!("panel/{key}/head")).h(30.0).w_full().padding(8.0).gap(8.0).center_y()
                .child(text(title).font(LABEL).color(INK).truncate())
                .child(el().grow(1.0))
                .child(text(badge.into()).font(MICRO_MONO).color(MUTED).truncate()),
        )
}

/// Collapsible `TerminalPanel` equivalent. `key` is persisted in preferences
/// and is also the panel's Rune key prefix.
pub fn terminal(s: &AppState, key: &str, title: &str, body: El) -> El {
    let collapsed = s.collapsed_groups.contains(key);
    let msg = Msg::TogglePanelCollapsed(key.to_string());
    let chevron = if collapsed { "+" } else { "−" };
    let mut out = panel(key, title, "");
    // Replace the badge with the expand/collapse control: a second header row
    // would waste vertical space in these dense panels.
    out = out.child(
        row().key(format!("panel/{key}/toggle")).h(24.0).w_full().px(8.0).gap(8.0).center_y()
            .fill(SURFACE_HOVER)
            .on_press(move |s: &mut AppState| dispatch(s, msg.clone()))
            .child(text(format!("{chevron} {}", if collapsed { "EXPAND" } else { "COLLAPSE" }))
                .font(MICRO).color(MUTED)),
    );
    if collapsed { out } else { out.child(body) }
}

pub fn scroll_body(key: &str, child: El) -> El {
    col().key(format!("{key}/body")).w_full().min_h(0.0).grow(1.0)
        .scroll(ScrollSpec { stick_bottom: false }).padding(8.0).gap(6.0).child(child)
}

pub fn data_row(key: &str, child: El) -> El {
    col().key(key).w_full().min_h(0.0).fill(SURFACE_RAISED)
        .border(1.0, LINE).padding(7.0).gap(2.0).child(child)
}

pub fn label(text_str: impl Into<String>) -> El {
    text(text_str.into()).font(MICRO).color(MUTED).truncate()
}

pub fn caption(text_str: impl Into<String>) -> El {
    text(text_str.into()).font(TINY).color(MUTED).max_lines(2)
}

pub fn mono(text_str: impl Into<String>, color: Color) -> El {
    text(text_str.into()).font(TINY_MONO).color(color)
}

pub fn value(text_str: impl Into<String>, color: Color) -> El {
    text(text_str.into()).font(BODY_MONO).color(color).truncate()
}

pub fn key_value(key: &str, name: &str, val: impl Into<String>, color: Color) -> El {
    row().key(key).w_full().min_h(0.0).padding(4.0).gap(8.0)
        .child(label(name).w(74.0))
        .child(text(val.into()).font(SMALL_MONO).color(color).min_w(0.0).grow(1.0).wrap())
}

pub fn dot(color: Color) -> El {
    el().w(6.0).h(6.0).radius(3.0).fill(color)
}

/// Compact action button matching `.action-button` / `.field-control`.
pub fn press(key: &str, title: impl Into<String>, msg: Msg, active: bool) -> El {
    row().key(format!("btn/{key}")).h(26.0).min_w(42.0).padding(8.0).center()
        .fill(if active { INK } else { SURFACE_RAISED })
        .border(1.0, if active { INK } else { LINE })
        .on_press(move |s: &mut AppState| dispatch(s, msg.clone()))
        .child(text(title.into()).font(MICRO).color(if active { PAPER } else { MUTED }).truncate())
}

pub fn press_color(key: &str, title: impl Into<String>, msg: Msg, color: Color) -> El {
    row().key(format!("btn/{key}")).h(26.0).min_w(42.0).padding(8.0).center()
        .fill(alpha(color, 0.12)).border(1.0, alpha(color, 0.5))
        .on_press(move |s: &mut AppState| dispatch(s, msg.clone()))
        .child(text(title.into()).font(MICRO).color(color).truncate())
}

/// Horizontal separator (hairline).
pub fn rule() -> El {
    el().key("rule").w_full().h(1.0).fill(LINE)
}

/// Empty / loading / unconfigured hint used while feeds are not wired.
pub fn hint(text_str: impl Into<String>) -> El {
    text(text_str.into()).font(TINY).color(FAINT).max_lines(3)
}
