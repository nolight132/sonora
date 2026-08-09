// SPDX-License-Identifier: GPL-3.0-or-later

use gpui::prelude::*;
use gpui::{App, Div, Interactivity, Pixels, StyleRefinement, Window, div, px};

use crate::theme::ActiveTheme as _;

const THICKNESS: Pixels = px(1.);

#[derive(Clone, Copy, PartialEq)]
enum Axis {
    Horizontal,
    Vertical,
}

#[derive(IntoElement)]
pub struct Separator {
    base: Div,
    axis: Axis,
}

impl Separator {
    #[track_caller]
    pub fn horizontal() -> Self {
        Self {
            base: div(),
            axis: Axis::Horizontal,
        }
    }

    #[track_caller]
    pub fn vertical() -> Self {
        Self {
            base: div(),
            axis: Axis::Vertical,
        }
    }
}

impl Styled for Separator {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl InteractiveElement for Separator {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.base.interactivity()
    }
}

impl RenderOnce for Separator {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self { mut base, axis } = self;
        let theme = cx.theme();
        let overrides = std::mem::take(base.style());

        let mut separator = base.flex_none().bg(theme.border).when_else(
            axis == Axis::Horizontal,
            |this| this.h(THICKNESS),
            |this| this.w(THICKNESS),
        );
        separator.style().refine(&overrides);
        separator
    }
}
