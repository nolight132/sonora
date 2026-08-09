// SPDX-License-Identifier: GPL-3.0-or-later

use gpui::prelude::*;
use gpui::{AnyElement, App, Div, StyleRefinement, Window, div, px};

use crate::theme::ActiveTheme as _;

const LINE: f32 = 1.;
const TICK: f32 = 6.;

#[derive(IntoElement)]
pub struct Tabs {
    base: Div,
    items: Vec<AnyElement>,
}

impl Tabs {
    pub fn new() -> Self {
        Self {
            base: div(),
            items: Vec::new(),
        }
    }

    pub fn items(mut self, items: impl IntoIterator<Item = impl IntoElement>) -> Self {
        self.items = items
            .into_iter()
            .map(IntoElement::into_any_element)
            .collect();
        self
    }
}

impl Styled for Tabs {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl RenderOnce for Tabs {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self { mut base, items } = self;
        let theme = cx.theme();
        let height = theme.metrics.control;
        let middle = height / 2.;
        let border = theme.sidebar_border;
        let overrides = std::mem::take(base.style());

        let mut tabs = base
            .relative()
            .flex()
            .flex_col()
            .gap_1()
            .ml_4()
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom(middle)
                    .w(px(LINE))
                    .bg(border),
            )
            .children(items.into_iter().map(|item| {
                div()
                    .relative()
                    .flex()
                    .items_center()
                    .h(height)
                    .pl_3()
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .top(middle)
                            .w(px(TICK))
                            .h(px(LINE))
                            .bg(border),
                    )
                    .child(div().flex().flex_1().child(item))
            }));

        tabs.style().refine(&overrides);
        tabs
    }
}
