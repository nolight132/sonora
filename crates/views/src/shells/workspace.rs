// SPDX-License-Identifier: GPL-3.0-or-later

use gpui::prelude::*;
use gpui::{AnyView, App, Context, Entity, FocusHandle, Render};
use gpui::{Window, div};
use input::WORKSPACE_CONTEXT;
use state::{Playback, Queue};
use ui::Dismiss;

use crate::chrome::{Chrome, PlayerBar, SidebarLeft, SidebarRight, TitleBarOptions, ToastStack};
use crate::shared::playlist_editor::PlaylistEditor;
use crate::shells::Shell;

pub(crate) struct Workspace {
    sidebar: Entity<SidebarLeft>,
    player_bar: Entity<PlayerBar>,
    sidebar_right: Entity<SidebarRight>,
    playlist_editor: Entity<PlaylistEditor>,
    toasts: Entity<ToastStack>,
    content: AnyView,
    focus: FocusHandle,
}

impl Workspace {
    pub fn new(
        playback: Entity<Playback>,
        queue: Entity<Queue>,
        content: AnyView,
        cx: &mut Context<Self>,
    ) -> Self {
        let sidebar = cx.new(SidebarLeft::new);
        let sidebar_right = cx.new(|cx| SidebarRight::new(queue.clone(), playback.clone(), cx));
        let player_bar =
            cx.new(|cx| PlayerBar::with_sidebar_right(playback, queue, sidebar_right.clone(), cx));

        Self {
            sidebar,
            player_bar,
            sidebar_right,
            playlist_editor: PlaylistEditor::entity(cx),
            toasts: cx.new(ToastStack::new),
            content,
            focus: cx.focus_handle(),
        }
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.focus, cx);
    }

    pub fn toggle_sidebar(&self, cx: &mut Context<Self>) {
        self.sidebar.update(cx, |sidebar, cx| sidebar.toggle(cx));
    }

    #[allow(dead_code)]
    pub fn content(&self) -> &AnyView {
        &self.content
    }

    pub fn set_content(&mut self, content: AnyView, cx: &mut Context<Self>) {
        self.content = content;
        cx.notify();
    }

    #[allow(dead_code)]
    pub fn player_bar(&self) -> &Entity<PlayerBar> {
        &self.player_bar
    }

    fn close_queue(&mut self, cx: &mut Context<Self>) {
        if self.sidebar_right.read(cx).is_open() {
            self.sidebar_right.update(cx, |panel, cx| panel.close(cx));
        }
    }
}

impl Shell for Workspace {
    fn title_bar(&self, content: Option<AnyView>, cx: &App) -> TitleBarOptions {
        let sidebar = self.sidebar.read(cx);

        TitleBarOptions {
            navigation: true,
            sidebar_open: sidebar.is_open(),
            offset: sidebar.occupied_width(),
            content,
        }
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.adapt(window, cx));
        let left = self.sidebar.read(cx).occupied_width();
        let right = self.sidebar_right.read(cx).occupied_width(window);
        Chrome::publish(left, right, cx);
        let covered = self.sidebar_right.read(cx).covers_content(window);
        let overlay = self.sidebar.read(cx).overlays();

        div()
            .relative()
            .flex()
            .flex_col()
            .w_full()
            .flex_1()
            .min_h_0()
            .key_context(WORKSPACE_CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &Dismiss, _, cx| this.close_queue(cx)))
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
                    .child(self.sidebar_right.clone())
                    .when(overlay, |this| this.child(self.sidebar.clone())),
            )
            .child(
                div()
                    .relative()
                    .child(self.player_bar.clone())
                    .child(self.toasts.clone()),
            )
            .child(self.playlist_editor.clone())
    }
}
