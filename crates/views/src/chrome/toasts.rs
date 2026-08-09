// SPDX-License-Identifier: GPL-3.0-or-later

use gpui::prelude::*;
use gpui::{Context, Entity, Render, Window, div};
use state::{Note, Toasts};
use ui::{ActiveTheme as _, Toast};

pub(crate) struct ToastStack {
    toasts: Entity<Toasts>,
}

impl ToastStack {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let toasts = Toasts::entity(cx);
        cx.observe(&toasts, |_, _, cx| cx.notify()).detach();
        Self { toasts }
    }
}

impl Render for ToastStack {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let shown = self.toasts.read(cx).shown().to_vec();
        if shown.is_empty() {
            return div();
        }

        div()
            .absolute()
            .bottom(theme.metrics.player_bar)
            .left_0()
            .right_0()
            .flex()
            .flex_col()
            .items_center()
            .gap_2()
            .pb(theme.metrics.pad)
            .children(shown.into_iter().map(|toast| {
                let id = toast.id;
                let toasts = self.toasts.clone();

                let message = match &toast.name {
                    None => i18n::lookup(&toast.key, None),
                    Some(name) => {
                        let mut args = i18n::FluentArgs::new();
                        args.set("name", i18n::Value::value(name.as_ref()));
                        i18n::lookup(&toast.key, Some(&args))
                    }
                };

                Toast::new(("toast", id), message)
                    .when_some(toast.name.clone(), Toast::strong)
                    .when(toast.note == Note::Failed, Toast::failed)
                    .on_dismiss(move |_, _, cx| {
                        toasts.update(cx, |this, cx| this.dismiss(id, cx));
                    })
            }))
    }
}
