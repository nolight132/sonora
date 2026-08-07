// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 nolight132

use std::f32::consts::TAU;

use gpui::{App, AppContext as _, Hsla, ImgResourceLoader, Resource, Rgba, SharedUri, Task};

const BINS: usize = 24;
const SAMPLES: usize = 6000;
const MIN_ALPHA: u8 = 128;
const MIN_SATURATION: f32 = 0.14;
const MIN_LIGHTNESS: f32 = 0.20;
const MAX_LIGHTNESS: f32 = 0.94;
const MIN_SHARE: f32 = 0.05;

pub fn tint(url: impl Into<SharedUri>, cx: &mut App) -> Task<Option<Hsla>> {
    let (load, _) = cx.fetch_asset::<ImgResourceLoader>(&Resource::Uri(url.into()));

    cx.spawn(async move |cx| {
        let image = load.await.ok()?;
        cx.background_spawn(async move { dominant(image.as_bytes(0)?) })
            .await
    })
}

#[derive(Clone, Copy, Default)]
struct Bin {
    weight: f32,
    x: f32,
    y: f32,
    saturation: f32,
    lightness: f32,
}

impl Bin {
    fn add(&mut self, color: Hsla, weight: f32) {
        let angle = color.h * TAU;
        self.weight += weight;
        self.x += angle.cos() * weight;
        self.y += angle.sin() * weight;
        self.saturation += color.s * weight;
        self.lightness += color.l * weight;
    }

    fn merge(&mut self, other: &Self) {
        self.weight += other.weight;
        self.x += other.x;
        self.y += other.y;
        self.saturation += other.saturation;
        self.lightness += other.lightness;
    }

    fn colour(&self) -> Option<Hsla> {
        (self.weight > 0.).then(|| Hsla {
            h: self.y.atan2(self.x).rem_euclid(TAU) / TAU,
            s: (self.saturation / self.weight).clamp(0., 1.),
            l: (self.lightness / self.weight).clamp(0., 1.),
            a: 1.,
        })
    }
}

fn dominant(pixels: &[u8]) -> Option<Hsla> {
    let stride = (pixels.len() / 4 / SAMPLES).max(1);
    let mut bins = [Bin::default(); BINS];
    let mut sampled = 0.;

    for pixel in pixels.chunks_exact(4).step_by(stride) {
        let [blue, green, red, alpha] = [pixel[0], pixel[1], pixel[2], pixel[3]];
        if alpha < MIN_ALPHA {
            continue;
        }
        sampled += 1.;

        let colour = Hsla::from(Rgba {
            r: red as f32 / 255.,
            g: green as f32 / 255.,
            b: blue as f32 / 255.,
            a: 1.,
        });
        if colour.s < MIN_SATURATION || colour.l < MIN_LIGHTNESS || colour.l > MAX_LIGHTNESS {
            continue;
        }

        let index = ((colour.h * BINS as f32) as usize).min(BINS - 1);
        bins[index].add(colour, colour.s * (1. - (colour.l - 0.5).abs()));
    }

    let peak = (0..BINS).max_by(|&a, &b| score(&bins, a).total_cmp(&score(&bins, b)))?;
    if score(&bins, peak) < sampled * MIN_SHARE {
        return None;
    }

    let mut cluster = bins[peak];
    cluster.merge(&bins[(peak + BINS - 1) % BINS]);
    cluster.merge(&bins[(peak + 1) % BINS]);
    cluster.colour()
}

fn score(bins: &[Bin; BINS], index: usize) -> f32 {
    bins[index].weight
        + (bins[(index + BINS - 1) % BINS].weight + bins[(index + 1) % BINS].weight) * 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(colours: &[([u8; 3], usize)]) -> Vec<u8> {
        colours
            .iter()
            .flat_map(|&([red, green, blue], count)| {
                std::iter::repeat_n([blue, green, red, 255], count).flatten()
            })
            .collect()
    }

    #[test]
    fn finds_the_dominant_hue() {
        let pixels = image(&[([204, 34, 34], 900), ([32, 32, 32], 100)]);
        let colour = dominant(&pixels).expect("a red image is colourful");

        assert!(colour.h < 0.02 || colour.h > 0.98, "hue was {}", colour.h);
        assert!(colour.s > 0.5, "saturation was {}", colour.s);
    }

    #[test]
    fn averages_across_the_bin_boundary() {
        let pixels = image(&[([255, 0, 60], 500), ([255, 60, 0], 500)]);
        let colour = dominant(&pixels).expect("a red image is colourful");

        assert!(colour.h < 0.02 || colour.h > 0.98, "hue was {}", colour.h);
    }

    #[test]
    fn ignores_greyscale_artwork() {
        let pixels = image(&[([18, 18, 18], 500), ([200, 200, 200], 500)]);

        assert!(dominant(&pixels).is_none());
    }

    #[test]
    fn ignores_a_small_splash_of_colour() {
        let pixels = image(&[([120, 120, 120], 990), ([0, 180, 255], 10)]);

        assert!(dominant(&pixels).is_none());
    }
}
