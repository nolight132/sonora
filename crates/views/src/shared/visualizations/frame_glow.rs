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

/// Strength weight in glow wash saturation.
const GLOW_SAT_SIGNAL: f32 = 0.65;

/// RMS weight in glow wash saturation.
const GLOW_SAT_RMS: f32 = 0.12;

/// Strength weight in glow wash lightness.
const GLOW_LIGHT_SIGNAL: f32 = 0.22;

/// RMS weight in glow wash lightness.
const GLOW_LIGHT_RMS: f32 = 0.06;

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

/// Colour of the glow wash - for now just white.
const GLOW_COLOUR: Hsla = Hsla {
    h: 0.,
    s: 0.,
    l: 1.,
    a: GLOW_BASELINE_OPACITY,
};

pub(crate) struct FrameGlow {
    chased: Pulse,
    last: Option<Instant>,
    frame: Option<Frame>,
}

struct Frame {
    show: bool,
    rim: Rim,
    strength: f32,
    glow: Hsla,
    glow_blur: Pixels,
    glow_scale: f32,
}

impl FrameGlow {
    pub(crate) fn new() -> Self {
        Self {
            chased: Pulse::default(),
            last: None,
            frame: None,
        }
    }

    pub(crate) fn sync(
        &mut self,
        size: Pixels,
        corner: Pixels,
        playback: &Playback,
        window: &mut Window,
        cx: &App,
    ) {
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
        let glow = wash(
            GLOW_COLOUR,
            strength * GLOW_SAT_SIGNAL + shaped.rms * GLOW_SAT_RMS,
            strength * GLOW_LIGHT_SIGNAL + shaped.rms * GLOW_LIGHT_RMS,
            GLOW_ALPHA_BASE + strength * GLOW_ALPHA_SIGNAL,
        );
        let glow_blur = px(
            (strength * GLOW_BLUR_SIGNAL + shaped.rms * GLOW_BLUR_RMS)
                .min(GLOW_RADIUS_MAX)
                .max(rim.min_blur.as_f32()),
        );
        let glow_scale = rim.min_scale.max(1. + strength * GLOW_SCALE_SIGNAL);

        self.frame = Some(Frame {
            show: allowed && strength > STRENGTH_MIN,
            rim,
            strength,
            glow,
            glow_blur,
            glow_scale,
        });
    }

    pub(crate) fn glow(&self) -> impl IntoElement {
        let Some(frame) = &self.frame else {
            return div();
        };
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .when(frame.show && frame.strength > STRENGTH_MIN, |this| {
                this.child(
                    div()
                        .absolute()
                        .inset_0()
                        .rounded(frame.rim.corner)
                        .bg(frame.glow)
                        .blur(frame.glow_blur)
                        .layer_scale(frame.glow_scale),
                )
            })
    }
}

impl FrameGlow {
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

fn follow(current: f32, target: f32, attack: f32, release: f32) -> f32 {
    let rate = match target > current {
        true => attack,
        false => release,
    };
    current + (target - current) * rate
}

fn shaped(pulse: Pulse) -> Pulse {
    Pulse {
        peak: curve(pulse.peak),
        rms: curve(pulse.rms),
        bass: curve(pulse.bass),
        mids: curve(pulse.mids),
        highs: curve(pulse.highs),
    }
}

fn curve(value: f32) -> f32 {
    (1. - (-value * CURVE_GAIN).exp()).clamp(0., 1.)
}

fn strength(pulse: &Pulse) -> f32 {
    STRENGTH_BASS * pulse.bass + (1. - STRENGTH_BASS) * mids_highs(pulse)
}

fn mids_highs(pulse: &Pulse) -> f32 {
    pulse.mids.max(pulse.highs) * MIDS_HIGHS_MULTIPLIER
}

struct Rim {
    corner: Pixels,
    min_blur: Pixels,
    min_scale: f32,
}

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

fn wash(base: Hsla, sat: f32, light: f32, alpha: f32) -> Hsla {
    Hsla {
        h: base.h,
        s: (base.s * (0.72 + sat)).clamp(0.18, 1.),
        l: (base.l + light).clamp(0.2, 0.78),
        a: (alpha * base.a).clamp(0., base.a),
    }
}
