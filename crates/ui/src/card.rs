// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 nolight132

use gpui::prelude::*;
use gpui::{
    AnyElement, App, ClickEvent, Div, ElementId, FontWeight, Hsla, Interactivity, Pixels,
    SharedString, Stateful, StyleRefinement, Window, div, px,
};

use crate::ExplicitBadge;
use crate::artwork::{Artwork, Avatar};
use crate::label::upper;
use crate::metrics::{Text, snapped};
use crate::skeleton::Skeleton;
use crate::theme::ActiveTheme as _;

const TITLE: Pixels = px(120.);
const BAR_TITLE: (Pixels, Pixels) = (px(140.), px(11.));
const BAR_META: (Pixels, Pixels) = (px(90.), px(9.));

type Press = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

#[derive(IntoElement)]
pub struct Card {
    base: Stateful<Div>,
    title: SharedString,
    eyebrow: Option<SharedString>,
    size: Option<Text>,
    weight: Option<FontWeight>,
    meta: Option<AnyElement>,
    footer: Option<AnyElement>,
    bare: bool,
    trailing: Option<AnyElement>,
    cover: Option<String>,
    art: Option<Pixels>,
    art_radius: Option<Pixels>,
    match_art_height: bool,
    circle: bool,
    tint: Option<Hsla>,
    spacing: Option<Pixels>,
    explicit: bool,
    explicit_gap: Option<Pixels>,
    fill: bool,
    hovered: Option<StyleRefinement>,
    press: Option<Press>,
    loading: bool,
}

impl Card {
    #[track_caller]
    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self {
        Self {
            base: div().id(id),
            title: title.into(),
            eyebrow: None,
            size: None,
            weight: None,
            meta: None,
            footer: None,
            bare: false,
            trailing: None,
            cover: None,
            art: None,
            art_radius: None,
            match_art_height: false,
            circle: false,
            tint: None,
            spacing: None,
            explicit: false,
            explicit_gap: None,
            fill: true,
            hovered: None,
            press: None,
            loading: false,
        }
    }

    pub fn cover(mut self, cover: Option<String>) -> Self {
        self.cover = cover;
        self
    }

    pub fn art(mut self, art: Pixels) -> Self {
        self.art = Some(art);
        self
    }

    pub fn art_radius(mut self, radius: Pixels) -> Self {
        self.art_radius = Some(radius);
        self
    }

    pub fn match_art_height(mut self) -> Self {
        self.match_art_height = true;
        self
    }

    pub fn circle(mut self) -> Self {
        self.circle = true;
        self
    }

    pub fn eyebrow(mut self, eyebrow: impl Into<SharedString>) -> Self {
        self.eyebrow = Some(eyebrow.into());
        self
    }

    pub fn size(mut self, size: Text) -> Self {
        self.size = Some(size);
        self
    }

    pub fn weight(mut self, weight: FontWeight) -> Self {
        self.weight = Some(weight);
        self
    }

    pub fn spacing(mut self, spacing: Pixels) -> Self {
        self.spacing = Some(spacing);
        self
    }

    pub fn tint(mut self, tint: Hsla) -> Self {
        self.tint = Some(tint);
        self
    }

    pub fn meta(mut self, meta: impl IntoElement) -> Self {
        self.meta = Some(meta.into_any_element());
        self
    }

    pub fn bare_meta(mut self, meta: impl IntoElement) -> Self {
        self.meta = Some(meta.into_any_element());
        self.bare = true;
        self
    }

    pub fn footer(mut self, footer: impl IntoElement) -> Self {
        self.footer = Some(footer.into_any_element());
        self
    }

    pub fn explicit(mut self) -> Self {
        self.explicit = true;
        self
    }

    pub fn explicit_gap(mut self, gap: Pixels) -> Self {
        self.explicit_gap = Some(gap);
        self
    }

    pub fn trailing(mut self, trailing: impl IntoElement) -> Self {
        self.trailing = Some(trailing.into_any_element());
        self
    }

    pub fn flat(mut self) -> Self {
        self.fill = false;
        self
    }

    pub fn loading(mut self) -> Self {
        self.loading = true;
        self.fill = false;
        self
    }

    pub fn press(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.press = Some(Box::new(handler));
        self
    }
}

impl Styled for Card {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl InteractiveElement for Card {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.base.interactivity()
    }

    fn hover(mut self, f: impl FnOnce(StyleRefinement) -> StyleRefinement) -> Self {
        self.hovered = Some(f(self.hovered.take().unwrap_or_default()));
        self
    }
}

impl StatefulInteractiveElement for Card {}

impl RenderOnce for Card {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            mut base,
            title,
            eyebrow,
            size,
            weight,
            meta,
            footer,
            bare,
            trailing,
            cover,
            art,
            art_radius,
            match_art_height,
            circle,
            tint,
            spacing,
            explicit,
            explicit_gap,
            fill,
            hovered,
            press,
            loading,
        } = self;

        let theme = *cx.theme();
        let height = snapped(theme.metrics.list_row, window);
        let listed = art.is_none();
        let art = art.unwrap_or(theme.metrics.list_row - theme.metrics.pad * 2.);
        let hovered = match (hovered, fill) {
            (Some(style), _) => Some(style),
            (None, true) => Some(StyleRefinement::default().bg(theme.table_hover)),
            (None, false) => None,
        };
        let overrides = std::mem::take(base.style());

        let leading = match loading {
            true => Skeleton::new()
                .size(art)
                .when(circle, Skeleton::circle)
                .into_any_element(),
            false if circle => Avatar::new(cover).size(art).into_any_element(),
            false => Artwork::new(cover)
                .size(art)
                .when_some(art_radius, Artwork::corner_radius)
                .into_any_element(),
        };

        let mut card = base
            .flex()
            .items_center()
            .gap_3()
            .px_2()
            .rounded(theme.radius)
            .when(listed, |this| this.flex_none().h(height))
            .when_some(hovered, |this, style| this.hover(move |_| style))
            .when_some(press, |this, press| {
                this.cursor_pointer()
                    .on_click(move |event, window, cx| press(event, window, cx))
            })
            .child(div().flex_none().child(leading))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .when(match_art_height, |this| this.h(art).justify_between())
                    .when(listed, |this| this.min_w(TITLE))
                    .when_some(spacing, |this, spacing| this.gap(spacing))
                    .when_else(
                        loading,
                        |this| {
                            this.gap_2()
                                .child(Skeleton::new().w(BAR_TITLE.0).h(BAR_TITLE.1))
                                .child(Skeleton::new().w(BAR_META.0).h(BAR_META.1))
                        },
                        |this| {
                            this.children(eyebrow.map(|eyebrow| {
                                div()
                                    .text_size(theme.text(Text::Small))
                                    .text_color(theme.muted_foreground)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(upper(eyebrow))
                            }))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(explicit_gap.unwrap_or(px(6.)))
                                    .min_w_0()
                                    .text_color(tint.unwrap_or(theme.foreground))
                                    .when_some(size, |this, size| this.text_size(theme.text(size)))
                                    .child(
                                        div()
                                            .min_w_0()
                                            .truncate()
                                            .when_some(weight, |this, weight| {
                                                this.font_weight(weight)
                                            })
                                            .child(title),
                                    )
                                    .when(explicit, |this| {
                                        this.child(div().flex_none().child(ExplicitBadge::new()))
                                    }),
                            )
                            .children(meta.map(|meta| {
                                match bare {
                                    true => div().child(meta),
                                    false => div()
                                        .truncate()
                                        .text_size(theme.text(Text::Small))
                                        .text_color(theme.muted_foreground)
                                        .child(meta),
                                }
                            }))
                            .children(footer.map(|footer| div().pt_1().child(footer)))
                        },
                    ),
            )
            .children(trailing.map(|trailing| div().flex_shrink(1.).min_w_0().child(trailing)));

        card.style().refine(&overrides);
        card
    }
}
