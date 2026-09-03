use gpui::prelude::*;
use gpui::{App, Div, IntoElement, Pixels, RenderOnce, StyleRefinement, Window, div, px};

use crate::theme::ActiveTheme as _;

const GAP: f32 = 3.;
const OPACITY: f32 = 0.32;
const FLOOR: f32 = 0.03;

#[derive(IntoElement)]
pub struct Visualizer {
    base: Div,
    levels: Vec<f32>,
    max: Pixels,
}

impl Visualizer {
    #[track_caller]
    pub fn new(levels: Vec<f32>, max: Pixels) -> Self {
        Self {
            base: div(),
            levels,
            max,
        }
    }
}

impl Styled for Visualizer {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl RenderOnce for Visualizer {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            mut base,
            levels,
            max,
        } = self;
        let theme = *cx.theme();
        let overrides = std::mem::take(base.style());

        let mut visualizer = base
            .h(max)
            .flex()
            .items_end()
            .justify_center()
            .gap(px(GAP))
            .children(levels.into_iter().enumerate().map(|(index, level)| {
                div()
                    .id(("visualizer-bar", index))
                    .flex_1()
                    // .rounded_t(theme.radius) -> for inherit radius from theme
                    .bg(theme.primary.opacity(OPACITY))
                    .h(max * level.clamp(FLOOR, 1.))
            }));
        visualizer.style().refine(&overrides);
        visualizer
    }
}
