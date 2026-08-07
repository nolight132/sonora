// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 nolight132

use gpui::prelude::*;
use gpui::{Context, Entity, Render, Window, div, px};
use ui::{Button, Menu, MenuItem};

use crate::LibraryView;

pub struct ColumnPicker {
    library: Entity<LibraryView>,
    open: bool,
}

impl ColumnPicker {
    pub fn new(library: Entity<LibraryView>, cx: &mut Context<Self>) -> Self {
        cx.observe(&library, |_, _, cx| cx.notify()).detach();

        Self {
            library,
            open: false,
        }
    }
}

impl Render for ColumnPicker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let toggles = self.library.read(cx).toggles(cx);

        div()
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .child(
                Button::new("columns-toggle")
                    .icon("icons/columns-3.svg")
                    .small()
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.open = !this.open;
                        cx.notify();
                    })),
            )
            .when(self.open, |this| {
                this.child(
                    Menu::new("columns-menu")
                        .top(px(30.))
                        .right_0()
                        .w(px(190.))
                        .on_dismiss(cx.listener(|this, _, _, cx| {
                            this.open = false;
                            cx.notify();
                        }))
                        .items(toggles.into_iter().map(|toggle| {
                            let key = toggle.key;
                            MenuItem::new(key, toggle.label)
                                .selected(toggle.visible)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.library.update(cx, |library, cx| {
                                        library.toggle_column(key, cx);
                                    });
                                    cx.notify();
                                }))
                        })),
                )
            })
    }
}
