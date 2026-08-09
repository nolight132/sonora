// SPDX-License-Identifier: GPL-3.0-or-later

mod player_bar;
mod sidebar_left;
mod sidebar_right;
mod title_bar;
mod toasts;
mod toolbar;
pub(crate) mod tools;

pub(crate) use player_bar::PlayerBar;
pub(crate) use sidebar_left::SidebarLeft;
pub(crate) use sidebar_right::SidebarRight;
pub(crate) use title_bar::{TitleBar, TitleBarEvent, TitleBarOptions};
pub(crate) use toasts::ToastStack;
pub(crate) use toolbar::{Searchable, Toolbar, Tooled};

use gpui::{App, AppContext as _, Entity, Global, Pixels, Window};
use ui::{MIN_CONTENT, Room};

pub(crate) fn cap(min: Pixels, max: Pixels, keep: Pixels, window: &Window) -> Pixels {
    let room = window.viewport_size().width - keep;
    max.min(room.max(min))
}

#[derive(Clone, Copy, Default, PartialEq)]
pub(crate) struct Chrome {
    sidebar_left: Pixels,
    sidebar_right: Pixels,
}

struct Installed(Entity<Chrome>);

impl Global for Installed {}

impl Chrome {
    pub fn entity(cx: &mut App) -> Entity<Chrome> {
        if cx.try_global::<Installed>().is_none() {
            let chrome = cx.new(|_| Chrome::default());
            cx.set_global(Installed(chrome));
        }
        cx.global::<Installed>().0.clone()
    }

    pub(crate) fn publish(left: Pixels, right: Pixels, cx: &mut App) {
        let next = Self {
            sidebar_left: left,
            sidebar_right: right,
        };
        let chrome = Self::entity(cx);
        chrome.update(cx, |chrome, cx| {
            if *chrome != next {
                *chrome = next;
                cx.notify();
            }
        });
    }

    pub fn get(cx: &App) -> Self {
        cx.try_global::<Installed>()
            .map(|installed| *installed.0.read(cx))
            .unwrap_or_default()
    }

    pub fn sidebar_right(cx: &App) -> Pixels {
        Self::get(cx).sidebar_right
    }

    pub fn content(window: &Window, cx: &App) -> Pixels {
        let chrome = Self::get(cx);
        (window.viewport_size().width - chrome.sidebar_left - chrome.sidebar_right).max(MIN_CONTENT)
    }

    pub fn room(window: &Window, cx: &App) -> Room {
        Room::of(Self::content(window, cx))
    }
}
