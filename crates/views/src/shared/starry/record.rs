use std::f32::consts::{PI, TAU};

use gpui::{Bounds, PathBuilder, Pixels, Point, Window, px};

use super::geometry::{arc, center, circle, wedge};
use super::paint::{Palette, SHEEN};
use super::{DISC, LABEL, SPIN};

/// Where the grooves run, as shares of the record's radius: outside the cover,
/// which is all of the record the eye can see.
const GROOVE_IN: f32 = 0.72;
const GROOVE_OUT: f32 = 0.985;
/// How many grooves the record wears.
const GROOVES: usize = 20;
/// The sheen: one pass's angular width in radians, and how many cells a pass is
/// subdivided into so its edges fade. How bright it gets is the paint's to say.
const SHEEN_SPAN: f32 = 0.72;
const SHEEN_CELLS: usize = 10;
/// Short bright arcs on the grooves, rotating with the sheen. On a texture of
/// perfect circles they are the only cue that the record itself is turning.
/// They ride between the cover and the rim, the only vinyl there is to see.
const MARKERS: [f32; 3] = [0.76, 0.84, 0.92];
const MARKER_SPAN: f32 = 0.26;

/// The record itself: face, grooves and rim. The grooves alternate two tones
/// so the surface has texture even where the sheen isn't passing.
pub(super) fn record(center: Point<Pixels>, radius: f32, paint: &Palette, window: &mut Window) {
    let mut face = PathBuilder::fill();
    circle(&mut face, center, radius);
    match face.build() {
        Ok(path) => window.paint_path(path, paint.disc),
        Err(error) => log::warn!("starry: cannot build the record: {error}"),
    }

    let mut light = PathBuilder::stroke(px(1.));
    let mut dark = PathBuilder::stroke(px(1.));
    for groove in 0..GROOVES {
        let t = GROOVE_IN + (GROOVE_OUT - GROOVE_IN) * groove as f32 / (GROOVES - 1) as f32;
        match groove % 2 == 0 {
            true => circle(&mut light, center, radius * t),
            false => circle(&mut dark, center, radius * t),
        }
    }
    circle(&mut light, center, radius * 0.995);
    match light.build() {
        Ok(path) => window.paint_path(path, paint.groove),
        Err(error) => log::warn!("starry: cannot build the grooves: {error}"),
    }
    match dark.build() {
        Ok(path) => window.paint_path(path, paint.groove_dark),
        Err(error) => log::warn!("starry: cannot build the grooves: {error}"),
    }

    let mut rim = PathBuilder::stroke(px(1.5));
    circle(&mut rim, center, radius - 0.75);
    match rim.build() {
        Ok(path) => window.paint_path(path, paint.rim),
        Err(error) => log::warn!("starry: cannot build the rim: {error}"),
    }
}

/// Everything that sweeps across the record's face as it turns: two sheen
/// passes opposite each other, three bright arcs riding the grooves, and a
/// hairline pressing the label's edge into the record so the two read as
/// separate surfaces. There is no spindle: the cover covers seven tenths of
/// the record and sits over the hole, the way a real label does. With y
/// pointing down, a growing angle already turns clockwise on screen.
pub(super) fn sheen(bounds: Bounds<Pixels>, turn: f32, paint: &Palette, window: &mut Window) {
    let Some(center) = center(bounds) else {
        return;
    };
    let side = bounds.size.width.min(bounds.size.height).as_f32();
    let radius = side * DISC / 2.;
    let angle = TAU * turn / SPIN;

    // Two sheen passes, opposite each other, each a fan of cells whose
    // opacity rises to a peak in the middle and falls off at both edges.
    // The fan is what rotates; the cells only keep its ends soft.
    for pass in 0..2 {
        let base = angle + pass as f32 * PI;
        for cell in 0..SHEEN_CELLS {
            let from = base + SHEEN_SPAN * cell as f32 / SHEEN_CELLS as f32;
            let to = base + SHEEN_SPAN * (cell + 1) as f32 / SHEEN_CELLS as f32;
            let alpha = SHEEN * (PI * (cell as f32 + 0.5) / SHEEN_CELLS as f32).sin();
            let mut builder = PathBuilder::fill();
            // From the cover's edge outwards: the sheen sweeps the vinyl,
            // not the art it carries.
            wedge(
                &mut builder,
                center,
                radius * LABEL,
                radius * 0.985,
                from,
                to,
            );
            match builder.build() {
                Ok(path) => window.paint_path(path, paint.sheen.opacity(alpha)),
                Err(error) => log::warn!("starry: cannot build the sheen: {error}"),
            }
        }
    }

    // Three short bright arcs riding the grooves at the sheen's angle.
    for marker in MARKERS {
        let mut builder = PathBuilder::stroke(px(1.2));
        arc(
            &mut builder,
            center,
            radius * marker,
            angle + 0.4,
            angle + 0.4 + MARKER_SPAN,
        );
        match builder.build() {
            Ok(path) => window.paint_path(path, paint.marker),
            Err(error) => log::warn!("starry: cannot build a marker: {error}"),
        }
    }

    let mut edge = PathBuilder::stroke(px(1.));
    circle(&mut edge, center, radius * LABEL + 0.5);
    match edge.build() {
        Ok(path) => window.paint_path(path, paint.label_edge),
        Err(error) => log::warn!("starry: cannot build the label edge: {error}"),
    }
}
