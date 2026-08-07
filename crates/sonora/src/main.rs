// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 nolight132

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actions;
mod assets;
mod http;

use std::sync::Arc;

use gpui::{
    App, AppContext as _, Bounds, Entity, TitlebarOptions, WindowBounds, WindowOptions, point, px,
    size,
};
use router::Destination;
use state::{Library, Playback, Queue, Session, Sonora};
use ui::ActiveTheme as _;
use views::Root;

fn main() {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("warn,symphonia=error"),
    )
    .format_timestamp(None)
    .format_module_path(false)
    .init();

    let io = match state::Io::new() {
        Ok(io) => io,
        Err(error) => {
            eprintln!("sonora: cannot start runtime: {error:#}");
            return;
        }
    };

    gpui_platform::application()
        .with_assets(assets::Assets)
        .with_http_client(Arc::new(http::Client::new(io.handle())))
        .run(move |cx: &mut App| {
            if let Err(error) = assets::Assets.load_fonts(cx) {
                log::error!("sonora: cannot load bundled fonts: {error:#}");
            }

            state::init(cx, io);
            router::init(Destination::Home, cx);
            let (look, overrides, language) = {
                let settings = Sonora::global(cx).settings.read(cx);
                (
                    settings.look(),
                    settings.theme_overrides().clone(),
                    settings.language().to_owned(),
                )
            };
            i18n::set(i18n::resolve(&language));
            ui::Theme::init(look, &overrides, cx);

            actions::register(cx);

            let Sonora {
                session,
                library,
                playback,
                queue,
                settings: _,
            } = Sonora::global(cx);
            let (session, library, playback, queue) = (
                session.clone(),
                library.clone(),
                playback.clone(),
                queue.clone(),
            );

            open_window(session.clone(), library, playback, queue, cx);
            session.update(cx, |session, cx| session.restore(cx));

            cx.activate(true);
        });
}

fn open_window(
    session: Entity<Session>,
    library: Entity<Library>,
    playback: Entity<Playback>,
    queue: Entity<Queue>,
    cx: &mut App,
) {
    let bounds = Bounds::centered(None, size(px(920.), px(640.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("sonora".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(
                    px(9.),
                    px(if cfg!(target_os = "macos") { 12. } else { 9. }),
                )),
            }),
            is_movable: true,
            is_resizable: true,
            app_id: Some("sonora".into()),
            window_min_size: Some(size(px(480.), px(400.))),
            ..Default::default()
        },
        |window, cx| {
            window.set_rem_size(cx.theme().font_size);
            cx.new(|cx| Root::new(session, library, playback, queue, window, cx))
        },
    )
    .expect("failed to open window");
}
