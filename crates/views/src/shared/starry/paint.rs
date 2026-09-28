use gpui::Hsla;
use ui::Theme;

/// How bright the sheen gets. The stage's sheen is a wash of white over the
/// turning vinyl, so this is an alpha and not a colour: the hue belongs to the
/// theme, and only how much of it is let through is the sheen's to say.
pub(super) const SHEEN: f32 = 0.05;

/// Every colour the stage paints with, resolved once from the theme.
#[derive(Clone, Copy)]
pub(super) struct Palette {
    pub(super) halo: Hsla,
    pub(super) disc: Hsla,
    pub(super) groove: Hsla,
    pub(super) groove_dark: Hsla,
    pub(super) rim: Hsla,
    pub(super) ring: Hsla,
    pub(super) star: Hsla,
    pub(super) sheen: Hsla,
    pub(super) marker: Hsla,
    pub(super) label_edge: Hsla,
    /// The sleeve's record: a pastel pressing in the theme's own hue.
    pub(super) record: Hsla,
    pub(super) record_groove: Hsla,
    pub(super) record_edge: Hsla,
    pub(super) hole: Hsla,
}

/// The record dresses in the theme's own hue at a whisper: a dark disc with a
/// trace of it, lighter and darker grooves over it, and everything that glows
/// — halo, ring, particles — in the hue itself. On a light theme the disc cuts
/// the hue deeper and the glows darken, so the stage sits with the adaptive
/// theme instead of washing out against a bright background.
pub(super) fn palette(theme: &Theme) -> Palette {
    let hue = theme.primary;
    let dark = theme.background.l < 0.5;
    let neutral = |l: f32, a: f32| Hsla {
        h: hue.h,
        s: 0.08,
        l,
        a,
    };

    let disc = Hsla {
        h: hue.h,
        s: match dark {
            true => (hue.s * 0.25).clamp(0.02, 0.28),
            false => (hue.s * 0.5).clamp(0.08, 0.4),
        },
        l: match dark {
            true => 0.115,
            false => 0.24,
        },
        a: 1.,
    };
    let glow = Hsla {
        l: match dark {
            true => hue.l.clamp(0.42, 0.72),
            false => hue.l.clamp(0.32, 0.52),
        },
        ..hue
    };
    // The sleeve's record: a pastel pressing in the theme's own hue, lighter
    // than the vinyl the starry stage paints so it reads against bare paint.
    let record = Hsla {
        s: (hue.s * 0.9).clamp(0.12, 0.5),
        l: match dark {
            true => 0.72,
            false => 0.58,
        },
        ..hue
    };

    Palette {
        halo: Hsla {
            s: (hue.s * 0.9).clamp(0.15, 0.7),
            l: match dark {
                true => (hue.l * 0.95).clamp(0.25, 0.6),
                false => hue.l.clamp(0.35, 0.55),
            },
            ..hue
        },
        disc,
        groove: Hsla {
            s: disc.s * 0.4,
            l: disc.l + 0.075,
            a: 0.5,
            ..disc
        },
        groove_dark: Hsla {
            s: disc.s * 0.4,
            l: disc.l - 0.065,
            a: 0.55,
            ..disc
        },
        rim: Hsla {
            l: disc.l + 0.2,
            a: 0.45,
            ..disc
        },
        ring: glow,
        // The particles carry the theme's own colour, not a neutral one: what
        // drifts off the rim should read as part of the same scheme as the disc
        // and the ring. On a light theme they take the hue deeper so they stay
        // held against the background instead of bleaching out.
        star: Hsla {
            s: hue.s.clamp(0.25, 0.8),
            l: match dark {
                true => (hue.l + 0.3).clamp(0.55, 0.82),
                false => (hue.l - 0.18).clamp(0.3, 0.5),
            },
            ..hue
        },
        sheen: neutral(0.9, SHEEN),
        marker: neutral(0.8, 0.06),
        label_edge: neutral(
            match dark {
                true => 0.04,
                false => 0.08,
            },
            0.35,
        ),
        record,
        record_groove: Hsla {
            l: record.l + 0.06,
            a: 0.7,
            ..record
        },
        record_edge: Hsla {
            l: record.l - 0.10,
            a: 0.6,
            ..record
        },
        hole: neutral(
            match dark {
                true => 0.10,
                false => 0.22,
            },
            1.,
        ),
    }
}
