// SPDX-License-Identifier: GPL-3.0-or-later

use gpui::prelude::*;
use gpui::{Anchor, AnyView, App, Context, Pixels, SharedString, Window, anchored, div, point, px};

use crate::metrics::Text;
use crate::theme::ActiveTheme as _;

const MARGIN: Pixels = px(8.);
const OFFSET: Pixels = px(6.);

#[derive(Clone, Copy, Default, PartialEq)]
pub enum Perch {
    #[default]
    Pointer,
    Above,
}

pub struct Tooltip {
    key: SharedString,
    perch: Perch,
}

impl Tooltip {
    pub fn new(key: impl Into<SharedString>) -> Self {
        Self {
            key: key.into(),
            perch: Perch::default(),
        }
    }

    pub fn perch(mut self, perch: Perch) -> Self {
        self.perch = perch;
        self
    }

    pub fn build(
        key: impl Into<SharedString>,
        perch: Perch,
    ) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
        let key = key.into();
        move |_, cx| cx.new(|_| Self::new(key.clone()).perch(perch)).into()
    }
}

impl Render for Tooltip {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let at = window.mouse_position();
        let (position, anchor) = match self.perch {
            Perch::Pointer => (at + point(-OFFSET, OFFSET), Anchor::TopRight),
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
                    .child(i18n::lookup(&self.key, None)),
            )
    }
}
