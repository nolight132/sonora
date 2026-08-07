// SPDX-License-Identifier: GPL-3.0-or-later

use std::cmp::Ordering;
use ui::ActiveTheme as _;

use gpui::{AnyElement, App, Entity, TextAlign};
use spotify::Playlist;
use state::{Library, LibraryState, Origin, Playback};
use ui::{Cell, ColumnSpec, GridSource, Width};

use crate::cells::{self, ALWAYS, NUMBER, ROOMY, SNUG, TRAILING};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PlaylistField {
    Index,
    Cover,
    Name,
    Owner,
    TrackCount,
}

pub(super) const COLUMNS: &[ColumnSpec<PlaylistField>] = &[
    ColumnSpec {
        field: PlaylistField::Index,
        key: "index",
        header: "column-index",
        align: TextAlign::Center,
        width: Width::Fixed(NUMBER),
        flush: false,
        sortable: false,
        hide_below: ALWAYS,
    },
    ColumnSpec {
        field: PlaylistField::Cover,
        key: "cover",
        header: "",
        align: TextAlign::Left,
        width: Width::Thumb,
        flush: true,
        sortable: false,
        hide_below: ALWAYS,
    },
    ColumnSpec {
        field: PlaylistField::Name,
        key: "name",
        header: "column-name",
        align: TextAlign::Left,
        width: Width::Fill(0.55),
        flush: false,
        sortable: true,
        hide_below: ALWAYS,
    },
    ColumnSpec {
        field: PlaylistField::Owner,
        key: "owner",
        header: "column-owner",
        align: TextAlign::Left,
        width: Width::Fill(0.45),
        flush: false,
        sortable: true,
        hide_below: ROOMY,
    },
    ColumnSpec {
        field: PlaylistField::TrackCount,
        key: "tracks",
        header: "column-tracks",
        align: TextAlign::Right,
        width: Width::Fixed(TRAILING),
        flush: false,
        sortable: true,
        hide_below: SNUG,
    },
];

pub(super) struct PlaylistSource {
    library: Entity<Library>,
    playback: Entity<Playback>,
}

impl PlaylistSource {
    pub(super) fn new(library: Entity<Library>, playback: Entity<Playback>) -> Self {
        Self { library, playback }
    }

    fn index_cell(&self, cell: &Cell<PlaylistField>, playlist: &Playlist, cx: &App) -> AnyElement {
        let origin = Origin::Playlist(playlist.id.clone());
        let state = self.playback.read(cx).playing_from(&origin);
        let id = playlist.id.clone();
        let press = cells::toggle(&self.playback, state.clone(), move |playback, cx| {
            playback.play_playlist(&id, cx)
        });

        cells::index(cell, state, true, None, press, cx)
    }

    pub(super) fn at(&self, row: usize, cx: &App) -> Option<Playlist> {
        self.playlists(cx).get(row).cloned()
    }

    fn playlists<'a>(&self, cx: &'a App) -> &'a [Playlist] {
        match self.library.read(cx).state() {
            LibraryState::Ready { playlists, .. } => playlists.as_slice(),
            _ => &[],
        }
    }
}

impl GridSource for PlaylistSource {
    type Field = PlaylistField;

    fn columns(&self) -> &'static [ColumnSpec<PlaylistField>] {
        COLUMNS
    }

    fn rows(&self, cx: &App) -> usize {
        self.playlists(cx).len()
    }

    fn matches(&self, row: usize, query: &str, cx: &App) -> bool {
        self.at(row, cx).is_some_and(|playlist| {
            let haystack = format!("{} {}", playlist.name, playlist.owner);
            haystack.to_lowercase().contains(query)
        })
    }

    fn playing(&self, row: usize, cx: &App) -> bool {
        self.playlists(cx).get(row).is_some_and(|playlist| {
            let origin = Origin::Playlist(playlist.id.clone());
            self.playback.read(cx).playing_from(&origin).is_some()
        })
    }

    fn is_loading(&self, cx: &App) -> bool {
        self.library.read(cx).is_loading()
    }

    fn cell(&self, cell: Cell<PlaylistField>, cx: &mut App) -> AnyElement {
        let muted = cx.theme().muted_foreground;

        let Some(playlist) = self.playlists(cx).get(cell.row) else {
            return cells::blank(&cell);
        };

        if cell.field == PlaylistField::Index {
            return self.index_cell(&cell, playlist, cx);
        }

        match cell.field {
            PlaylistField::Cover => cells::artwork(&cell, playlist.cover.clone()),
            PlaylistField::Name => cells::text(&cell, playlist.name.clone()),
            PlaylistField::Owner => cells::dim(&cell, playlist.owner.clone(), muted),
            PlaylistField::TrackCount => {
                cells::dim(&cell, format!("{}", playlist.track_count), muted)
            }
            PlaylistField::Index => cells::blank(&cell),
        }
    }

    fn compare(&self, field: PlaylistField, a: usize, b: usize, cx: &App) -> Ordering {
        let playlists = self.playlists(cx);
        let text = |index: usize, pick: fn(&Playlist) -> &String| {
            playlists
                .get(index)
                .map(|playlist| pick(playlist).to_lowercase())
                .unwrap_or_default()
        };

        match field {
            PlaylistField::Name => {
                text(a, |playlist| &playlist.name).cmp(&text(b, |playlist| &playlist.name))
            }
            PlaylistField::Owner => {
                text(a, |playlist| &playlist.owner).cmp(&text(b, |playlist| &playlist.owner))
            }
            PlaylistField::TrackCount => playlists
                .get(a)
                .map(|playlist| playlist.track_count)
                .cmp(&playlists.get(b).map(|playlist| playlist.track_count)),
            PlaylistField::Index | PlaylistField::Cover => a.cmp(&b),
        }
    }
}
