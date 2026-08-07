// SPDX-License-Identifier: GPL-3.0-or-later

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{App, Hsla, MouseButton, Pixels, SharedString, Window, div};

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

impl RenderOnce for InlineLinks {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let Self {
            id,
            items,
            fallback,
            color,
            text_size,
            clip,
            on_click,
        } = self;
        let empty = items.is_empty();
        let id = id.to_string();

        div()
            .flex()
            .min_w_0()
            .overflow_hidden()
            .text_color(color)
            .when_some(text_size, |this, text_size| this.text_size(text_size))
            .when(clip, |this| this.whitespace_nowrap())
            .when(empty, |this| match clip {
                true => this.child(div().min_w_0().truncate().child(fallback)),
                false => this.child(fallback),
            })
            .when(!empty, |this| {
                this.children(items.into_iter().enumerate().map(|(index, item)| {
                    let InlineLink { label, value } = item;
                    let item = div()
                        .id(SharedString::from(format!("{id}-{index}")))
                        .min_w_0()
                        .when(clip, |this| this.truncate());
                    let item = match value {
                        Some(value) => {
                            let handler = on_click.clone();
                            item.hover(|style| style.underline())
                                .cursor_pointer()
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .when_some(handler, |this, handler| {
                                    this.on_click(move |_, _, cx| handler(value.clone(), cx))
                                })
                                .child(label)
                        }
                        None => item.child(label),
                    };

                    div()
                        .flex()
                        .min_w_0()
                        .when(!clip, |this| this.flex_none())
                        .when(index > 0, |this| this.child(div().flex_none().child(", ")))
                        .child(item)
                }))
            })
    }
}
