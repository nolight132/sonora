// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 nolight132

use gpui::prelude::*;
use gpui::{App, Div, ElementId, Interactivity, Stateful, StyleRefinement, Window, div};

#[derive(IntoElement)]
pub struct Shield {
    base: Stateful<Div>,
}

impl Shield {
    #[track_caller]
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self { base: div().id(id) }
    }
}

impl Styled for Shield {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl InteractiveElement for Shield {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.base.interactivity()
    }
}

impl StatefulInteractiveElement for Shield {}

impl RenderOnce for Shield {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let mut base = self.base;
        let overrides = std::mem::take(base.style());

        let mut shield = base.occlude();
        shield.style().refine(&overrides);
        shield
    }
}
