// SPDX-License-Identifier: GPL-3.0-or-later

use std::cell::Cell as Slot;
use std::rc::Rc;
use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    AnyWindowHandle, App, Context, DragMoveEvent, Empty, EntityId, MouseButton, MouseDownEvent,
    Pixels, Render, ScrollHandle, Task, Window, div, point, px,
};

use crate::theme::ActiveTheme as _;

const BAR: Pixels = px(6.);
const MIN_THUMB: Pixels = px(24.);
const LINGER: Duration = Duration::from_secs(2);
const IDLE: f32 = 0.;
const RESTING: f32 = 0.35;
const ACTIVE: f32 = 0.55;

type HoverGuard = Rc<dyn Fn(bool, AnyWindowHandle, &mut App)>;

#[derive(Clone)]
struct Grab {
    owner: EntityId,
    start: Slot<Pixels>,
    offset: Slot<Pixels>,
}

impl Render for Grab {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

pub fn scrolled(scroll: &ScrollHandle) -> Pixels {
    (-scroll.offset().y).clamp(Pixels::ZERO, scroll.max_offset().y)
}

pub struct Scrollbar {
    scroll: ScrollHandle,
    seen: Pixels,
    awake: bool,
    hovered: bool,
    always_visible: bool,
    track_inset: Pixels,
    hover_guard: Option<HoverGuard>,
    linger: Option<Task<()>>,
}

impl Scrollbar {
    pub fn new(scroll: ScrollHandle) -> Self {
        Self {
            scroll,
            seen: Pixels::ZERO,
            awake: false,
            hovered: false,
            always_visible: false,
            track_inset: Pixels::ZERO,
            hover_guard: None,
            linger: None,
        }
    }

    pub fn always_visible(mut self) -> Self {
        self.always_visible = true;
        self
    }

    pub fn track_inset(mut self, inset: Pixels) -> Self {
        self.track_inset = inset;
        self
    }

    pub fn scroll(&self) -> &ScrollHandle {
        &self.scroll
    }

    pub(crate) fn set_hover_guard(
        &mut self,
        guard: impl Fn(bool, AnyWindowHandle, &mut App) + 'static,
    ) {
        self.hover_guard = Some(Rc::new(guard));
    }

    fn wake(&mut self, cx: &mut Context<Self>) {
        self.awake = true;
        cx.notify();
        self.linger = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(LINGER).await;
            this.update(cx, |this, cx| {
                this.awake = false;
                cx.notify();
            })
            .ok();
        }));
    }
}

impl Render for Scrollbar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport = self.scroll.bounds().size.height;
        let hidden = self.scroll.max_offset().y;
        let offset = scrolled(&self.scroll);

        if offset != self.seen {
            self.seen = offset;
            self.wake(cx);
        }

        if viewport <= Pixels::ZERO || hidden <= Pixels::ZERO {
            return div().into_any_element();
        }

        let theme = *cx.theme();
        let content = viewport + hidden;
        let progress = (offset / hidden).clamp(0., 1.);
        let track = (viewport - self.track_inset * 2.).max(Pixels::ZERO);
        let thumb = (track * (viewport / content)).max(MIN_THUMB).min(track);
        let travel = track - thumb;
        let resting = match self.always_visible || self.awake || self.hovered {
            true => RESTING,
            false => IDLE,
        };

        let jump = self.scroll.clone();
        let drag = self.scroll.clone();
        let owner = cx.entity_id();
        let hover_guard = self.hover_guard.clone();

        div()
            .id("scrollbar")
            .occlude()
            .absolute()
            .top(self.track_inset)
            .right_0()
            .w(BAR)
            .h(track)
            .on_hover(cx.listener(move |this, hovered: &bool, window, cx| {
                this.hovered = *hovered;
                this.wake(cx);
                if let Some(guard) = hover_guard.as_ref() {
                    guard(*hovered, window.window_handle(), cx);
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    let local =
                        event.position.y - jump.bounds().origin.y - this.track_inset - thumb / 2.;
                    let fraction = (local / travel).clamp(0., 1.);
                    jump.set_offset(point(Pixels::ZERO, Pixels::ZERO - hidden * fraction));
                    this.wake(cx);
                }),
            )
            .child(
                div()
                    .id("scrollbar-thumb")
                    .absolute()
                    .top(travel * progress)
                    .right_1()
                    .w(BAR)
                    .h(thumb)
                    .rounded_full()
                    .bg(theme.muted_foreground.opacity(resting))
                    .hover(move |style| style.bg(theme.muted_foreground.opacity(ACTIVE)))
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _: &MouseDownEvent, _, cx| {
                            this.wake(cx);
                            cx.stop_propagation();
                        }),
                    )
                    .on_drag(
                        Grab {
                            owner,
                            start: Slot::new(Pixels::ZERO),
                            offset: Slot::new(offset),
                        },
                        |grab, _, window, cx| {
                            grab.start.set(window.mouse_position().y);
                            cx.new(|_| grab.clone())
                        },
                    )
                    .on_drag_move(
                        cx.listener(move |this, event: &DragMoveEvent<Grab>, _, cx| {
                            let (start, base) = {
                                let grab = event.drag(cx);
                                if grab.owner != owner {
                                    return;
                                }
                                (grab.start.get(), grab.offset.get())
                            };
                            let moved = event.event.position.y - start;
                            let scrolled = base + moved * (hidden / travel);
                            let clamped = scrolled.clamp(Pixels::ZERO, hidden);
                            drag.set_offset(point(Pixels::ZERO, Pixels::ZERO - clamped));
                            this.wake(cx);
                        }),
                    ),
            )
            .into_any_element()
    }
}
