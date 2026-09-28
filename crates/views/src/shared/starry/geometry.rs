use std::f32::consts::TAU;

use gpui::{Bounds, PathBuilder, Pixels, Point, point, px};

use super::SEGMENTS;

/// The stage's centre, or `None` while the canvas is still empty.
pub(super) fn center(bounds: Bounds<Pixels>) -> Option<Point<Pixels>> {
    let side = bounds.size.width.min(bounds.size.height);
    match side <= px(1.) {
        true => None,
        false => Some(point(
            bounds.origin.x + bounds.size.width / 2.,
            bounds.origin.y + bounds.size.height / 2.,
        )),
    }
}

/// Where `radius` out from `center` at `angle` lies.
pub(super) fn on(center: Point<Pixels>, radius: f32, angle: f32) -> Point<Pixels> {
    point(
        center.x + px(radius * angle.cos()),
        center.y + px(radius * angle.sin()),
    )
}

/// Appends a full circle as a closed polygon of `SEGMENTS` straight segments.
pub(super) fn circle(builder: &mut PathBuilder, center: Point<Pixels>, radius: f32) {
    let step = TAU / SEGMENTS as f32;
    for segment in 0..=SEGMENTS {
        let at = on(center, radius, segment as f32 * step);
        match segment {
            0 => builder.move_to(at),
            _ => builder.line_to(at),
        }
    }
    builder.close();
}

/// Appends a disc whose own size decides how many segments it takes. A particle
/// is a few pixels across and does not need the seventy-two a record does, and
/// a thousand of them share a path.
pub(super) fn disc(builder: &mut PathBuilder, center: Point<Pixels>, radius: f32) {
    let cells = ((radius * 0.8).ceil() as usize).clamp(6, SEGMENTS);
    let step = TAU / cells as f32;
    for cell in 0..=cells {
        let at = on(center, radius, cell as f32 * step);
        match cell {
            0 => builder.move_to(at),
            _ => builder.line_to(at),
        }
    }
    builder.close();
}

/// Appends a curved run from `from` to `to`, subdivided finely enough that the
/// chords never read as corners at these radii.
pub(super) fn arc(
    builder: &mut PathBuilder,
    center: Point<Pixels>,
    radius: f32,
    from: f32,
    to: f32,
) {
    let cells = ((to - from).abs() / 0.06).ceil().max(3.) as usize;
    for cell in 0..=cells {
        let at = on(
            center,
            radius,
            from + (to - from) * cell as f32 / cells as f32,
        );
        match cell {
            0 => builder.move_to(at),
            _ => builder.line_to(at),
        }
    }
}

/// Appends an annular sector — the sheen's cell — between two radii and two
/// angles, both ends curved.
pub(super) fn wedge(
    builder: &mut PathBuilder,
    center: Point<Pixels>,
    inner: f32,
    outer: f32,
    from: f32,
    to: f32,
) {
    let cells = ((to - from).abs() / 0.06).ceil().max(3.) as usize;
    builder.move_to(on(center, inner, from));
    builder.line_to(on(center, outer, from));
    for cell in 1..=cells {
        builder.line_to(on(
            center,
            outer,
            from + (to - from) * cell as f32 / cells as f32,
        ));
    }
    builder.line_to(on(center, inner, to));
    builder.line_to(on(center, inner, from));
    builder.close();
}

/// A stable pseudo-random fraction of the seed, in 0..1. The constants are the
/// usual integer-mixing suspects; only that the output is spread evenly and
/// repeatable matters here.
pub(super) fn scatter(seed: u32) -> f32 {
    let mut mixed = seed.wrapping_mul(0x9E37_79B9).wrapping_add(0x85EB_CA6B);
    mixed ^= mixed >> 13;
    mixed = mixed.wrapping_mul(0xC2B2_AE35);
    mixed ^= mixed >> 16;
    (mixed >> 8) as f32 / 16_777_216.
}
