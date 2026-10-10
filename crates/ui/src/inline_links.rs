use std::rc::Rc;

use gpui::prelude::*;
use gpui::{App, Div, ElementId, Hsla, Pixels, SharedString, StyleRefinement, Window, div};

const RAMP: f32 = 6.;
const DEPTH: usize = 5;

#[derive(Clone, Debug)]
pub struct InlineLink {
    pub label: SharedString,
    pub value: Option<SharedString>,
}

impl InlineLink {
    pub fn new(label: impl Into<SharedString>, value: Option<SharedString>) -> Self {
        Self {
            label: label.into(),
            value,
        }
    }
}

type ClickHandler = Rc<dyn Fn(SharedString, &mut App) + 'static>;

#[derive(IntoElement)]
pub struct InlineLinks {
    base: Div,
    id: SharedString,
    items: Vec<InlineLink>,
    fallback: SharedString,
    color: Hsla,
    text_size: Option<Pixels>,
    clip: bool,
    on_click: Option<ClickHandler>,
}

impl InlineLinks {
    pub fn new(
        id: impl Into<SharedString>,
        items: impl IntoIterator<Item = InlineLink>,
        fallback: impl Into<SharedString>,
        color: Hsla,
    ) -> Self {
        Self {
            base: div(),
            id: id.into(),
            items: items.into_iter().collect(),
            fallback: fallback.into(),
            color,
            text_size: None,
            clip: false,
            on_click: None,
        }
    }

    pub fn text_size(mut self, text_size: Pixels) -> Self {
        self.text_size = Some(text_size);
        self
    }

    pub fn truncate(mut self) -> Self {
        self.clip = true;
        self
    }

    pub fn on_click(mut self, handler: impl Fn(SharedString, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

fn eagerness(index: usize) -> f32 {
    RAMP.powi(index.min(DEPTH) as i32)
}

impl Styled for InlineLinks {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl RenderOnce for InlineLinks {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let Self {
            mut base,
            id,
            items,
            fallback,
            color,
            text_size,
            clip,
            on_click,
        } = self;
        let empty = items.is_empty();
        let overrides = std::mem::take(base.style());

        // A clipped list ellipsizes its text rather than hiding overflow, so a text shadow
        // set on it is not cut off at the line box.
        let mut links = base
            .flex()
            .min_w_0()
            .when(!clip, |this| this.overflow_hidden())
            .text_color(color)
            .when_some(text_size, |this, text_size| this.text_size(text_size))
            .when(clip, |this| this.whitespace_nowrap())
            .when(empty, |this| match clip {
                true => this.child(div().min_w_0().text_ellipsis().child(fallback)),
                false => this.child(fallback),
            })
            .when(!empty, |this| {
                let lone = items.len() == 1;
                this.children(items.into_iter().enumerate().map(|(index, item)| {
                    let InlineLink { label, value } = item;
                    let item = div()
                        .id(ElementId::NamedInteger(id.clone(), index as u64))
                        .min_w_0()
                        .when(lone, |this| this.flex_shrink(1.))
                        .when(clip, |this| this.text_ellipsis());
                    let item = match value {
                        Some(value) => {
                            let handler = on_click.clone();
                            item.hover(|style| style.underline())
                                .cursor_pointer()
                                .when_some(handler, |this, handler| {
                                    this.on_click(move |_, _, cx| {
                                        cx.stop_propagation();
                                        handler(value.clone(), cx);
                                    })
                                })
                                .child(label)
                        }
                        None => item.child(label),
                    };

                    match lone {
                        true => item.into_any_element(),
                        false => div()
                            .flex()
                            .min_w_0()
                            .when_else(
                                clip,
                                |this| this.flex_shrink(eagerness(index)),
                                |this| this.flex_none(),
                            )
                            .when(index > 0, |this| this.child(div().flex_none().child(", ")))
                            .child(item)
                            .into_any_element(),
                    }
                }))
            });
        links.style().refine(&overrides);
        links
    }
}
