use std::time::Instant;

use gpui::prelude::*;
use gpui::{App, Hsla, Pixels, Window, div, px};
use music::Pulse;
use state::{Playback, PlaybackState, Sonora};
use ui::ActiveTheme as _;
use ui::motion::animates;

/// Exponential rise rate for chased pulse values, per second.
const ATTACK: f32 = 24.;

/// Exponential fall rate for chased pulse values, per second.
const RELEASE: f32 = 7.;

/// Frame delta cap passed into the smoothing step, in seconds.
const CHASE_STEP_MAX: f32 = 0.08;

/// Minimum blended strength before the glow layer renders.
const STRENGTH_MIN: f32 = 0.006;

/// Bass weight in the blended glow strength (`STRENGTH_BASS * bass + (1 - STRENGTH_BASS) * upper`),
/// where `upper` is the max of mids (`body`) and highs (`air`).
/// Raise toward `1.0` to follow kick and sub; lower toward `0.0` to follow mids and highs; `0.5` splits evenly.
const STRENGTH_BASS: f32 = 0.5;

const UPPER_MULITPLIER: f32 = 3.0;

/// Upper opacity clamp for the signal-driven glow wash.
const GLOW_ALPHA_MAX: f32 = 0.85;

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

/// Strength weight in glow blur radius, in pixels at full signal.
const GLOW_BLUR_SIGNAL: f32 = 75.;

/// RMS weight in glow blur radius, in pixels at full signal.
const GLOW_BLUR_RMS: f32 = 10.;

/// Strength weight in glow layer scale above 1.0.
const GLOW_SCALE_SIGNAL: f32 = 0.35;

/// Input gain in the `1 - e^(-x·k)` curve applied to each pulse band.
const CURVE_GAIN: f32 = 5.;

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

/// Minimum seconds between stderr debug lines from each glow instance.
const LOG_INTERVAL: f32 = 0.5;

pub(crate) struct VisualizationGlow {
    chased: Pulse,
    last: Option<Instant>,
    logged: Option<Instant>,
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

impl VisualizationGlow {
    pub(crate) fn new() -> Self {
        Self {
            chased: Pulse::default(),
            last: None,
            logged: None,
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
        let strength = strength(&shaped);
        if allowed && (playing || strength > STRENGTH_MIN) {
            window.request_animation_frame();
        }

        let tint = cx.theme().tint.unwrap_or(cx.theme().primary);
        let rim = rim(size, corner);
        let glow = wash(
            tint,
            strength * GLOW_SAT_SIGNAL + shaped.rms * GLOW_SAT_RMS,
            strength * GLOW_LIGHT_SIGNAL + shaped.rms * GLOW_LIGHT_RMS,
            GLOW_ALPHA_BASE + strength * GLOW_ALPHA_SIGNAL,
        );
        let glow_blur = rim
            .min_blur
            .max(px(strength * GLOW_BLUR_SIGNAL + shaped.rms * GLOW_BLUR_RMS));
        let glow_scale = rim.min_scale.max(1. + strength * GLOW_SCALE_SIGNAL);

        self.log(
            allowed,
            playing,
            allowed && (playing || strength > STRENGTH_MIN),
            target,
            self.chased,
            shaped,
            strength,
            rim.min_blur,
            rim.min_scale,
            glow.a,
            glow_blur,
            glow_scale,
        );

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

impl VisualizationGlow {
    fn log(
        &mut self,
        allowed: bool,
        playing: bool,
        animate: bool,
        target: Pulse,
        chased: Pulse,
        shaped: Pulse,
        strength: f32,
        min_blur: Pixels,
        min_scale: f32,
        glow_a: f32,
        glow_blur: Pixels,
        glow_scale: f32,
    ) {
        let due = match self.logged {
            Some(last) => last.elapsed().as_secs_f32() >= LOG_INTERVAL,
            None => true,
        };
        if !due {
            return;
        }
        self.logged = Some(Instant::now());

        eprintln!(
            "visualization-glow: allowed={allowed} playing={playing} animate={animate} \
             strength={strength:.3} bass={:.3} upper={:.3} \
             min_blur={min_blur} min_scale={min_scale:.3} \
             target=({:.3},{:.3},{:.3},{:.3},{:.3}) \
             chased=({:.3},{:.3},{:.3},{:.3},{:.3}) \
             shaped=({:.3},{:.3},{:.3},{:.3},{:.3}) \
             glow_alpha={glow_a:.3} glow_blur={glow_blur} glow_scale={glow_scale:.3}",
            shaped.bass,
            upper(&shaped),
            target.peak,
            target.rms,
            target.bass,
            target.body,
            target.air,
            chased.peak,
            chased.rms,
            chased.bass,
            chased.body,
            chased.air,
            shaped.peak,
            shaped.rms,
            shaped.bass,
            shaped.body,
            shaped.air,
        );
    }

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
            body: follow(self.chased.body, target.body, attack, release),
            air: follow(self.chased.air, target.air, attack, release),
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
        body: curve(pulse.body),
        air: curve(pulse.air),
    }
}

fn curve(value: f32) -> f32 {
    (1. - (-value * CURVE_GAIN).exp()).clamp(0., 1.)
}

fn strength(pulse: &Pulse) -> f32 {
    STRENGTH_BASS * pulse.bass + (1. - STRENGTH_BASS) * upper(pulse)
}

fn upper(pulse: &Pulse) -> f32 {
    pulse.body.max(pulse.air) * UPPER_MULITPLIER
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
        a: alpha.clamp(0., GLOW_ALPHA_MAX),
    }
}
