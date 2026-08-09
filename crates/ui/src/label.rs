// SPDX-License-Identifier: GPL-3.0-or-later

use gpui::prelude::*;
use gpui::{App, Div, FontWeight, SharedString, div};

use crate::metrics::Text;
use crate::theme::ActiveTheme as _;

pub fn eyebrow(label: impl Into<SharedString>, cx: &App) -> Div {
    let theme = cx.theme();

    div()
        .flex_none()
        .text_size(theme.text(Text::Small))
        .text_color(theme.muted_foreground)
        .font_weight(FontWeight::SEMIBOLD)
        .child(upper(label))
}

pub fn upper(label: impl Into<SharedString>) -> SharedString {
    label.into().to_uppercase().into()
}

pub fn vacant(label: impl Into<SharedString>, cx: &App) -> Div {
    let theme = cx.theme();

    div()
        .flex()
        .w_full()
        .items_center()
        .justify_center()
        .p(theme.metrics.pad * 2.)
        .text_align(gpui::TextAlign::Center)
        .text_size(theme.text(Text::Body))
        .text_color(theme.muted_foreground)
        .child(label.into())
}

pub fn heading(label: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .flex_none()
        .text_size(cx.theme().text(Text::Title))
        .font_weight(FontWeight::SEMIBOLD)
        .child(label.into())
}
