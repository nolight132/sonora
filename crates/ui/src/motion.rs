use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

use gpui::{
    Animation, AnimationElement, AnimationExt as _, App, ElementId, Hsla, IntoElement, Rgba,
    SharedString, ease_in_out, ease_out_quint,
};
use i18n::t;

const CONTROL: Duration = Duration::from_millis(110);
const QUICK: Duration = Duration::from_millis(120);
const BASE: Duration = Duration::from_millis(200);
const SLOW: Duration = Duration::from_millis(320);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Control,
    Quick,
    Base,
    Slow,
}

impl Motion {
    pub fn span(self) -> Duration {
        match self {
            Self::Control => CONTROL,
            Self::Quick => QUICK.mul_f32(pace().scale()),
            Self::Base => BASE.mul_f32(pace().scale()),
            Self::Slow => SLOW.mul_f32(pace().scale()),
        }
    }

    pub fn animation(self) -> Animation {
        let animation = Animation::new(self.span());

        match self {
            Self::Control | Self::Base => animation.with_easing(ease_in_out),
            Self::Quick | Self::Slow => animation.with_easing(ease_out_quint()),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pace {
    Slow,
    #[default]
    Base,
    Quick,
}

impl Pace {
    pub const ALL: [Self; 3] = [Self::Slow, Self::Base, Self::Quick];

    pub fn id(self) -> &'static str {
        match self {
            Self::Slow => "slow",
            Self::Base => "base",
            Self::Quick => "quick",
        }
    }

    pub fn label(self) -> SharedString {
        match self {
            Self::Slow => t!("pace-slow"),
            Self::Base => t!("pace-base"),
            Self::Quick => t!("pace-quick"),
        }
    }

    pub fn from_id(id: &str) -> Self {
        match id {
            "slow" => Self::Slow,
            "quick" => Self::Quick,
            _ => Self::Base,
        }
    }

    fn scale(self) -> f32 {
        match self {
            Self::Slow => 1.6,
            Self::Base => 1.,
            Self::Quick => 0.6,
        }
    }
}

pub fn mix(from: Hsla, to: Hsla, t: f32) -> Hsla {
    let (from, to) = (Rgba::from(from), Rgba::from(to));
    let step = t.clamp(0., 1.);
    let blend = |a: f32, b: f32| a + (b - a) * step;

    Rgba {
        r: blend(from.r, to.r),
        g: blend(from.g, to.g),
        b: blend(from.b, to.b),
        a: blend(from.a, to.a),
    }
    .into()
}

pub fn ease_out_expo(progress: f32) -> f32 {
    cubic_bezier(progress.clamp(0., 1.), 0.16, 1., 0.3, 1.)
}

pub fn ease_out_quad(progress: f32) -> f32 {
    cubic_bezier(progress.clamp(0., 1.), 0.5, 1., 0.89, 1.)
}

pub fn ease_out_cubic(progress: f32) -> f32 {
    cubic_bezier(progress.clamp(0., 1.), 0.33, 1., 0.68, 1.)
}

pub fn ease_in_out_expo(progress: f32) -> f32 {
    cubic_bezier(progress.clamp(0., 1.), 0.87, 0., 0.13, 1.)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sweep {
    Steady,
    Gentle,
    #[default]
    Smooth,
    Snappy,
    Glide,
}

impl Sweep {
    pub const ALL: [Self; 5] = [
        Self::Steady,
        Self::Gentle,
        Self::Smooth,
        Self::Snappy,
        Self::Glide,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Steady => "steady",
            Self::Gentle => "gentle",
            Self::Smooth => "smooth",
            Self::Snappy => "snappy",
            Self::Glide => "glide",
        }
    }

    pub fn label(self) -> SharedString {
        match self {
            Self::Steady => t!("sweep-steady"),
            Self::Gentle => t!("sweep-gentle"),
            Self::Smooth => t!("sweep-smooth"),
            Self::Snappy => t!("sweep-snappy"),
            Self::Glide => t!("sweep-glide"),
        }
    }

    pub fn from_id(id: &str) -> Self {
        match id {
            "steady" => Self::Steady,
            "gentle" => Self::Gentle,
            "snappy" => Self::Snappy,
            "glide" => Self::Glide,
            _ => Self::Smooth,
        }
    }

    pub fn stretch(self) -> f32 {
        match self {
            Self::Steady => 1.,
            Self::Gentle => 1.2,
            Self::Smooth => 1.4,
            Self::Snappy => 1.8,
            Self::Glide => 1.25,
        }
    }

    pub fn ease(self, progress: f32) -> f32 {
        match self {
            Self::Steady => progress.clamp(0., 1.),
            Self::Gentle => ease_out_quad(progress),
            Self::Smooth => ease_out_cubic(progress),
            Self::Snappy => ease_out_expo(progress),
            Self::Glide => ease_in_out_expo(progress),
        }
    }
}

fn cubic_bezier(progress: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    if progress == 0. || progress == 1. {
        return progress;
    }

    let axis = |t: f32, a: f32, b: f32| {
        let remaining = 1. - t;
        3. * remaining * remaining * t * a + 3. * remaining * t * t * b + t * t * t
    };
    let slope = |t: f32| {
        let remaining = 1. - t;
        3. * remaining * remaining * x1 + 6. * remaining * t * (x2 - x1) + 3. * t * t * (1. - x2)
    };

    let mut parameter = progress;
    for _ in 0..6 {
        let gradient = slope(parameter);
        if gradient.abs() <= f32::EPSILON {
            break;
        }
        parameter = (parameter - (axis(parameter, x1, x2) - progress) / gradient).clamp(0., 1.);
    }
    axis(parameter, y1, y2)
}

fn pace() -> Pace {
    match PACE.load(Ordering::Relaxed) {
        0 => Pace::Slow,
        2 => Pace::Quick,
        _ => Pace::Base,
    }
}

static PACE: AtomicU8 = AtomicU8::new(1);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Stillness {
    #[default]
    System,
    Always,
    Never,
}

impl Stillness {
    pub const ALL: [Self; 3] = [Self::System, Self::Always, Self::Never];

    pub fn id(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Always => "always",
            Self::Never => "never",
        }
    }

    pub fn label(self) -> SharedString {
        match self {
            Self::System => t!("motion-system"),
            Self::Always => t!("motion-always"),
            Self::Never => t!("motion-never"),
        }
    }

    pub fn from_id(id: &str) -> Self {
        match id {
            "always" => Self::Always,
            "never" => Self::Never,
            _ => Self::System,
        }
    }

    pub fn still(self) -> bool {
        match self {
            Self::System => system_still(),
            Self::Always => true,
            Self::Never => false,
        }
    }
}

pub trait Motioned: Sized {
    fn motion(
        self,
        id: impl Into<ElementId>,
        motion: Motion,
        animator: impl Fn(Self, f32) -> Self + 'static,
    ) -> AnimationElement<Self>;
}

impl<E: IntoElement + 'static> Motioned for E {
    fn motion(
        self,
        id: impl Into<ElementId>,
        motion: Motion,
        animator: impl Fn(Self, f32) -> Self + 'static,
    ) -> AnimationElement<Self> {
        self.with_animation(id, motion.animation(), animator)
    }
}

pub fn apply(stillness: Stillness, pace: Pace, cx: &mut App) {
    PACE.store(
        match pace {
            Pace::Slow => 0,
            Pace::Base => 1,
            Pace::Quick => 2,
        },
        Ordering::Relaxed,
    );
    cx.set_reduce_motion(stillness.still());
}

pub fn animates(cx: &App) -> bool {
    !cx.reduce_motion()
}

fn system_still() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expo_easing_has_css_endpoints_and_shape() {
        assert_eq!(ease_out_expo(0.), 0.);
        assert_eq!(ease_out_expo(1.), 1.);
        assert!(ease_out_expo(0.25) > 0.8);
        assert!(ease_out_expo(0.5) > 0.97);

        let mut previous = 0.;
        for step in 1..=20 {
            let value = ease_out_expo(step as f32 / 20.);
            assert!(value >= previous);
            previous = value;
        }
    }

    #[test]
    fn quad_easing_matches_css_curve() {
        assert_eq!(ease_out_quad(0.), 0.);
        assert_eq!(ease_out_quad(1.), 1.);
        assert!(ease_out_quad(0.25) > 0.4);
        assert!(ease_out_quad(0.5) > 0.7);
    }
}
