// SPDX-License-Identifier: GPL-3.0-or-later

use gpui::prelude::*;
use gpui::{Anchor, AnyView, App, Context, Pixels, SharedString, Window, anchored, div, point, px};

use crate::metrics::Text;
use crate::theme::ActiveTheme as _;

const MARGIN: Pixels = px(8.);
const OFFSET: Pixels = px(6.);

pub struct Tooltip {
    key: SharedString,
}

impl Tooltip {
    pub fn new(key: impl Into<SharedString>) -> Self {
        Self { key: key.into() }
    }

    pub fn build(
        key: impl Into<SharedString>,
    ) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
        let key = key.into();
        move |_, cx| cx.new(|_| Self::new(key.clone())).into()
    }
}

impl Render for Tooltip {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let at = window.mouse_position() + point(-OFFSET, OFFSET);

        anchored()
            .position(at)
            .anchor(Anchor::TopRight)
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
