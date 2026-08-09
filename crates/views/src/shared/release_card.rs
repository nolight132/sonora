// SPDX-License-Identifier: GPL-3.0-or-later

use gpui::prelude::*;
use gpui::{App, Entity, FontWeight, Pixels, SharedString, Window, div};
use router::{Destination, navigate};
use spotify::Album;
use state::{Origin, Playback, PlaybackState};
use ui::{ActiveTheme as _, Card, Text};

#[derive(IntoElement)]
pub(crate) struct ReleaseCard {
    index: usize,
    album: Album,
    playback: Entity<Playback>,
    load_art: bool,
    width: Option<Pixels>,
}

impl ReleaseCard {
    pub(crate) fn new(index: usize, album: Album, playback: Entity<Playback>) -> Self {
        Self {
            index,
            album,
            playback,
            load_art: true,
            width: None,
        }
    }

    pub(crate) fn load_art(mut self, load: bool) -> Self {
        self.load_art = load;
        self
    }

    pub(crate) fn width(mut self, width: Pixels) -> Self {
        self.width = Some(width);
        self
    }
}

impl RenderOnce for ReleaseCard {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            index,
            album,
            playback,
            load_art,
            width,
        } = self;

        let theme = *cx.theme();
        let cover = match load_art {
            true => album.cover_large.clone().or_else(|| album.cover.clone()),
            false => None,
        };
        let artists = crate::shared::cells::artist_links(
            SharedString::from(format!("release-artist-{index}")),
            album.artist_refs.clone(),
            album.artists.clone(),
            theme.muted_foreground,
        )
        .text_size(theme.text(Text::Small))
        .truncate();
        let origin = Origin::Album(album.id.clone());
        let state = playback.read(cx).playing_from(&origin);
        let playing = matches!(state, Some(PlaybackState::Playing));
        let played = album.id.clone();
        let opened = SharedString::from(album.id);

        Card::new(("artist-release", index), SharedString::from(album.name))
            .tile(width.unwrap_or(theme.metrics.cover))
            .art_radius(theme.radius)
            .cover(cover)
            .weight(FontWeight::SEMIBOLD)
            .flat()
            .underline()
            .bare_meta(div().child(artists))
            .play(playing, move |_, _, cx| {
                playback.update(cx, |playback, cx| match &state {
                    Some(PlaybackState::Playing) => playback.pause(cx),
                    Some(PlaybackState::Paused) => playback.resume(cx),
                    _ => playback.play_album(&played, cx),
                });
            })
            .press(move |_, _, cx| navigate(Destination::Album(opened.clone()), cx))
    }
}
