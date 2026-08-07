// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 nolight132

use crate::theme::ActiveTheme as _;
use gpui::prelude::*;
use gpui::{App, IntoElement, RenderOnce, Window, div};

#[derive(IntoElement)]
pub struct ExplicitBadge {}

impl ExplicitBadge {
    pub fn new() -> Self {
        Self {}
    }
}

impl RenderOnce for ExplicitBadge {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *cx.theme();
        div()
            .size_4()
            .flex()
            .items_center()
            .justify_center()
            .text_xs()
            .text_color(theme.muted_foreground)
            .bg(theme.muted)
            .rounded_xs()
            .child("E")
    }
}
