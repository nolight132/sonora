// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 nolight132

use gpui::prelude::*;
use gpui::{AnyView, App, Context, Entity, Pixels, Render, SharedString, Window, div, px};
use input::{Dismiss, Input};
use ui::Button;

const WIDEST: Pixels = px(280.);

type Apply = Box<dyn Fn(&str, &mut App)>;

pub trait Searchable: 'static {
    fn search(&mut self, query: &str, cx: &mut Context<Self>)
    where
        Self: Sized;

    fn hint() -> SharedString {
        "common-search".into()
    }
}

pub struct Filter {
    input: Entity<Input>,
    apply: Option<Apply>,
    actions: Option<AnyView>,
    open: bool,
}

impl Filter {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| {
            Input::new(String::new(), cx)
                .icon("icons/search.svg")
                .compact()
        });

        cx.observe(&input, |this, input, cx| {
            let query = input.read(cx).text().to_owned();
            if let Some(apply) = &this.apply {
                apply(&query, cx);
            }
            cx.notify();
        })
        .detach();

        Self {
            input,
            apply: None,
            actions: None,
            open: false,
        }
    }

    pub fn bind<V: Searchable>(&mut self, view: &Entity<V>, cx: &mut Context<Self>) {
        self.clear(cx);

        let target = view.downgrade();

        self.apply = Some(Box::new(move |query, cx| {
            let query = query.to_owned();
            target.update(cx, |view, cx| view.search(&query, cx)).ok();
        }));
        self.reset(V::hint(), cx);
    }

    pub fn set_actions(&mut self, actions: Option<AnyView>, cx: &mut Context<Self>) {
        self.actions = actions;
        cx.notify();
    }

    pub fn release(&mut self, cx: &mut Context<Self>) {
        self.clear(cx);
        self.reset("common-search".into(), cx);
    }

    pub fn focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.apply.is_none() {
            return;
        }

        self.open = true;
        self.input.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.open {
            true => self.close(cx),
            false => self.focus(window, cx),
        }
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        self.open = false;
        self.wipe(cx);
        cx.notify();
    }

    fn clear(&mut self, cx: &mut Context<Self>) {
        if let Some(apply) = self.apply.take() {
            apply("", cx);
        }
        self.open = false;
    }

    fn wipe(&mut self, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| input.set_text("", cx));
    }

    fn reset(&mut self, hint: SharedString, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| {
            input.set_hint(hint, cx);
            input.set_text("", cx);
        });
        cx.notify();
    }
}

impl Render for Filter {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_1()
            .min_w_0()
            .items_center()
            .justify_end()
            .gap_1()
            .on_action(cx.listener(|this, _: &Dismiss, _, cx| this.close(cx)))
            .children(self.actions.clone())
            .when(self.apply.is_some(), |this| {
                this.when(self.open, |this| {
                    this.child(
                        div()
                            .flex()
                            .flex_1()
                            .min_w_0()
                            .max_w(WIDEST)
                            .child(self.input.clone()),
                    )
                })
                .child(
                    Button::new("filter-toggle")
                        .icon(match self.open {
                            true => "icons/x.svg",
                            false => "icons/search.svg",
                        })
                        .small()
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| this.toggle(window, cx))),
                )
            })
    }
}
