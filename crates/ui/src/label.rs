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

pub fn heading(label: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .flex_none()
        .text_size(cx.theme().text(Text::Title))
        .font_weight(FontWeight::SEMIBOLD)
        .child(label.into())
}
