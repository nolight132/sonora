// SPDX-License-Identifier: GPL-3.0-or-later

use gpui::prelude::*;
use gpui::{App, MouseButton, Pixels, Window, WindowControlArea, div, px, svg};

use crate::theme::ActiveTheme as _;

const BUTTON: Pixels = px(20.);
const GLYPH: Pixels = px(16.);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Control {
    Minimize,
    Maximize,
    Restore,
    Close,
}

impl Control {
    fn icon(self) -> &'static str {
        match self {
            Self::Minimize => "icons/window-minimize.svg",
            Self::Maximize => "icons/window-maximize.svg",
            Self::Restore => "icons/window-restore.svg",
            Self::Close => "icons/window-close.svg",
        }
    }

    fn id(self) -> &'static str {
        match self {
            Self::Minimize => "window-minimize",
            Self::Maximize | Self::Restore => "window-maximize",
            Self::Close => "window-close",
        }
    }

    fn area(self) -> WindowControlArea {
        match self {
            Self::Minimize => WindowControlArea::Min,
            Self::Maximize | Self::Restore => WindowControlArea::Max,
            Self::Close => WindowControlArea::Close,
        }
    }
}

#[derive(IntoElement)]
pub struct WindowControls {
    leading: bool,
}

impl WindowControls {
    pub fn new(leading: bool) -> Self {
        Self { leading }
    }
}

impl RenderOnce for WindowControls {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let supported = window.window_controls();
        let maximized = window.is_maximized();
        let theme = *cx.theme();

        let mut wanted: Vec<Control> = [
            supported.minimize.then_some(Control::Minimize),
            supported.maximize.then_some(match maximized {
                true => Control::Restore,
                false => Control::Maximize,
            }),
            Some(Control::Close),
        ]
        .into_iter()
        .flatten()
        .collect();

        if self.leading {
            wanted.reverse();
        }

        div()
            .flex()
            .flex_none()
            .items_center()
            .gap_2()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .children(wanted.into_iter().map(move |control| {
                let danger = control == Control::Close;

                div()
                    .id(control.id())
                    .group(control.id())
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(BUTTON)
                    .rounded(theme.radius)
                    .cursor_pointer()
                    .window_control_area(control.area())
                    .hover(move |style| {
                        style.bg(match danger {
                            true => theme.danger,
                            false => theme.secondary_active,
                        })
                    })
                    .child(
                        svg()
                            .path(control.icon())
                            .size(GLYPH)
                            .flex_none()
                            .text_color(theme.muted_foreground)
                            .group_hover(control.id(), move |style| {
                                style.text_color(match danger {
                                    true => theme.danger_foreground,
                                    false => theme.foreground,
                                })
                            }),
                    )
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        match control {
                            Control::Minimize => window.minimize_window(),
                            Control::Maximize | Control::Restore => window.zoom_window(),
                            Control::Close => window.remove_window(),
                        }
                    })
            }))
    }
}
