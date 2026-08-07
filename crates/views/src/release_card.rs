// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 nolight132

use gpui::prelude::*;
use gpui::{App, FontWeight, SharedString, Window, div};
use i18n::t;
use router::{Destination, Link as _};
use spotify::{Album, ReleaseType};
use ui::{ActiveTheme as _, Artwork, Text};

pub(crate) fn release_label(kind: ReleaseType) -> SharedString {
    match kind {
        ReleaseType::Album => t!("release-album"),
        ReleaseType::Single => t!("release-single"),
        ReleaseType::Compilation => t!("release-compilation"),
        ReleaseType::Ep => t!("release-ep"),
        ReleaseType::Audiobook => t!("release-audiobook"),
        ReleaseType::Podcast => t!("release-podcast"),
    }
}

#[derive(IntoElement)]
pub(crate) struct ReleaseCard {
    index: usize,
    album: Album,
}

impl ReleaseCard {
    pub(crate) fn new(index: usize, album: Album) -> Self {
        Self { index, album }
    }
}

impl RenderOnce for ReleaseCard {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *cx.theme();
        let cover = self
            .album
            .cover_large
            .clone()
            .or_else(|| self.album.cover.clone());
        let release = release_label(self.album.release_type);
        let metadata = match self.album.year > 0 {
            true => t!("release-meta", year = self.album.year, kind = &release),
            false => release,
        };

        div()
            .id(("artist-release", self.index))
            .w(theme.metrics.cover)
            .flex()
            .flex_col()
            .gap_2()
            .cursor_pointer()
            .link(Destination::Album(self.album.id.into()))
            .child(Artwork::new(cover).size(theme.metrics.cover))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .truncate()
                            .line_height(theme.text(Text::Body))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(SharedString::from(self.album.name)),
                    )
                    .child(
                        div()
                            .text_size(theme.text(Text::Small))
                            .line_height(theme.text(Text::Small))
                            .text_color(theme.muted_foreground)
                            .child(metadata),
                    ),
            )
    }
}
