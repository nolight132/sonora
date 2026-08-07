// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 nolight132

use crate::metrics::Text;
use crate::theme::ActiveTheme as _;
use std::cell::Cell;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    App, Bounds, DragMoveEvent, Empty, Hsla, MouseButton, MouseDownEvent, MouseUpEvent, Pixels,
    Point, Render, SharedString, Window, canvas, div, px, relative,
};

const TRACK: f32 = 0.5;
const THUMB: f32 = 1.5;
const HIT: f32 = 2.;
const BUBBLE: f32 = 7.;

fn track(pad: Pixels) -> Pixels {
    px((pad / px(1.) * TRACK).round())
}

fn thumb(pad: Pixels) -> Pixels {
    px((pad / px(1.) * THUMB).round())
}

fn hit(pad: Pixels) -> Pixels {
    px((pad / px(1.) * HIT).round())
}

#[derive(Clone)]
struct Grab(SharedString);

impl Render for Grab {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

pub struct ScrubberState {
    id: SharedString,
    bounds: Rc<Cell<Bounds<Pixels>>>,
}

impl ScrubberState {
    pub fn new(id: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            bounds: Rc::new(Cell::new(Bounds::default())),
        }
    }

    pub fn hovered(&self, position: Point<Pixels>, pad: Pixels) -> Option<f32> {
        let bounds = self.bounds.get();
        let reach = Bounds {
            origin: gpui::point(bounds.origin.x, bounds.origin.y - hit(pad) / 2.),
            size: gpui::size(bounds.size.width, bounds.size.height + hit(pad)),
        };
        reach
            .contains(&position)
            .then(|| self.fraction_at(position.x, pad))
    }

    fn fraction_at(&self, x: Pixels, pad: Pixels) -> f32 {
        let bounds = self.bounds.get();
        let pin = thumb(pad);
        let travel = bounds.size.width - pin;
        if travel <= px(0.) {
            return 0.;
        }
        ((x - bounds.origin.x - pin / 2.) / travel).clamp(0., 1.)
    }
}

#[derive(IntoElement)]
pub struct Scrubber {
    id: SharedString,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    fraction: f32,
    filled: Hsla,
    empty: Hsla,
    thumb: Hsla,
    enabled: bool,
    bubble: Option<(f32, SharedString)>,
    lift: Pixels,
    on_move: Option<Box<dyn Fn(&f32, &mut Window, &mut App) + 'static>>,
    on_release: Option<Box<dyn Fn(&MouseUpEvent, &mut Window, &mut App) + 'static>>,
}

impl Scrubber {
    pub fn new(state: &ScrubberState, fraction: f32) -> Self {
        Self {
            id: state.id.clone(),
            bounds: state.bounds.clone(),
            fraction: fraction.clamp(0., 1.),
            filled: gpui::white(),
            empty: gpui::black(),
            thumb: gpui::white(),
            enabled: true,
            bubble: None,
            lift: px(12.),
            on_move: None,
            on_release: None,
        }
    }

    pub fn bubble(mut self, fraction: f32, text: impl Into<SharedString>) -> Self {
        self.bubble = Some((fraction.clamp(0., 1.), text.into()));
        self
    }

    pub fn lift(mut self, lift: Pixels) -> Self {
        self.lift = lift;
        self
    }

    pub fn colors(mut self, filled: Hsla, empty: Hsla, thumb: Hsla) -> Self {
        self.filled = filled;
        self.empty = empty;
        self.thumb = thumb;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn on_move(mut self, handler: impl Fn(&f32, &mut Window, &mut App) + 'static) -> Self {
        self.on_move = Some(Box::new(handler));
        self
    }

    pub fn on_release(
        mut self,
        handler: impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_release = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for Scrubber {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let pad = cx.theme().metrics.pad;
        let line = track(pad);
        let pin = thumb(pad);
        let reach = hit(pad);
        let bubble_width = px((pad / px(1.) * BUBBLE).round());
        let popover = cx.theme().popover;
        let popover_border = cx.theme().border;
        let popover_text = cx.theme().popover_foreground;
        let text_size = cx.theme().text(Text::Tiny);

        let Self {
            id,
            bounds,
            fraction,
            filled,
            empty,
            thumb,
            enabled,
            bubble,
            lift,
            on_move,
            on_release,
        } = self;

        let state = Rc::new(ScrubberState {
            id: id.clone(),
            bounds: bounds.clone(),
        });
        let on_move = on_move.map(Rc::new);
        let on_release = on_release.map(Rc::new);

        let down = {
            let state = state.clone();
            let on_move = on_move.clone();
            move |event: &MouseDownEvent, window: &mut Window, cx: &mut App| {
                if let Some(handler) = on_move.as_ref() {
                    handler(&state.fraction_at(event.position.x, pad), window, cx);
                }
            }
        };

        let dragged = {
            let state = state.clone();
            let on_move = on_move.clone();
            let mine = id.clone();
            move |event: &DragMoveEvent<Grab>, window: &mut Window, cx: &mut App| {
                if event.drag(cx).0 != mine {
                    return;
                }
                if let Some(handler) = on_move.as_ref() {
                    handler(&state.fraction_at(event.event.position.x, pad), window, cx);
                }
            }
        };

        let released = move |event: &MouseUpEvent, window: &mut Window, cx: &mut App| {
            if let Some(handler) = on_release.as_ref() {
                handler(event, window, cx);
            }
        };

        let width = bounds.get().size.width;
        let travel = (width - pin).max(Pixels::ZERO);
        let centered = enabled && width > Pixels::ZERO;

        div()
            .id(gpui::ElementId::Name(id.clone()))
            .flex()
            .items_center()
            .w_full()
            .h(reach)
            .when(enabled, |this| {
                this.cursor_pointer()
                    .on_mouse_down(MouseButton::Left, down)
                    .on_drag(Grab(id.clone()), |grab, _, _, cx| cx.new(|_| grab.clone()))
                    .on_drag_move(dragged)
                    .on_mouse_up(MouseButton::Left, released.clone())
                    .on_mouse_up_out(MouseButton::Left, released)
            })
            .child(
                div()
                    .relative()
                    .w_full()
                    .h(line)
                    .rounded_full()
                    .bg(empty)
                    .child(
                        div()
                            .h_full()
                            .rounded_full()
                            .bg(filled)
                            .map(|this| match centered {
                                true => this.w(pin / 2. + travel * fraction),
                                false => this.w(relative(fraction)),
                            }),
                    )
                    .when(enabled, |this| {
                        this.child(
                            div()
                                .absolute()
                                .top((line - pin) / 2.)
                                .map(|this| match width > Pixels::ZERO {
                                    true => this.left(travel * fraction),
                                    false => {
                                        this.left(relative(fraction)).ml(Pixels::ZERO - pin / 2.)
                                    }
                                })
                                .size(pin)
                                .rounded_full()
                                .bg(thumb),
                        )
                    })
                    .when_some(bubble, |this, (at, text)| {
                        this.child(
                            div()
                                .absolute()
                                .map(|this| {
                                    if lift >= Pixels::ZERO {
                                        this.bottom(lift)
                                    } else {
                                        this.top(Pixels::ZERO - lift)
                                    }
                                })
                                .map(|this| match centered {
                                    true => this.left(pin / 2. + travel * at),
                                    false => this.left(relative(at)),
                                })
                                .ml(Pixels::ZERO - bubble_width / 2.)
                                .w(bubble_width)
                                .flex()
                                .justify_center()
                                .child(
                                    div()
                                        .px_1p5()
                                        .rounded_md()
                                        .bg(popover)
                                        .border_1()
                                        .border_color(popover_border)
                                        .text_color(popover_text)
                                        .text_size(text_size)
                                        .child(text),
                                ),
                        )
                    })
                    .child(
                        canvas(move |b, _, _| bounds.set(b), |_, _, _, _| {})
                            .absolute()
                            .size_full(),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use gpui::{Bounds, point, px, size};

    use super::{ScrubberState, thumb};

    const PAD: gpui::Pixels = px(8.);
    const ORIGIN: f32 = 20.;
    const WIDTH: f32 = 300.;

    fn measured() -> ScrubberState {
        let state = ScrubberState::new("test");
        state.bounds.set(Bounds {
            origin: point(px(ORIGIN), px(0.)),
            size: size(px(WIDTH), px(4.)),
        });
        state
    }

    #[test]
    fn the_thumb_lands_under_the_pointer() {
        let state = measured();
        let pin = thumb(PAD);
        let travel = px(WIDTH) - pin;

        for step in 0..=100 {
            let fraction = step as f32 / 100.;
            let center = px(ORIGIN) + pin / 2. + travel * fraction;
            let read = state.fraction_at(center, PAD);
            assert!((read - fraction).abs() < 1e-4, "{fraction} read as {read}");
        }
    }

    #[test]
    fn the_ends_clamp() {
        let state = measured();

        assert_eq!(state.fraction_at(px(0.), PAD), 0.);
        assert_eq!(state.fraction_at(px(ORIGIN), PAD), 0.);
        assert_eq!(state.fraction_at(px(ORIGIN + WIDTH), PAD), 1.);
        assert_eq!(state.fraction_at(px(9999.), PAD), 1.);
    }

    #[test]
    fn an_unmeasured_track_reads_zero() {
        let state = ScrubberState::new("test");

        assert_eq!(state.fraction_at(px(50.), PAD), 0.);
    }
}
