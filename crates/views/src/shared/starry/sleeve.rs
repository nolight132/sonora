use std::f32::consts::TAU;

use gpui::PathBuilder;
use gpui::prelude::*;
use gpui::{Bounds, Div, Pixels, SharedString, Window, canvas, div, point, px};
use ui::Artwork;

use super::SPIN;
use super::geometry::{arc, circle};
use super::paint::Palette;

/// The sleeve: the cover's edge and the record's diameter as shares of the
/// stage's side, how far the record's centre sits past the cover's right edge,
/// where the cover starts, and the label's diameter as a share of the
/// record's. Together they leave the record peeking out to the right with
/// about half of it showing, the way a record slid out of its sleeve sits in
/// the hand.
const SLEEVE: f32 = 0.64;
const SLEEVE_DISC: f32 = 0.52;
const SLEEVE_CX: f32 = 0.68;
const SLEEVE_X: f32 = 0.025;
const SLEEVE_LABEL: f32 = 0.52;
/// The sleeve's grooves, as shares of the record's radius: they run outside
/// the label and stop short of the rim.
const SLEEVE_GROOVES: usize = 6;
const SLEEVE_GROOVE_IN: f32 = 0.58;
const SLEEVE_GROOVE_OUT: f32 = 0.94;
/// How far the sleeve's own sheen runs along the rim, in radians. The sleeve's
/// record peers out from behind the cover, so this short arc is most of what
/// there is to see of it turning.
const SLEEVE_SHEEN: f32 = 0.26;

/// The sleeve layout: the cover slid most of the way out of its sleeve — that
/// is, off the record behind it — so the record peeks out to the right with
/// the cover's own art turning on it as its label. The cover itself stays
/// square and still; the turn belongs to the record and its label.
pub(super) fn sleeve(
    side: f32,
    label: Option<SharedString>,
    waiting: bool,
    turn: f32,
    paint: &Palette,
    radius: Pixels,
) -> Div {
    let cover = side * SLEEVE;
    let disc = side * SLEEVE_DISC;
    let center_x = side * SLEEVE_CX;
    let center_y = side / 2.;
    let label_side = disc * SLEEVE_LABEL;

    div()
        .relative()
        .size(px(side))
        .child(
            canvas(move |_, _, _| {}, {
                let paint = *paint;
                move |bounds, _, window, _| platter(bounds, turn, &paint, window)
            })
            .absolute()
            .inset_0(),
        )
        .child(
            div()
                .absolute()
                .left(px(center_x - label_side / 2.))
                .top(px(center_y - label_side / 2.))
                // The cover's own art rides the record as its label, turning
                // with it the way a record label does.
                .child(
                    Artwork::new(label.clone())
                        .size(px(label_side))
                        .spin(turn / SPIN)
                        .circle()
                        .soft(waiting),
                ),
        )
        .child(
            div()
                .absolute()
                .left(px(side * SLEEVE_X))
                .top(px((side - cover) / 2.))
                .child(
                    Artwork::new(label)
                        .size(px(cover))
                        .corner_radius(radius)
                        .soft(waiting),
                ),
        )
}

/// The sleeve's record, as a pastel pressing: face, grooves, a sheen riding
/// the turn, and the spindle hole at its centre. The label — the cover's own
/// art, turning — is an element laid over this, not painted here.
fn platter(bounds: Bounds<Pixels>, turn: f32, paint: &Palette, window: &mut Window) {
    let side = bounds.size.width.min(bounds.size.height).as_f32();
    // Painted paths read in window coordinates, so the record's centre is the
    // canvas origin plus its share of the stage's side.
    let center = bounds.origin + point(px(side * SLEEVE_CX), px(side / 2.));
    let radius = side * SLEEVE_DISC / 2.;
    let angle = TAU * turn / SPIN;

    let mut face = PathBuilder::fill();
    circle(&mut face, center, radius);
    match face.build() {
        Ok(path) => window.paint_path(path, paint.record),
        Err(error) => log::warn!("starry: cannot build the platter: {error}"),
    }

    let mut grooves = PathBuilder::stroke(px(1.));
    for groove in 0..SLEEVE_GROOVES {
        let t = SLEEVE_GROOVE_IN
            + (SLEEVE_GROOVE_OUT - SLEEVE_GROOVE_IN) * groove as f32 / (SLEEVE_GROOVES - 1) as f32;
        circle(&mut grooves, center, radius * t);
    }
    match grooves.build() {
        Ok(path) => window.paint_path(path, paint.record_groove),
        Err(error) => log::warn!("starry: cannot build the platter's grooves: {error}"),
    }

    let mut rim = PathBuilder::stroke(px(1.5));
    circle(&mut rim, center, radius - 0.75);
    match rim.build() {
        Ok(path) => window.paint_path(path, paint.record_edge),
        Err(error) => log::warn!("starry: cannot build the platter's rim: {error}"),
    }

    // A short sheen riding the rim with the turn, the only cue on an even
    // pastel face that the record is moving at all.
    let mut sheen = PathBuilder::stroke(px(2.));
    arc(
        &mut sheen,
        center,
        radius * 0.97,
        angle + 0.4,
        angle + 0.4 + SLEEVE_SHEEN,
    );
    match sheen.build() {
        Ok(path) => window.paint_path(path, paint.record_groove),
        Err(error) => log::warn!("starry: cannot build the platter's sheen: {error}"),
    }

    let pin = (side * 0.008).max(2.5);
    let mut hole = PathBuilder::fill();
    circle(&mut hole, center, pin);
    match hole.build() {
        Ok(path) => window.paint_path(path, paint.hole),
        Err(error) => log::warn!("starry: cannot build the spindle hole: {error}"),
    }
    let mut rim = PathBuilder::stroke(px(1.));
    circle(&mut rim, center, pin + 1.2);
    match rim.build() {
        Ok(path) => window.paint_path(path, paint.record_edge),
        Err(error) => log::warn!("starry: cannot build the spindle hole's rim: {error}"),
    }
}
