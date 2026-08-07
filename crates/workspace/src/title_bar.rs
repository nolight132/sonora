// SPDX-License-Identifier: GPL-3.0-or-later

use gpui::prelude::*;
use gpui::{AnyView, Context, Entity, MouseButton, Pixels, Render};
use gpui::{Window, div, px};
use ui::WindowControls;
use ui::{ActiveTheme as _, Button};

use router::Navigation;
use state::{AppSettings, Sonora};

use crate::Sidebar;

#[cfg(target_os = "macos")]
const TITLE_BAR_LEFT_INSET: f32 = 74.;
#[cfg(not(target_os = "macos"))]
const TITLE_BAR_LEFT_INSET: f32 = 12.;

pub struct TitleBar {
    sidebar: Entity<Sidebar>,
    navigation: Entity<Navigation>,
    settings: Entity<AppSettings>,
    content: Option<AnyView>,
}

impl TitleBar {
    pub fn new(sidebar: Entity<Sidebar>, cx: &mut Context<Self>) -> Self {
        let navigation = router::trail(cx);
        let settings = Sonora::global(cx).settings.clone();

        cx.observe(&sidebar, |_, _, cx| cx.notify()).detach();
        cx.observe(&navigation, |_, _, cx| cx.notify()).detach();
        cx.observe(&settings, |_, _, cx| cx.notify()).detach();
        Self {
            sidebar,
            navigation,
            settings,
            content: None,
        }
    }

    pub fn set_content(&mut self, content: Option<AnyView>, cx: &mut Context<Self>) {
        self.content = content;
        cx.notify();
    }

    fn history(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let hover = cx.theme().sidebar_accent;
        let muted = cx.theme().muted_foreground;
        let navigation = self.navigation.read(cx);
        let (can_back, can_forward) = (navigation.can_go_back(), navigation.can_go_forward());
        let back = self.navigation.clone();
        let forward = self.navigation.clone();

        div()
            .flex()
            .flex_none()
            .items_center()
            .gap_1()
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                Button::new("history-back")
                    .ghost()
                    .icon("icons/chevron-left.svg")
                    .tint(muted)
                    .disabled(!can_back)
                    .size_8()
                    .px_0()
                    .when(can_back, |button| {
                        button
                            .hover(move |style| style.bg(hover))
                            .active(move |style| style.bg(hover))
                    })
                    .on_click(move |_, _, cx| {
                        back.update(cx, |navigation, cx| navigation.back(cx))
                    }),
            )
            .child(
                Button::new("history-forward")
                    .ghost()
                    .icon("icons/chevron-right.svg")
                    .tint(muted)
                    .disabled(!can_forward)
                    .size_8()
                    .px_0()
                    .when(can_forward, |button| {
                        button
                            .hover(move |style| style.bg(hover))
                            .active(move |style| style.bg(hover))
                    })
                    .on_click(move |_, _, cx| {
                        forward.update(cx, |navigation, cx| navigation.forward(cx))
                    }),
            )
    }

    fn toggle(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let sidebar = self.sidebar.clone();
        let icon = if sidebar.read(cx).is_open() {
            "icons/panel-right-close.svg"
        } else {
            "icons/panel-right-open.svg"
        };

        div()
            .flex()
            .flex_none()
            .items_center()
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                Button::new("sidebar-toggle")
                    .ghost()
                    .flex()
                    .small()
                    .icon(icon)
                    .on_click(move |_, _, cx| {
                        sidebar.update(cx, |sidebar, cx| sidebar.toggle(cx));
                    }),
            )
    }
}

impl Render for TitleBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let height = ui::snapped(theme.metrics.title_bar, window);
        let offset = self.sidebar.read(cx).occupied_width();
        let settings = self.settings.read(cx);
        let decorated = cfg!(not(target_os = "macos")) && settings.window_controls();
        let leading = decorated && settings.controls_on_left();

        div()
            .flex()
            .items_center()
            .w_full()
            .h(height)
            .flex_none()
            .bg(theme.background)
            .border_b_1()
            .border_color(theme.title_bar_border)
            .window_control_area(gpui::WindowControlArea::Drag)
            .on_mouse_down(MouseButton::Left, |_, window, _| {
                window.start_window_move();
            })
            .when(leading, |this| {
                this.child(div().flex_none().pl_2().child(WindowControls::new(true)))
            })
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .when(!leading, |this| this.pl(px(TITLE_BAR_LEFT_INSET)))
                    .pr_3()
                    .gap_1()
                    .when(offset > Pixels::ZERO, |this| this.w(offset))
                    .child(self.toggle(cx)),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .items_center()
                    .child(self.history(cx))
                    .children(self.content.clone())
                    .pr_3(),
            )
            .when(decorated && !leading, |this| {
                this.child(div().flex_none().pr_2().child(WindowControls::new(false)))
            })
    }
}
