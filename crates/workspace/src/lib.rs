// SPDX-License-Identifier: GPL-3.0-or-later

mod chrome;
mod player_bar;
mod searchable;
mod sidebar_left;
mod sidebar_right;
mod title_bar;

pub use chrome::Chrome;
pub use player_bar::PlayerBar;
pub use searchable::{Filter, Searchable};
pub use sidebar_left::Sidebar;
pub use title_bar::TitleBar;

use gpui::prelude::*;
use gpui::{AnyView, App, Context, Entity, FocusHandle, Render};
use gpui::{Window, div};
use input::{Dismiss, WORKSPACE_CONTEXT};
use sidebar_right::QueuePanel;
use state::{Playback, Queue};

pub struct Workspace {
    title_bar: Entity<TitleBar>,
    sidebar: Entity<Sidebar>,
    player_bar: Entity<PlayerBar>,
    queue_panel: Entity<QueuePanel>,
    content: AnyView,
    focus: FocusHandle,
}

impl Workspace {
    pub fn new(
        sidebar: Entity<Sidebar>,
        playback: Entity<Playback>,
        queue: Entity<Queue>,
        content: AnyView,
        cx: &mut Context<Self>,
    ) -> Self {
        let title_bar = cx.new(|cx| TitleBar::new(sidebar.clone(), cx));
        let queue_panel =
            cx.new(|cx| QueuePanel::new(queue.clone(), playback.clone(), sidebar.clone(), cx));
        let player_bar =
            cx.new(|cx| PlayerBar::with_queue_panel(playback, queue, queue_panel.clone(), cx));

        Self {
            title_bar,
            sidebar,
            player_bar,
            queue_panel,
            content,
            focus: cx.focus_handle(),
        }
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.focus, cx);
    }

    pub fn content(&self) -> &AnyView {
        &self.content
    }

    pub fn set_content(&mut self, content: AnyView, cx: &mut Context<Self>) {
        self.content = content;
        cx.notify();
    }

    pub fn set_toolbar(&mut self, toolbar: Option<AnyView>, cx: &mut Context<Self>) {
        self.title_bar
            .update(cx, |bar, cx| bar.set_content(toolbar, cx));
    }

    pub fn player_bar(&self) -> &Entity<PlayerBar> {
        &self.player_bar
    }

    fn close_queue(&mut self, cx: &mut Context<Self>) {
        if self.queue_panel.read(cx).is_open() {
            self.queue_panel.update(cx, |panel, cx| panel.close(cx));
        }
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.adapt(window, cx));
        let sidebar = self.sidebar.read(cx).occupied_width();
        let queue = self.queue_panel.read(cx).occupied_width(window, cx);
        Chrome::publish(sidebar, queue, cx);
        let covered = self.queue_panel.read(cx).covers_content(window, cx);
        let overlay = self.sidebar.read(cx).overlays();

        div()
            .flex()
            .flex_col()
            .size_full()
            .key_context(WORKSPACE_CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &Dismiss, _, cx| this.close_queue(cx)))
            .child(self.title_bar.clone())
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .when(!overlay, |this| this.child(self.sidebar.clone()))
                    .child(
                        div()
                            .relative()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .when(covered, |this| this.hidden())
                            .child(self.content.clone()),
                    )
                    .child(self.queue_panel.clone())
                    .when(overlay, |this| this.child(self.sidebar.clone())),
            )
            .child(self.player_bar.clone())
    }
}
