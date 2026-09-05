use gpui::prelude::*;
use gpui::{
    App, Div, MouseButton, Pixels, StyleRefinement, Window, WindowControlArea, div, px, svg,
};

use crate::theme::ActiveTheme as _;

const SYSTEM_ACTS: bool = cfg!(target_os = "windows");
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

    fn system(self) -> bool {
        SYSTEM_ACTS && matches!(self, Self::Maximize | Self::Restore)
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
    base: Div,
    leading: bool,
}

impl WindowControls {
    pub fn new(leading: bool) -> Self {
        Self {
            base: div(),
            leading,
        }
    }
}

impl Styled for WindowControls {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl RenderOnce for WindowControls {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let supported = window.window_controls();
        let maximized = window.is_maximized();
        let theme = *cx.theme();
        let overrides = std::mem::take(self.base.style());

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

        let is_windows = cfg!(target_os = "windows");
        let mut controls = self
            .base
            .flex()
            .flex_none()
            .items_center()
            .when_else(
                is_windows,
                |this| this.h_full().self_stretch().gap_0(),
                |this| this.gap_2(),
            )
            .when(!SYSTEM_ACTS, |this| {
                this.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            })
            .children(wanted.into_iter().map(move |control| {
                let danger = control == Control::Close;

                div()
                    .id(control.id())
                    .group(control.id())
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .when_else(
                        is_windows,
                        |this| this.h_full().aspect_square().rounded_none(),
                        |this| this.size(BUTTON).rounded(theme.radius).cursor_pointer(),
                    )
                    .occlude()
                    .window_control_area(control.area())
                    .hover(move |style| {
                        style.bg(match danger {
                            true => theme.danger,
                            false => theme.secondary_active,
                        })
                    })
                    .when(is_windows, |this| {
                        this.active(move |style| {
                            style.bg(match danger {
                                true => theme.danger_hover,
                                false => theme.secondary_hover,
                            })
                        })
                    })
                    .child(
                        svg()
                            .path(icons::path(control.icon()))
                            .id("glyph")
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
                    .when(!control.system(), |this| {
                        this.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(move |_, window, cx| {
                                cx.stop_propagation();
                                match control {
                                    Control::Minimize => window.minimize_window(),
                                    Control::Maximize | Control::Restore => window.zoom_window(),
                                    Control::Close => window.remove_window(),
                                }
                            })
                    })
            }));
        controls.style().refine(&overrides);
        controls
    }
}
