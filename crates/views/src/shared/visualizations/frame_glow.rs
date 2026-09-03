use std::time::Instant;

use gpui::prelude::*;
use gpui::{App, Hsla, Pixels, Window, div, px};
use music::Pulse;
use state::{Playback, PlaybackState, Sonora};
use ui::motion::animates;

/// Master intensity applied to blended strength. `1.0` is the designed response.
const LEVEL: f32 = 1.;

/// Strength weight in glow blur radius, in pixels at full signal.
const GLOW_BLUR_SIGNAL: f32 = 50.;

/// Upper bound on glow blur radius, in pixels.
const GLOW_RADIUS_MAX: f32 = 20.;

/// Baseline opacity for the glow wash.
///
/// This is the maximum opacity that the glow wash will reach when the strength is at its maximum.
const GLOW_BASELINE_OPACITY: f32 = 0.8;

/// Exponential rise rate for chased pulse values, per second.
const ATTACK: f32 = 24.;

/// Exponential fall rate for chased pulse values, per second.
const RELEASE: f32 = 7.;

/// Frame delta cap passed into the smoothing step, in seconds.
const CHASE_STEP_MAX: f32 = 0.08;

/// Minimum blended strength before the glow layer renders.
const STRENGTH_MIN: f32 = 0.006;

/// Bass weight in the blended glow strength (`STRENGTH_BASS * bass + (1 - STRENGTH_BASS) * mids_highs`),
/// where `mids_highs` is the max of `mids` and `highs`.
/// Raise toward `1.0` to follow kick and sub; lower toward `0.0` to follow mids and highs; `0.5` splits evenly.
const STRENGTH_BASS: f32 = 0.5;

/// Multiplier for the mids-and-highs signal before it is added to the bass signal.
const MIDS_HIGHS_MULTIPLIER: f32 = 3.0;

/// Glow wash opacity before strength contributes.
const GLOW_ALPHA_BASE: f32 = 0.08;

/// Strength multiplier added to [`GLOW_ALPHA_BASE`] for glow wash opacity.
const GLOW_ALPHA_SIGNAL: f32 = 2.5;

/// RMS weight in glow blur radius, in pixels at full signal.
const GLOW_BLUR_RMS: f32 = 10.;

/// Strength weight in glow layer scale above 1.0.
const GLOW_SCALE_SIGNAL: f32 = 0.35;

/// Input gain in the `1 - e^(-x·k)` curve applied to each pulse band.
const CURVE_GAIN: f32 = 0.5;

/// Corner-radius weight when deriving the minimum glow blur from artwork geometry.
const RIM_BLUR_CORNER: f32 = 0.9;

/// Base padding added to the rim blur floor, in pixels.
const RIM_BLUR_BASE: f32 = 2.;

/// Minimum glow blur regardless of artwork size, in pixels.
const RIM_BLUR_FLOOR: f32 = 3.;

/// Corner-radius weight when deriving the minimum glow scale from artwork geometry.
const RIM_SCALE_CORNER: f32 = 2.;

/// Minimum glow scale above 1.0 for small artwork, as a fraction of side length.
const RIM_SCALE_FLOOR: f32 = 0.012;

/// Colour of the glow wash.
const GLOW_COLOUR: Hsla = Hsla {
    h: 0.,
    s: 0.18,
    l: 0.78,
    a: GLOW_BASELINE_OPACITY,
};

/// Produces an artwork glow that follows the current audio pulse.
pub(crate) struct FrameGlow {
    /// Smoothed pulse carried between rendered frames.
    chased: Pulse,
    /// Time when the pulse was last advanced.
    last: Option<Instant>,
}

impl FrameGlow {
    /// Creates an idle frame glow with no accumulated pulse.
    pub(crate) fn new() -> Self {
        Self {
            chased: Pulse::default(),
            last: None,
        }
    }

    /// Advances the audio response and returns the current glow layer.
    ///
    /// # Arguments
    ///
    /// - `size`: Side length of the square artwork.
    /// - `corner`: Artwork corner radius.
    /// - `playback`: Source of playback state and visualizer pulses.
    /// - `window`: Window to schedule while the glow is active or fading.
    /// - `cx`: Application context used to read visualization settings.
    pub(crate) fn sync(
        &mut self,
        size: Pixels,
        corner: Pixels,
        playback: &Playback,
        window: &mut Window,
        cx: &App,
    ) -> impl IntoElement {
        let settings = Sonora::global(cx).settings.read(cx);
        let allowed = settings.visualization() && animates(cx);
        let playing = *playback.state() == PlaybackState::Playing;
        let target = match allowed && playing {
            true => playback.pulse(),
            false => Pulse::default(),
        };

        self.smooth(target);
        let shaped = shaped(self.chased);
        let strength = strength(&shaped) * LEVEL;
        if allowed && (playing || strength > STRENGTH_MIN) {
            window.request_animation_frame();
        }

        let rim = rim(size, corner);
        let glow = Hsla {
            a: ((GLOW_ALPHA_BASE + strength * GLOW_ALPHA_SIGNAL) * GLOW_COLOUR.a)
                .clamp(0., GLOW_COLOUR.a),
            ..GLOW_COLOUR
        };
        let glow_blur = px((strength * GLOW_BLUR_SIGNAL + shaped.rms * GLOW_BLUR_RMS)
            .min(GLOW_RADIUS_MAX)
            .max(rim.min_blur.as_f32()));
        let glow_scale = rim.min_scale.max(1. + strength * GLOW_SCALE_SIGNAL);
        div().absolute().top_0().left_0().size_full().when(
            allowed && strength > STRENGTH_MIN,
            |this| {
                this.child(
                    div()
                        .absolute()
                        .inset_0()
                        .rounded(rim.corner)
                        .bg(glow)
                        .blur(glow_blur)
                        .layer_scale(glow_scale),
                )
            },
        )
    }

    /// Moves the accumulated pulse toward a target using asymmetric response rates (Attack and Release).
    ///
    /// # Arguments
    ///
    /// - `target`: Latest pulse to chase, or a silent pulse while inactive.
    fn smooth(&mut self, target: Pulse) {
        let now = Instant::now();
        let step = match self.last.replace(now) {
            Some(last) => last.elapsed().as_secs_f32().clamp(0., CHASE_STEP_MAX),
            None => 1.,
        };
        let attack = 1. - (-step * ATTACK).exp();
        let release = 1. - (-step * RELEASE).exp();
        self.chased = Pulse {
            peak: follow(self.chased.peak, target.peak, attack, release),
            rms: follow(self.chased.rms, target.rms, attack, release),
            bass: follow(self.chased.bass, target.bass, attack, release),
            mids: follow(self.chased.mids, target.mids, attack, release),
            highs: follow(self.chased.highs, target.highs, attack, release),
        };
    }
}

/// Moves one pulse value toward its target.
///
/// # Arguments
///
/// - `current`: Previously accumulated value.
/// - `target`: Latest sampled value.
/// - `attack`: Interpolation rate used while the value rises.
/// - `release`: Interpolation rate used while the value falls.
fn follow(current: f32, target: f32, attack: f32, release: f32) -> f32 {
    let rate = match target > current {
        true => attack,
        false => release,
    };
    current + (target - current) * rate
}

/// Applies the response curve to every component of a pulse.
///
/// # Arguments
///
/// - `pulse`: Smoothed pulse to shape.
fn shaped(pulse: Pulse) -> Pulse {
    Pulse {
        peak: curve(pulse.peak),
        rms: curve(pulse.rms),
        bass: curve(pulse.bass),
        mids: curve(pulse.mids),
        highs: curve(pulse.highs),
    }
}

/// Compresses an input value into the normalized visual response.
///
/// # Arguments
///
/// - `value`: Pulse component to shape.
fn curve(value: f32) -> f32 {
    (1. - (-value * CURVE_GAIN).exp()).clamp(0., 1.)
}

/// Blends the shaped frequency bands into the glow strength.
///
/// # Arguments
///
/// - `pulse`: Shaped pulse containing the frequency-band levels.
fn strength(pulse: &Pulse) -> f32 {
    STRENGTH_BASS * pulse.bass + (1. - STRENGTH_BASS) * mids_highs(pulse)
}

/// Returns the weighted larger value from the middle and high bands.
///
/// # Arguments
///
/// - `pulse`: Shaped pulse containing the frequency-band levels.
fn mids_highs(pulse: &Pulse) -> f32 {
    pulse.mids.max(pulse.highs) * MIDS_HIGHS_MULTIPLIER
}

/// Minimum geometry needed to reveal the glow beyond the artwork rim.
struct Rim {
    /// Corner radius shared with the artwork.
    corner: Pixels,
    /// Smallest blur that clears the rounded edge.
    min_blur: Pixels,
    /// Smallest scale that clears the rounded edge.
    min_scale: f32,
}

/// Derives minimum glow geometry from the artwork dimensions.
///
/// # Arguments
///
/// - `size`: Side length of the square artwork.
/// - `corner`: Artwork corner radius.
fn rim(size: Pixels, corner: Pixels) -> Rim {
    let side = size.as_f32().max(1.);
    let corner = corner.as_f32().max(0.);
    let min_blur = (corner * RIM_BLUR_CORNER + RIM_BLUR_BASE).max(RIM_BLUR_FLOOR);
    let min_scale = 1. + (corner * RIM_SCALE_CORNER / side).max(RIM_SCALE_FLOOR);
    Rim {
        corner: px(corner),
        min_blur: px(min_blur),
        min_scale,
    }
}
