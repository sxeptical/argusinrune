//! News and traffic-camera panels.

use argus_core::format::format_sg_date_time;
use rune::{Color, Cx, El, LineCap, Shape, canvas, col, el, row, text};

use crate::msg::{Msg, dispatch};
use crate::panels::common::*;
use crate::state::AppState;
use crate::theme::*;

pub fn news_panel(s: &AppState, _: &mut Cx<AppState>) -> El {
    let news = &s.data.snapshot().news;
    let out = panel("news", "News", format!("{} items", news.len()));
    let mut body = col().key("news/body").w_full().min_h(0.0).gap(6.0).padding(6.0);
    if news.is_empty() {
        body = body.child(hint("No news signals available."));
    }
    for item in news.iter().take(8) {
        body = body.child(
            col().key(format!("news/{}", item.published_at)).w_full().min_h(0.0).padding(6.0)
                .gap(2.0).fill(SURFACE_RAISED).border(1.0, LINE)
                .child(text(&item.title).font(SMALL).color(INK).max_lines(2))
                .child(row().w_full().gap(6.0)
                    .child(text(&item.source).font(MICRO).color(MUTED).truncate().grow(1.0))
                    .child(text(format_sg_date_time(&item.published_at)).font(MICRO).color(FAINT).truncate())),
        );
    }
    out.child(scroll_body("news", body))
}

fn camera_thumb() -> Shape {
    let mut s = Shape::new();
    s = s.line([0.0, 4.0], [100.0, 40.0], 1.0, LineCap::Butt, Color { a: 0.25, ..LINE_STRONG })
        .line([0.0, 40.0], [100.0, 4.0], 1.0, LineCap::Butt, Color { a: 0.25, ..LINE_STRONG });
    s.view_box(100.0, 44.0)
}

pub fn cameras_panel(s: &AppState, _: &mut Cx<AppState>) -> El {
    let cams = &s.data.snapshot().cameras;
    let out = panel("cameras", "Road Cameras", format!("{} nodes", cams.len()));
    let thumb = camera_thumb();
    let mut body = col().key("cameras/body").w_full().min_h(0.0).gap(6.0).padding(6.0);
    if cams.is_empty() {
        body = body.child(hint("No traffic camera images available."));
    }
    let selected = s.camera_dialog.as_deref();
    let ordered: Vec<_> = match selected {
        Some(id) => {
            let mut v: Vec<_> = cams.iter().filter(|c| &c.camera_id == id).collect();
            v.extend(cams.iter().filter(|c| &c.camera_id != id).take(3));
            v
        }
        None => cams.iter().take(4).collect(),
    };
    // Two thumbnails per row.
    for (row_i, pair) in ordered.chunks(2).enumerate() {
        let mut r = row().key(format!("cameras/row/{row_i}")).w_full().min_h(0.0).gap(6.0);
        for cam in pair {
            let msg = Msg::SelectCamera(cam.camera_id.clone());
            r = r.child(
                col().key(format!("camera/{}", cam.camera_id)).w_full().min_w(0.0).gap(2.0)
                    .fill(SURFACE_RAISED).border(1.0, LINE).on_press(move |s: &mut AppState| dispatch(s, msg.clone()))
                    .child(canvas(thumb.clone()).key(format!("camera/{}/img", cam.camera_id)).w_full().h(44.0).fill(PAPER))
                    .child(text(&cam.location).font(MICRO).color(MUTED).truncate().padding(4.0)),
            );
        }
        body = body.child(r);
    }
    if cams.is_empty() {
        body = body.child(el());
    }
    out.child(scroll_body("cameras", body))
}
