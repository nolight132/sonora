use std::f32::consts::TAU;

use gpui::{Bounds, PathBuilder, Pixels, Point, Window, point, px};
use ui::{Levels, VisualizerStyle};

use super::geometry::{center, circle, disc, on, scatter};
use super::paint::Palette;
use super::record::record;
use super::{DISC, Motion, SPIN, spin};

/// The ring: how far its bars start off the rim, and how far a full band
/// reaches, as shares of the stage's side.
const RING_GAP: f32 = 0.025;
const RING_REACH: f32 = 0.075;
/// The lowest band stands this tall, so a silent track still shows a hairline.
const RING_FLOOR: f32 = 0.015;
/// The wave's echo, the second layer: how far it sits outside the line itself,
/// in pixels; how much of the ring's reach it keeps, squashed so it keeps to
/// the line's quieter register; and how far its own slow swell wanders, in
/// pixels, so the two layers part and cross instead of tracking.
const WAVE_ECHO: f32 = 6.;
const ECHO_REACH: f32 = 0.72;
const WAVE_SWELL: f32 = 3.;
/// How wide the halo's reach is past the rim, as a share of the side. It has
/// to fade out before the stage's own edge or the layer clips it mid-glow.
const HALO_REACH: f32 = 0.10;
/// How many concentric discs fake the halo's radial falloff.
const HALO: usize = 12;
/// How bright a particle gets at the top of its drift, and how many shades that
/// brightness is rounded into. Every particle of one shade shares a path, so
/// the field costs one draw per shade and not one per particle.
const STAR: f32 = 0.52;
const TONES: usize = 8;

/// The field under the label: halo, spectrum ring and, when the record is
/// staged, its face and grooves.
pub(super) fn field(
    bounds: Bounds<Pixels>,
    levels: &Levels,
    style: VisualizerStyle,
    vinyl: bool,
    paint: &Palette,
    window: &mut Window,
) {
    let Some(center) = center(bounds) else {
        return;
    };
    let side = bounds.size.width.min(bounds.size.height).as_f32();
    let radius = side * DISC / 2.;

    // The halo: concentric discs whose opacity falls off quadratically, the
    // same trick the ambient field uses for its blobs. It is what makes the
    // stage read as diffuse rather than as a disc on flat paint.
    for step in 0..HALO {
        let u = step as f32 / HALO as f32;
        let reach = radius + side * HALO_REACH * u;
        let alpha = (1. - u) * (1. - u) * 0.10;
        let mut builder = PathBuilder::fill();
        circle(&mut builder, center, reach);
        match builder.build() {
            Ok(path) => window.paint_path(path, paint.halo.opacity(alpha)),
            Err(error) => log::warn!("starry: cannot build the halo: {error}"),
        }
    }

    if style.shown() {
        ring(center, side, radius, levels, style, paint, window);
    }
    if vinyl {
        record(center, radius, paint, window);
    }
}

/// The particles: each on its own cycle from the rim outward, fading in and
/// back out over the trip, scattered by a hash so the field is the same
/// every frame without a random source. They are gathered by how bright they
/// are and one path is drawn per shade, so a thousand of them cost a
/// handful of draws rather than a thousand. The whole field carries the
/// music's presence: it fades out as the music stops and back in as it
/// starts, and a field faded to nothing costs nothing at all.
pub(super) fn particles(
    bounds: Bounds<Pixels>,
    motion: Motion,
    particles: usize,
    paint: &Palette,
    window: &mut Window,
) {
    let Motion {
        presence, elapsed, ..
    } = motion;
    if presence <= 0.01 {
        return;
    }
    let Some(center) = center(bounds) else {
        return;
    };
    let side = bounds.size.width.min(bounds.size.height).as_f32();
    let radius = side * DISC / 2.;

    let mut shades: Vec<PathBuilder> = (0..TONES).map(|_| PathBuilder::fill()).collect();
    let mut counts = [0usize; TONES];
    for seed in 0..particles as u32 {
        let first = scatter(seed);
        let second = scatter(seed.wrapping_add(0x9E1));
        let third = scatter(seed.wrapping_add(0x37D));
        let period = 6. + first * 6.;
        let journey = (elapsed / period + second).fract();
        let distance = radius * (1.03 + 0.23 * journey.powf(0.8));
        // A slow orbit in the record's own direction, a little different
        // per particle, so the field circles rather than spins rigidly.
        let heading = -TAU / 4. + TAU * third + elapsed * TAU / SPIN * (0.06 + first * 0.12);
        let alpha = (std::f32::consts::PI * journey).sin() * (0.22 + second * 0.30) * presence;
        let size = side * 0.0025 + third * side * 0.003;
        let shade = ((alpha / STAR * TONES as f32) as usize).min(TONES - 1);
        counts[shade] += 1;
        disc(&mut shades[shade], on(center, distance, heading), size);
    }
    for (shade, builder) in shades.into_iter().enumerate() {
        if counts[shade] == 0 {
            continue;
        }
        let alpha = (shade as f32 + 0.5) / TONES as f32 * STAR;
        match builder.build() {
            Ok(path) => window.paint_path(path, paint.star.opacity(alpha)),
            Err(error) => log::warn!("starry: cannot build the particles: {error}"),
        }
    }
}

/// The ring around the rim, in whatever shape the visualizer is set to: bars
/// stand in a mirrored circle, the wave runs one smooth loop, and `Both` puts
/// the bars inside the wave, cut to its height.
fn ring(
    center: Point<Pixels>,
    side: f32,
    radius: f32,
    levels: &Levels,
    style: VisualizerStyle,
    paint: &Palette,
    window: &mut Window,
) {
    let bands = levels.mixed();
    if bands.is_empty() {
        return;
    }
    let inner = radius + side * RING_GAP;
    let reach = side * RING_REACH;

    match style {
        VisualizerStyle::None => {}
        VisualizerStyle::Bars => bars(center, inner, reach, &bands, |_| f32::MAX, paint, window),
        VisualizerStyle::Wave => wave(center, inner, reach, &bands, paint, window),
        VisualizerStyle::Both => {
            wave(center, inner, reach, &bands, paint, window);
            // Under the wave: a bar standing proud of it is cut back to the
            // wave's own height at that angle, so the two read as one shape
            // with the wave as its edge.
            bars(
                center,
                inner,
                reach,
                &bands,
                |angle| wave_radius(inner, reach, &bands, angle),
                paint,
                window,
            );
        }
    }
}

/// The wave's own radius at `angle`, walked along the same cubics `wave` draws
/// with. The bars stand between the wave's samples, where a spline dips below
/// the points it passes through, so their own heights are not enough to keep
/// them in — they have to be cut to the curve itself.
fn wave_radius(inner: f32, reach: f32, bands: &[f32], angle: f32) -> f32 {
    let count = bands.len();
    let start = -TAU / 4.;
    let step = TAU / count as f32;
    let at = |index: i64| {
        let band = bands[index.rem_euclid(count as i64) as usize];
        let level = RING_FLOOR + band.clamp(0., 1.) * (1. - RING_FLOOR);
        let turned = start + step * (index as f32 + 0.5);
        let radius = inner + reach * level;
        (radius * turned.cos(), radius * turned.sin())
    };

    let walked = (angle - start) / step;
    let segment = walked.floor();
    let t = (walked - segment).clamp(0., 1.);
    let index = segment as i64;
    let (x0, y0) = at(index);
    let (x1, y1) = at(index + 1);
    let (bx, by) = at(index - 1);
    let (ax, ay) = at(index + 2);

    let (c1x, c1y) = (x0 + (x1 - bx) / 6., y0 + (y1 - by) / 6.);
    let (c2x, c2y) = (x1 - (ax - x0) / 6., y1 - (ay - y0) / 6.);
    let u = 1. - t;
    let x = x0 * (u * u * u) + c1x * (3. * u * u * t) + c2x * (3. * u * t * t) + x1 * (t * t * t);
    let y = y0 * (u * u * u) + c1y * (3. * u * u * t) + c2y * (3. * u * t * t) + y1 * (t * t * t);
    (x * x + y * y).sqrt()
}

/// The bars: the mixed bands walked twice around the rim, once and then
/// mirrored, so the circle reads as symmetric. All bars share one stroked
/// path, one subpath each. `ceiling` is the furthest a bar may reach at a
/// given angle, which is what keeps them under the wave when both are drawn.
fn bars(
    center: Point<Pixels>,
    inner: f32,
    reach: f32,
    bands: &[f32],
    ceiling: impl Fn(f32) -> f32,
    paint: &Palette,
    window: &mut Window,
) {
    let count = bands.len();
    let posts = 2 * count;
    let width = (TAU * inner / posts as f32 * 0.5).max(1.2);
    let mut builder = PathBuilder::stroke(px(width));
    for post in 0..posts {
        let band = match post < count {
            true => bands[post],
            false => bands[posts - 1 - post],
        };
        let level = RING_FLOOR + band.clamp(0., 1.) * (1. - RING_FLOOR);
        let angle = -TAU / 4. + TAU * (post as f32 + 0.5) / posts as f32;
        // A hair under the ceiling, so the wave's stroke reads as the edge and
        // the bars as what fills it.
        let tip = (inner + reach * level).min(ceiling(angle) - 0.5);
        builder.move_to(on(center, inner, angle));
        builder.line_to(on(center, tip, angle));
    }
    match builder.build() {
        Ok(path) => window.paint_path(path, paint.ring.opacity(0.55)),
        Err(error) => log::warn!("starry: cannot build the ring: {error}"),
    }
}

/// The wave: two smooth loops through every band — the line itself, and a
/// softer echo that wanders on its own — closed Catmull-Rom splines, the same
/// spline the bottom wave runs, bent around the rim.
fn wave(
    center: Point<Pixels>,
    inner: f32,
    reach: f32,
    bands: &[f32],
    paint: &Palette,
    window: &mut Window,
) {
    let now = spin();
    let mut points = Vec::with_capacity(bands.len());
    let mut echo = Vec::with_capacity(bands.len());
    let count = bands.len();
    for (index, band) in bands.iter().enumerate() {
        let level = RING_FLOOR + band.clamp(0., 1.) * (1. - RING_FLOOR);
        let angle = -TAU / 4. + TAU * (index as f32 + 0.5) / count as f32;
        points.push(on(center, inner + reach * level, angle));
        // The echo is not the wave's shadow. It hears the same music a step
        // around the clock, keeps only a squashed share of its reach, sits a
        // step further out, and breathes on a slow swell of its own — so the
        // two layers part and cross the way ink lines do, never tracking.
        let lagged = bands[(index + count / 8) % count].clamp(0., 1.);
        let level = RING_FLOOR + lagged * (1. - RING_FLOOR) * ECHO_REACH;
        let swell = WAVE_SWELL * (3. * angle + now * 0.7).sin();
        echo.push(on(center, inner + WAVE_ECHO + reach * level + swell, angle));
    }
    // The echo runs under the line, quieter and a hair thinner, so the two
    // read as layers of one printed shape rather than as two waves.
    let mut under = PathBuilder::stroke(px(1.));
    wave_ring(&mut under, &echo);
    match under.build() {
        Ok(path) => window.paint_path(path, paint.ring.opacity(0.22)),
        Err(error) => log::warn!("starry: cannot build the wave's echo: {error}"),
    }
    let mut builder = PathBuilder::stroke(px(1.5));
    wave_ring(&mut builder, &points);
    match builder.build() {
        Ok(path) => window.paint_path(path, paint.ring.opacity(0.55)),
        Err(error) => log::warn!("starry: cannot build the wave: {error}"),
    }
}

/// Appends a closed Catmull-Rom spline through `points`, as cubics.
fn wave_ring(builder: &mut PathBuilder, points: &[Point<Pixels>]) {
    let count = points.len();
    if count < 2 {
        return;
    }
    builder.move_to(points[0]);
    for index in 0..count {
        let from = points[index];
        let to = points[(index + 1) % count];
        let before = points[(index + count - 1) % count];
        let after = points[(index + 2) % count];
        builder.cubic_bezier_to(
            to,
            point(
                from.x + (to.x - before.x) / 6.,
                from.y + (to.y - before.y) / 6.,
            ),
            point(
                to.x - (after.x - from.x) / 6.,
                to.y - (after.y - from.y) / 6.,
            ),
        );
    }
    builder.close();
}
