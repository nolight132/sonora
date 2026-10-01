use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    Anchor, AnyView, App, Context, DispatchPhase, MouseMoveEvent, Pixels, Point, SharedString,
    Window, anchored, canvas, div, point, px,
};

use crate::metrics::Text;
use crate::theme::ActiveTheme as _;

const MARGIN: Pixels = px(8.);
const OFFSET: Pixels = px(6.);

/// Where a tooltip sits relative to the pointer that summoned it.
#[derive(Clone, Copy, Default, PartialEq)]
pub enum Perch {
    /// Below and to the left of the pointer, fixed where the hover delay ended.
    #[default]
    Pointer,
    /// Centred above the pointer, clear of a small control under it.
    Above,
    /// Placed like `Pointer`, but shown the moment the pointer arrives and moved along with it.
    Follow,
}

pub struct Tooltip {
    text: SharedString,
    raw: bool,
    perch: Perch,
    at: Point<Pixels>,
}

/// Attaches a tooltip to an element with the show delay its perch calls for.
pub trait Tipped: Sized {
    /// Shows the Fluent string `key` on hover.
    fn tip(self, key: impl Into<SharedString>, perch: Perch) -> Self;
    /// Shows `text` on hover as it is, without a Fluent lookup.
    fn tip_label(self, text: impl Into<SharedString>, perch: Perch) -> Self;
}

impl<E: StatefulInteractiveElement> Tipped for E {
    fn tip(self, key: impl Into<SharedString>, perch: Perch) -> Self {
        timed(self.tooltip(Tooltip::build(key, perch)), perch)
    }

    fn tip_label(self, text: impl Into<SharedString>, perch: Perch) -> Self {
        timed(self.tooltip(Tooltip::label(text, perch)), perch)
    }
}

impl Tooltip {
    pub fn new(key: impl Into<SharedString>, at: Point<Pixels>) -> Self {
        Self {
            text: key.into(),
            raw: false,
            perch: Perch::default(),
            at,
        }
    }

    pub fn perch(mut self, perch: Perch) -> Self {
        self.perch = perch;
        self
    }

    pub fn raw(mut self) -> Self {
        self.raw = true;
        self
    }

    fn build(
        key: impl Into<SharedString>,
        perch: Perch,
    ) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
        let key = key.into();
        move |window, cx| {
            let at = window.mouse_position();
            cx.new(|_| Self::new(key.clone(), at).perch(perch)).into()
        }
    }

    fn label(
        text: impl Into<SharedString>,
        perch: Perch,
    ) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
        let text = text.into();
        move |window, cx| {
            let at = window.mouse_position();
            cx.new(|_| Self::new(text.clone(), at).perch(perch).raw())
                .into()
        }
    }
}

impl Render for Tooltip {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let follow = self.perch == Perch::Follow;
        let at = match follow {
            true => window.mouse_position(),
            false => self.at,
        };
        let (position, anchor) = match self.perch {
            Perch::Pointer | Perch::Follow => (at + point(-OFFSET, OFFSET), Anchor::TopRight),
            Perch::Above => (
                point(at.x, at.y - theme.metrics.control_small / 2.),
                Anchor::BottomCenter,
            ),
        };

        anchored()
            .position(position)
            .anchor(anchor)
            .snap_to_window_with_margin(MARGIN)
            .child(
                div()
                    .px_2()
                    .py_1()
                    .rounded(theme.radius)
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.popover)
                    .text_size(theme.text(Text::Small))
                    .text_color(theme.popover_foreground)
                    .child(match self.raw {
                        true => self.text.clone(),
                        false => i18n::lookup(&self.text, None),
                    })
                    .when(follow, |this| this.child(tracker())),
            )
    }
}

/// Drops the hover delay for a perch that has to be there the moment the pointer arrives.
fn timed<E: StatefulInteractiveElement>(element: E, perch: Perch) -> E {
    match perch {
        Perch::Follow => element.tooltip_show_delay(Duration::ZERO),
        Perch::Pointer | Perch::Above => element,
    }
}

/// Redraws the window on every pointer move so a following tooltip renders at the new position.
fn tracker() -> impl IntoElement {
    canvas(
        |_, _, _| {},
        |_, _, window, _| {
            window.on_mouse_event(|_: &MouseMoveEvent, phase, window, _| {
                if phase == DispatchPhase::Bubble {
                    window.refresh();
                }
            })
        },
    )
    .absolute()
}
