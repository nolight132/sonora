//! The fullscreen stage: the cover, and everything the stage plays around it.
//!
//! Four ways of staging the cover share this module — bare, starry, on a
//! turning record, and slid out of its sleeve — so what they have in common
//! lives here and what only one of them does lives beside it: [`field`] paints
//! the halo, the spectrum ring and the drifting particles, [`record`] the
//! vinyl and the sheen that sweeps it, [`sleeve`] the square cover with its
//! record behind, [`paint`] the colours and [`clock`] the two clocks the stage
//! runs on.

mod clock;
mod field;
mod geometry;
mod paint;
mod record;
mod sleeve;

use std::sync::OnceLock;
use std::time::Instant;

use gpui::prelude::*;
use gpui::{Bounds, Div, Pixels, SharedString, Window, canvas, div, px};
use ui::{Artwork, Levels, Theme, VisualizerStyle};

pub(crate) use clock::{Clock, Drive, Pose};

use paint::{Palette, palette};

/// One turn of the record, in seconds. A real 33⅓ single spins in 1.8, which
/// at this size reads as frantic; the stage settles for a slow, visible turn.
const SPIN: f32 = 18.;
/// The record's diameter, as a share of the square the stage is given. The
/// rest of the square is the room the ring, the halo and the particles live in,
/// and it has to end inside the square: the raster layer clips at its edge.
const DISC: f32 = 0.78;
/// The cover's diameter, as a share of the record's: seven of cover to three of
/// vinyl, so the art reads as the label and the record as its rim.
const LABEL: f32 = 0.7;
/// How many straight segments stand in for a smooth circle.
const SEGMENTS: usize = 72;

/// Where the stage's clock started, so the angle carries over between visits
/// to fullscreen instead of resetting to the same pose every time.
pub(crate) fn spin() -> f32 {
    static STARTED: OnceLock<Instant> = OnceLock::new();
    STARTED.get_or_init(Instant::now).elapsed().as_secs_f32()
}

/// What the stage is asked to show: which way the cover is staged, the
/// spectrum the ring reads and the shape it reads them in, how much drifts off
/// the rim, how far the record has turned and for how long, and the theme the
/// stage dresses in. Two clocks on purpose: the record and everything riding
/// it turn only while sound plays, while the particles keep to the wall clock
/// whether or not it does.
pub(crate) struct Stage {
    pub(crate) levels: Levels,
    pub(crate) style: VisualizerStyle,
    pub(crate) particles: usize,
    /// The way the cover is staged: bare, starry, on a record, in its sleeve.
    pub(crate) layout: ui::StageStyle,
    /// Seconds on the wall clock, driving the particles' drift.
    pub(crate) elapsed: f32,
    /// Seconds the music has been playing, driving the record's turn.
    pub(crate) turn: f32,
    /// How much of the particle field is present, easing with the music.
    pub(crate) presence: f32,
    pub(crate) theme: Theme,
}

/// Everything on the stage that moves, and by how much. The record turns on
/// the music's own clock; the field is as present as the music has left it;
/// the particles drift on the wall clock, whatever the music is doing, and
/// fade with the field.
#[derive(Clone, Copy)]
struct Motion {
    /// Seconds the music has been playing.
    turn: f32,
    /// How much of the particle field is there, from none to all of it.
    presence: f32,
    /// Seconds since the stage was first drawn.
    elapsed: f32,
}

/// The whole stage, sized to `side`. The sleeve stages the cover on bare
/// paint with the record behind it; the rest halo the cover and read the
/// spectrum, staging the record with everything that sweeps across it only
/// when a record is asked for.
pub(crate) fn stage(
    side: Pixels,
    label: Option<impl Into<SharedString>>,
    waiting: bool,
    stage: Stage,
) -> Div {
    let Stage {
        levels,
        style,
        particles,
        layout,
        elapsed,
        turn,
        presence,
        theme,
    } = stage;
    let paint = palette(&theme);
    let side = side.as_f32();
    // Resolved once so both the sleeve's cover and its record label can carry
    // the same art.
    let label: Option<SharedString> = label.map(Into::into);

    // The sleeve is a different composition: the cover square with the record
    // peeking out behind it, no field around either.
    if layout == ui::StageStyle::Sleeve {
        return sleeve::sleeve(side, label, waiting, turn, &paint, theme.radius * 2.);
    }
    let vinyl = layout.turned();

    div()
        .relative()
        .size(px(side))
        .child(
            canvas(move |_, _, _| {}, {
                let levels = levels.clone();
                move |bounds, _, window, _| {
                    field::field(bounds, &levels, style, vinyl, &paint, window)
                }
            })
            .absolute()
            .inset_0(),
        )
        .child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .child({
                    // Seven of cover to three of vinyl, with the record or
                    // without it: the stage's size is not the record's to give.
                    let art = Artwork::new(label).size(px(side * DISC * LABEL));
                    // The cover rides the record: the renderer cannot turn an
                    // image, so the turn is cut from its pixels and handed back
                    // as a frame of its own, once per degree.
                    match vinyl {
                        true => art.spin(turn / SPIN).circle().soft(waiting),
                        false => art.circle().soft(waiting),
                    }
                }),
        )
        .child(
            canvas(move |_, _, _| {}, {
                let motion = Motion {
                    turn,
                    presence,
                    elapsed,
                };
                move |bounds, _, window, _| shine(bounds, motion, particles, vinyl, &paint, window)
            })
            .absolute()
            .inset_0(),
        )
}

/// Everything that sweeps over the label: the sheen and its groove markers
/// turning only while the music does, and the particles as present as the
/// music has left them.
fn shine(
    bounds: Bounds<Pixels>,
    motion: Motion,
    particles: usize,
    vinyl: bool,
    paint: &Palette,
    window: &mut Window,
) {
    if vinyl {
        record::sheen(bounds, motion.turn, paint, window);
    }
    field::particles(bounds, motion, particles, paint, window);
}
