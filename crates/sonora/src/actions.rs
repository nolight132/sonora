// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 nolight132

use gpui::{App, Menu, MenuItem};
use i18n::t;
use input::{Quit, RefreshLibrary, SignOut, TogglePlayback};
use state::Sonora;

pub fn register(cx: &mut App) {
    cx.bind_keys(input::bindings());

    cx.on_action(|_: &Quit, cx: &mut App| cx.quit());

    cx.on_window_closed(|cx, _| {
        if cx.windows().is_empty() {
            cx.quit();
        }
    })
    .detach();

    cx.on_action(|_: &SignOut, cx: &mut App| {
        let session = Sonora::global(cx).session.clone();
        session.update(cx, |session, cx| session.sign_out(cx));
    });

    cx.on_action(|_: &RefreshLibrary, cx: &mut App| {
        let library = Sonora::global(cx).library.clone();
        library.update(cx, |library, cx| library.refresh(cx));
    });

    cx.on_action(|_: &TogglePlayback, cx: &mut App| {
        let playback = Sonora::global(cx).playback.clone();
        playback.update(cx, |playback, cx| playback.toggle_play(cx));
    });

    cx.set_menus(vec![Menu {
        name: "sonora".into(),
        disabled: false,
        items: vec![
            MenuItem::action(t!("app-refresh-library"), RefreshLibrary),
            MenuItem::action(t!("app-sign-out"), SignOut),
            MenuItem::separator(),
            MenuItem::action(t!("app-quit"), Quit),
        ],
    }]);
}
