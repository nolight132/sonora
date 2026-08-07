// SPDX-License-Identifier: GPL-3.0-or-later

use std::borrow::Cow;

use anyhow::Result;
use gpui::{App, AssetSource, SharedString};

macro_rules! icons {
    ($($name:literal),* $(,)?) => {
        &[
            $(
                (
                    concat!("icons/", $name, ".svg"),
                    include_bytes!(
                        concat!("../../../assets/icons/", $name, ".svg")
                    ).as_slice(),
                ),
            )*
        ]
    };
}

macro_rules! fonts {
    ($($file:literal),* $(,)?) => {
        &[
            $(
                (
                    concat!("fonts/", $file),
                    include_bytes!(
                        concat!("../../../assets/fonts/", $file)
                    ).as_slice(),
                ),
            )*
        ]
    };
}

const FONTS: &[(&str, &[u8])] = fonts!["Inter.ttf", "Inter-Italic.ttf",];

const ICONS: &[(&str, &[u8])] = icons![
    "chevron-down",
    "columns-3",
    "chevron-left",
    "chevron-right",
    "chevrons-up-down",
    "chevron-up",
    "heart",
    "house",
    "info",
    "library-big",
    "link",
    "list",
    "list-end",
    "list-plus",
    "log-out",
    "music",
    "music-2",
    "pause",
    "panel-right-close",
    "panel-right-open",
    "play",
    "play-off",
    "plus",
    "radio",
    "refresh-cw",
    "search",
    "repeat",
    "repeat-one",
    "settings",
    "shuffle",
    "skip-back",
    "skip-forward",
    "volume",
    "volume-1",
    "volume-2",
    "volume-x",
    "window-close",
    "window-maximize",
    "window-minimize",
    "window-restore",
    "x",
];

pub struct Assets;

impl Assets {
    pub fn load_fonts(&self, cx: &App) -> Result<()> {
        let embedded = FONTS
            .iter()
            .map(|(_, bytes)| Cow::Borrowed(*bytes))
            .collect();

        cx.text_system().add_fonts(embedded)
    }
}

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        let registered = ICONS.iter().chain(FONTS.iter());

        if let Some((_, bytes)) = registered.clone().find(|(name, _)| *name == path) {
            return Ok(Some(Cow::Borrowed(bytes)));
        }
        log::warn!("assets: {path} is not registered");
        Ok(None)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(ICONS
            .iter()
            .chain(FONTS.iter())
            .filter(|(name, _)| name.starts_with(path))
            .map(|(name, _)| SharedString::from(*name))
            .collect())
    }
}
