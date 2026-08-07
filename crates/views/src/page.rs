// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 nolight132

use gpui::{App, Entity, Pixels, ScrollHandle, Window, px};

use state::Playback;
use ui::{GridState, Viewport, scrolled};

use crate::cells;
use crate::tracks::{self, TrackSource};

const FRAME: Pixels = px(1.);

pub(crate) fn reserved(inset: Pixels) -> Pixels {
    inset * 2. + px(2.)
}

pub(crate) fn play(
    table: &Entity<GridState<TrackSource>>,
    playback: &Entity<Playback>,
    display: usize,
    cx: &mut App,
) {
    let queued = tracks::ordered(table, cx);
    playback.update(cx, |playback, cx| playback.start(queued, display, cx));
}

pub(crate) fn resize(
    table: &Entity<GridState<TrackSource>>,
    width: &mut Pixels,
    inset: Pixels,
    window: &Window,
    cx: &mut App,
) {
    let next = cells::content_width(window, reserved(inset), cx);
    if (next - *width).abs() < px(0.5) {
        return;
    }
    *width = next;
    table.update(cx, |table, cx| {
        table.delegate_mut().set_width(next, cx);
        table.refresh(cx);
    });
}

pub(crate) fn viewport(scroll: &ScrollHandle, inset: Pixels, window: &Window) -> Viewport {
    let hero = scroll
        .bounds_for_item(0)
        .map(|bounds| bounds.size.height)
        .unwrap_or_default();
    let visible = scroll.bounds().size.height;

    Viewport {
        top: (scrolled(scroll) - inset - hero - FRAME).max(Pixels::ZERO),
        height: match visible > Pixels::ZERO {
            true => visible,
            false => window.viewport_size().height,
        },
    }
}
