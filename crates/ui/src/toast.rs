// SPDX-License-Identifier: GPL-3.0-or-later

use gpui::prelude::*;
use gpui::{
    App, ClickEvent, Div, ElementId, FontWeight, Pixels, SharedString, Window, div, px, svg,
};

use crate::button::Button;
use crate::metrics::Text;
use crate::theme::ActiveTheme as _;

const ICON: Pixels = px(16.);
const REACH: Pixels = px(420.);

type Press = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

#[derive(IntoElement)]
pub struct Toast {
    id: ElementId,
    message: SharedString,
    strong: Option<SharedString>,
    failed: bool,
    dismiss: Option<Press>,
}

impl Toast {
    #[track_caller]
    pub fn new(id: impl Into<ElementId>, message: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            message: message.into(),
            strong: None,
            failed: false,
            dismiss: None,
        }
    }

    pub fn failed(mut self) -> Self {
        self.failed = true;
        self
    }

    pub fn strong(mut self, name: impl Into<SharedString>) -> Self {
        self.strong = Some(name.into());
        self
    }

    pub fn on_dismiss(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.dismiss = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for Toast {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            id,
            message,
            strong,
            failed,
            dismiss,
        } = self;

        let theme = *cx.theme();
        let (tint, icon) = match failed {
            true => (theme.danger, "icons/circle-alert.svg"),
            false => (theme.primary, "icons/circle-check.svg"),
        };

        div()
            .id(id)
            .flex()
            .items_center()
            .justify_between()
            .gap_4()
            .max_w(REACH)
            .py(theme.metrics.pad)
            .pl(theme.metrics.pad * 2)
            .pr(theme.metrics.pad)
            .rounded(theme.radius)
            .border_1()
            .shadow_md()
            .border_color(theme.border)
            .bg(theme.popover)
            .text_size(theme.text(Text::Small))
            .text_color(theme.foreground)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .min_w_0()
                    .child(svg().path(icon).size(ICON).flex_none().text_color(tint))
                    .child(said(message, strong)),
            )
            .children(dismiss.map(|dismiss| {
                Button::new("dismiss-toast")
                    .ghost()
                    .small()
                    .icon("icons/x.svg")
                    .on_click(move |event, window, cx| dismiss(event, window, cx))
            }))
    }
}

fn said(message: SharedString, strong: Option<SharedString>) -> Div {
    let split = strong
        .as_ref()
        .and_then(|name| message.find(name.as_ref()).map(|at| (at, name)));

    let Some((at, name)) = split else {
        return div().min_w_0().truncate().child(message);
    };

    div()
        .flex()
        .min_w_0()
        .child(
            div()
                .flex_none()
                .child(SharedString::from(message[..at].to_owned())),
        )
        .child(
            div()
                .min_w_0()
                .truncate()
                .font_weight(FontWeight::BOLD)
                .child(name.clone()),
        )
        .child(
            div()
                .flex_none()
                .child(SharedString::from(message[at + name.len()..].to_owned())),
        )
}
