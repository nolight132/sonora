use std::time::Instant;

use gpui::prelude::*;
use gpui::{App, Hsla, Pixels, Window, div, px};
use music::Spectrum;
use state::{Playback, PlaybackState, Sonora};
use ui::ActiveTheme as _;
use ui::motion::animates;

/// Master intensity applied to blended strength. `1.0` is the designed response.
const LEVEL: f32 = 1.;

/// Upper bound on glow blur radius, as a fraction of the artwork side.
const GLOW_BLUR_MAX: f32 = 0.08;

/// Maximum opacity the glow wash reaches at full strength.
const GLOW_OPACITY: f32 = 0.5;

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

/// RMS weight in glow blur radius, as a fraction of the artwork side at full signal.
const GLOW_BLUR_RMS: f32 = 0.03;

/// Strength weight in glow layer scale above 1.0.
const GLOW_SCALE_SIGNAL: f32 = 0.12;

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

/// Soft knee applied to raw FFT band means, same curve the old pulse used.
const SQUASH: f32 = 0.08;

#[derive(Clone, Copy, Default)]
struct Pulse {
    peak: f32,
    rms: f32,
    bass: f32,
    mids: f32,
    highs: f32,
}

/// Produces an artwork glow that follows the current audio spectrum.
pub(crate) struct FrameGlow {
    /// Smoothed pulse carried between rendered frames.
    chased: Pulse,
    /// Time when the pulse was last advanced.
    last: Option<Instant>,
    /// Master intensity applied to the glow. `1.0` is the designed response.
    level: f32,
}

impl FrameGlow {
    /// Creates an idle frame glow with no accumulated pulse.
    pub(crate) fn new(level: f32) -> Self {
        Self {
            chased: Pulse::default(),
            last: None,
            level,
        }
    }

    /// Advances the audio response and returns the current glow layer.
    ///
    /// # Arguments
    ///
    /// - `size`: Side length of the square artwork.
    /// - `corner`: Artwork corner radius.
    /// - `scale`: Presentation scale the artwork is drawn at, so the glow keeps pace with it.
    /// - `playback`: Source of playback state and spectrum bands.
    /// - `window`: Window to schedule while the glow is active or fading.
    /// - `cx`: Application context used to read the visualizer setting.
    pub(crate) fn sync(
        &mut self,
        size: Pixels,
        corner: Pixels,
        scale: f32,
        playback: &Playback,
        window: &mut Window,
        cx: &App,
    ) -> impl IntoElement {
        let settings = Sonora::global(cx).settings.read(cx);
        let allowed = settings.visualizer() && animates(cx);
        let playing = *playback.state() == PlaybackState::Playing;
        let target = match allowed && playing {
            true => playback
                .spectrum()
                .map(|spectrum| pulse(&spectrum))
                .unwrap_or_default(),
            false => Pulse::default(),
        };

        self.smooth(target);
        let shaped = shaped(self.chased);
        let strength = strength(&shaped) * self.level;
        if allowed && (playing || strength > STRENGTH_MIN) {
            window.request_animation_frame();
        }

        let rim = rim(size, corner);
        let side = size.as_f32().max(1.);
        let opacity = GLOW_OPACITY * self.level;
        let glow = Hsla {
            a: ((GLOW_ALPHA_BASE + strength * GLOW_ALPHA_SIGNAL) * opacity).clamp(0., opacity),
            ..cx.theme().primary
        };
        let glow_blur = px(((strength * LEVEL + shaped.rms * GLOW_BLUR_RMS) * side)
            .min(GLOW_BLUR_MAX * side)
            .max(rim.min_blur.as_f32()));
        let glow_scale = rim.min_scale.max(1. + strength * GLOW_SCALE_SIGNAL) * scale;
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

fn pulse(spectrum: &Spectrum) -> Pulse {
    Pulse {
        peak: spectrum.peak(),
        rms: spectrum.rms(),
        bass: squash(spectrum.bass()),
        mids: squash(spectrum.mids()),
        highs: squash(spectrum.highs()),
    }
}

fn squash(magnitude: f32) -> f32 {
    (magnitude / (magnitude + SQUASH)).clamp(0., 1.)
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
