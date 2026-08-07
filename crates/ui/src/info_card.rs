// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 nolight132

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Div, FontWeight, Interactivity, SharedString, StyleRefinement, Window, div, px,
};

use crate::label::eyebrow;
use crate::theme::ActiveTheme as _;

#[derive(IntoElement)]
pub struct InfoCard {
    base: Div,
    title: SharedString,
    children: Vec<AnyElement>,
    borderless: bool,
    stretch: bool,
}

impl InfoCard {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            base: div(),
            title: title.into(),
            children: Vec::new(),
            borderless: false,
            stretch: false,
        }
    }

    pub fn borderless(mut self) -> Self {
        self.borderless = true;
        self
    }

    pub fn stretch(mut self) -> Self {
        self.stretch = true;
        self
    }
}

impl ParentElement for InfoCard {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for InfoCard {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl InteractiveElement for InfoCard {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.base.interactivity()
    }
}

impl RenderOnce for InfoCard {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            mut base,
            title,
            children,
            borderless,
            stretch,
        } = self;

        let theme = *cx.theme();
        let overrides = std::mem::take(base.style());

        let mut card = base
            .flex()
            .flex_col()
            .when(stretch, |this| this.h_full())
            .gap_4()
            .p_5()
            .rounded(theme.radius)
            .when(!borderless, |this| {
                this.border_1().border_color(theme.border)
            })
            .bg(theme.secondary.opacity(0.45))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(eyebrow(title, cx))
                    .child(div().h(px(1.)).flex_1().bg(theme.border)),
            )
            .children(children);

        card.style().refine(&overrides);
        card
    }
}

#[derive(IntoElement)]
pub struct Fact {
    base: Div,
    label: SharedString,
    value: SharedString,
    striped: bool,
}

impl Fact {
    pub fn new(label: impl Into<SharedString>, value: impl Into<SharedString>) -> Self {
        Self {
            base: div(),
            label: label.into(),
            value: value.into(),
            striped: false,
        }
    }

    pub fn striped(mut self, striped: bool) -> Self {
        self.striped = striped;
        self
    }
}

impl Styled for Fact {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl InteractiveElement for Fact {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.base.interactivity()
    }
}

impl RenderOnce for Fact {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            mut base,
            label,
            value,
            striped,
        } = self;

        let theme = *cx.theme();
        let overrides = std::mem::take(base.style());

        let mut fact = base
            .flex()
            .items_center()
            .justify_between()
            .gap_4()
            .px(theme.metrics.pad)
            .py(theme.metrics.pad / 2.)
            .rounded(theme.radius)
            .when(striped, |this| this.bg(theme.table_hover.opacity(0.35)))
            .child(div().text_color(theme.muted_foreground).child(label))
            .child(
                div()
                    .text_right()
                    .font_weight(FontWeight::MEDIUM)
                    .child(value),
            );

        fact.style().refine(&overrides);
        fact
    }
}
