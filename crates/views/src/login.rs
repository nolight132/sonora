// SPDX-License-Identifier: GPL-3.0-or-later

use gpui::{
    Context, Entity, FontWeight, IntoElement, ParentElement as _, Render, SharedString,
    Styled as _, Window, div, px,
};
use i18n::t;
use state::{Session, SessionState};
use ui::ActiveTheme as _;
use ui::{Button, Text};

pub struct LoginView {
    session: Entity<Session>,
}

impl LoginView {
    pub fn new(session: Entity<Session>, cx: &mut Context<Self>) -> Self {
        cx.observe(&session, |_, _, cx| cx.notify()).detach();
        Self { session }
    }
}

impl Render for LoginView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.session.read(cx).state().clone();
        let pending = self.session.read(cx).is_pending();

        let status = match &state {
            SessionState::SignedOut => t!("login-signed-out"),
            SessionState::Restoring => t!("login-restoring"),
            SessionState::Authorizing => t!("login-authorizing"),
            SessionState::SignedIn(profile) => t!("login-signed-in", name = &profile.display_name),
            SessionState::Failed(error) => SharedString::from(error.clone()),
        };

        let theme = *cx.theme();
        let status_color = if matches!(state, SessionState::Failed(_)) {
            theme.danger
        } else {
            theme.muted_foreground
        };

        let session = self.session.clone();

        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_4()
            .size_full()
            .child(
                div()
                    .child("sonora")
                    .text_size(theme.text(Text::Display))
                    .font_weight(FontWeight::BOLD),
            )
            .child(
                div()
                    .max_w(px(560.))
                    .text_center()
                    .text_size(theme.text(Text::Body))
                    .text_color(status_color)
                    .child(status),
            )
            .child(
                Button::new("sign-in")
                    .label(t!("login-sign-in"))
                    .primary()
                    .disabled(pending)
                    .on_click(move |_, _, cx| {
                        session.update(cx, |session, cx| session.sign_in(cx));
                    }),
            )
    }
}
