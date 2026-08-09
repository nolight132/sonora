// SPDX-License-Identifier: GPL-3.0-or-later

use gpui::prelude::*;
use gpui::{
    App, ClickEvent, Div, ElementId, Interactivity, Stateful, StyleRefinement, Window, div, px,
};

use crate::theme::ActiveTheme as _;

const SCALE: f32 = 0.85;
const INSET: f32 = 2.;
const WIDTH: f32 = 1.75;

#[derive(IntoElement)]
pub struct Switch {
    base: Stateful<Div>,
    checked: bool,
    disabled: bool,
    on_click: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
}

impl Switch {
    #[track_caller]
    pub fn new(id: impl Into<ElementId>, checked: bool) -> Self {
        Self {
            base: div().id(id),
            checked,
            disabled: false,
            on_click: None,
        }
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl Styled for Switch {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl InteractiveElement for Switch {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.base.interactivity()
    }
}

impl RenderOnce for Switch {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            mut base,
            checked,
            disabled,
            on_click,
        } = self;
        let theme = cx.theme();
        let height = theme.metrics.control_small * SCALE;
        let thumb = height - px(INSET * 2.);
        let background = match checked {
            true => theme.primary,
            false => theme.muted,
        };
        let hover = match checked {
            true => theme.primary_hover,
            false => theme.secondary_hover,
        };
        let overrides = std::mem::take(base.style());

        let mut switch = base
            .flex()
            .flex_none()
            .items_center()
            .when(checked, |this| this.justify_end())
            .w(height * WIDTH)
            .h(height)
            .p(px(INSET))
            .rounded(height / 2.)
            .bg(background)
            .border_1()
            .border_color(match checked {
                true => theme.primary,
                false => theme.border,
            })
            .when(disabled, |this| this.opacity(0.4))
            .when(!disabled, |this| {
                this.cursor_pointer().hover(move |style| style.bg(hover))
            })
            .child(div().size(thumb).rounded(thumb / 2.).bg(match checked {
                true => theme.primary_foreground,
                false => theme.muted_foreground,
            }));

        switch.style().refine(&overrides);
        if !disabled && let Some(handler) = on_click {
            switch = switch.on_click(handler);
        }
        switch
    }
}
