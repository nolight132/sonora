// SPDX-License-Identifier: GPL-3.0-or-later

use std::cmp::Ordering;
use std::rc::Rc;
use ui::ActiveTheme as _;

use gpui::{AnyElement, App, ClipboardItem, Entity, Hsla, Styled as _, TextAlign, WeakEntity};
use i18n::t;
use jiff::Timestamp;
use router::{Destination, navigate};
use spotify::Track;
use state::{LibraryState, Playback, PlaybackState, Sonora};
use ui::{
    Cell, ColumnSpec, GridSource, GridState, Menu, MenuItem, Scrollbar, SubmenuState, Width, clock,
};

use crate::cells::{self, ALWAYS, DATE, NUMBER, ROOMY, SNUG, TRAILING, WIDE};
use crate::hero::release_date_label;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrackField {
    Index,
    Cover,
    Title,
    Artists,
    Album,
    AddedAt,
    Plays,
    Duration,
}

pub(crate) const LIBRARY_COLUMNS: &[ColumnSpec<TrackField>] = &[
    ColumnSpec {
        field: TrackField::Index,
        key: "index",
        header: "column-index",
        align: TextAlign::Center,
        width: Width::Fixed(NUMBER),
        flush: false,
        sortable: false,
        hide_below: ALWAYS,
    },
    ColumnSpec {
        field: TrackField::Cover,
        key: "cover",
        header: "",
        align: TextAlign::Left,
        width: Width::Thumb,
        flush: true,
        sortable: false,
        hide_below: ALWAYS,
    },
    ColumnSpec {
        field: TrackField::Title,
        key: "title",
        header: "column-title",
        align: TextAlign::Left,
        width: Width::Fill(0.42),
        flush: false,
        sortable: true,
        hide_below: ALWAYS,
    },
    ColumnSpec {
        field: TrackField::Artists,
        key: "artists",
        header: "column-artist",
        align: TextAlign::Left,
        width: Width::Fill(0.29),
        flush: false,
        sortable: true,
        hide_below: ROOMY,
    },
    ColumnSpec {
        field: TrackField::Album,
        key: "album",
        header: "column-album",
        align: TextAlign::Left,
        width: Width::Fill(0.29),
        flush: false,
        sortable: true,
        hide_below: WIDE,
    },
    ColumnSpec {
        field: TrackField::AddedAt,
        key: "added-at",
        header: "column-date-added",
        align: TextAlign::Left,
        width: Width::Fixed(DATE),
        flush: false,
        sortable: true,
        hide_below: WIDE,
    },
    ColumnSpec {
        field: TrackField::Duration,
        key: "duration",
        header: "column-length",
        align: TextAlign::Right,
        width: Width::Fixed(TRAILING),
        flush: false,
        sortable: true,
        hide_below: SNUG,
    },
];

pub(crate) const ALBUM_COLUMNS: &[ColumnSpec<TrackField>] = &[
    ColumnSpec {
        field: TrackField::Index,
        key: "index",
        header: "column-index",
        align: TextAlign::Center,
        width: Width::Fixed(NUMBER),
        flush: false,
        sortable: false,
        hide_below: ALWAYS,
    },
    ColumnSpec {
        field: TrackField::Title,
        key: "title",
        header: "column-title",
        align: TextAlign::Left,
        width: Width::Fill(0.62),
        flush: false,
        sortable: true,
        hide_below: ALWAYS,
    },
    ColumnSpec {
        field: TrackField::Artists,
        key: "artists",
        header: "column-artist",
        align: TextAlign::Left,
        width: Width::Fill(0.38),
        flush: false,
        sortable: true,
        hide_below: ROOMY,
    },
    ColumnSpec {
        field: TrackField::Plays,
        key: "plays",
        header: "column-plays",
        align: TextAlign::Left,
        width: Width::Fixed(DATE),
        flush: false,
        sortable: true,
        hide_below: WIDE,
    },
    ColumnSpec {
        field: TrackField::Duration,
        key: "duration",
        header: "column-length",
        align: TextAlign::Right,
        width: Width::Fixed(TRAILING),
        flush: false,
        sortable: true,
        hide_below: SNUG,
    },
];

pub(crate) type PlaybackStatus = (Option<String>, PlaybackState);

pub(crate) fn playback_status(playback: &Entity<Playback>, cx: &App) -> PlaybackStatus {
    let playback = playback.read(cx);
    let track = playback.track().and_then(|track| track.id.clone());
    (track, playback.state().clone())
}

pub(crate) trait Tracks: 'static {
    fn tracks<'a>(&self, cx: &'a App) -> &'a [Track];
    fn is_loading(&self, cx: &App) -> bool;
}

#[derive(Clone)]
pub(crate) struct TrackMenu {
    playlist_submenu: SubmenuState,
    playlist_scrollbar: Entity<Scrollbar>,
}

impl TrackMenu {
    pub(crate) fn new(playlist_scrollbar: Entity<Scrollbar>) -> Self {
        Self {
            playlist_submenu: SubmenuState::default(),
            playlist_scrollbar,
        }
    }

    pub(crate) fn reset(&self) {
        self.playlist_submenu.reset();
    }

    pub(crate) fn for_track(&self, track: &Track, cx: &App) -> Menu {
        let playlists = match Sonora::global(cx).library.read(cx).state() {
            LibraryState::Ready { playlists, .. } => playlists.clone(),
            _ => Vec::new(),
        };
        let playlist_menu = if playlists.is_empty() {
            Menu::new("playlist-submenu")
                .w(gpui::px(220.))
                .item(MenuItem::new("no-playlists", t!("menu-no-playlists")).disabled())
        } else {
            Menu::new("playlist-submenu")
                .w(gpui::px(220.))
                .max_h(gpui::px(360.))
                .scrollbar(self.playlist_scrollbar.clone())
                .item(
                    MenuItem::new("new-playlist", t!("menu-new-playlist"))
                        .icon("icons/plus.svg")
                        .disabled(),
                )
                .item(MenuItem::separator("playlist-separator"))
                .items(playlists.into_iter().map(|playlist| {
                    MenuItem::new(format!("playlist-{}", playlist.id), playlist.name)
                        .artwork(playlist.cover)
                        .disabled()
                }))
        };
        let copy = match track.id.clone() {
            Some(id) => MenuItem::new("copy-track-link", t!("menu-copy-link"))
                .icon("icons/link.svg")
                .on_click(move |_, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(format!(
                        "https://open.spotify.com/track/{id}"
                    )));
                }),
            None => MenuItem::new("copy-track-link", t!("menu-copy-link"))
                .icon("icons/link.svg")
                .disabled(),
        };
        let queue = match track.playable {
            true => {
                let track = track.clone();
                MenuItem::new("add-to-queue", t!("menu-add-to-queue"))
                    .icon("icons/list-end.svg")
                    .on_click(move |_, _, cx| {
                        let queue = Sonora::global(cx).queue.clone();
                        queue.update(cx, |queue, cx| queue.append(track.clone(), cx));
                    })
            }
            false => MenuItem::new("add-to-queue", t!("menu-add-to-queue"))
                .icon("icons/list-end.svg")
                .disabled(),
        };
        let details = match track.id.clone() {
            Some(id) => MenuItem::new("view-details", t!("menu-view-details"))
                .icon("icons/info.svg")
                .on_click(move |_, _, cx| navigate(Destination::Song(id.clone().into()), cx)),
            None => MenuItem::new("view-details", t!("menu-view-details"))
                .icon("icons/info.svg")
                .disabled(),
        };

        Menu::new("track-context-menu")
            .relative()
            .w(gpui::px(210.))
            .item(
                MenuItem::new("add-to-playlist", t!("menu-add-to-playlist"))
                    .icon("icons/list-plus.svg")
                    .submenu(playlist_menu, self.playlist_submenu.clone()),
            )
            .item(
                MenuItem::new("toggle-library", t!("menu-toggle-library"))
                    .icon("icons/heart.svg")
                    .disabled(),
            )
            .item(queue)
            .item(
                MenuItem::new("song-radio", t!("menu-song-radio"))
                    .icon("icons/radio.svg")
                    .disabled(),
            )
            .item(details)
            .item(copy)
    }
}

pub(crate) fn ordered(table: &Entity<GridState<TrackSource>>, cx: &App) -> Vec<Track> {
    let state = table.read(cx);
    let delegate = state.delegate();

    (0..delegate.row_count())
        .filter_map(|display| delegate.source().at(delegate.row(display), cx))
        .collect()
}

pub(crate) struct TrackSource {
    columns: &'static [ColumnSpec<TrackField>],
    provider: Rc<dyn Tracks>,
    playback: Entity<Playback>,
    menu: TrackMenu,
    table: Option<WeakEntity<GridState<TrackSource>>>,
}

impl TrackSource {
    pub(crate) fn new(
        columns: &'static [ColumnSpec<TrackField>],
        provider: impl Tracks,
        playback: Entity<Playback>,
        playlist_scrollbar: Entity<Scrollbar>,
    ) -> Self {
        Self {
            columns,
            provider: Rc::new(provider),
            playback,
            menu: TrackMenu::new(playlist_scrollbar),
            table: None,
        }
    }

    pub(crate) fn table(mut self, table: WeakEntity<GridState<TrackSource>>) -> Self {
        self.table = Some(table);
        self
    }

    fn artist_cell(&self, cell: &Cell<TrackField>, track: &Track, color: Hsla) -> AnyElement {
        cells::artists(
            cell,
            track.artist_refs.clone(),
            track.artists.clone(),
            color,
        )
    }

    fn album_cell(&self, cell: &Cell<TrackField>, track: &Track, color: Hsla) -> AnyElement {
        let Some(album) = track.album_id.clone() else {
            return cells::dim(cell, track.album.clone(), color);
        };

        cells::link(
            cell,
            "album",
            track.album.clone(),
            color,
            Destination::Album(album.into()),
        )
    }

    fn index_cell(&self, cell: &Cell<TrackField>, track: &Track, cx: &App) -> AnyElement {
        let state = self.now_playing(track, cx);
        let (preload, press) = match track.playable {
            false => (None, None),
            true => {
                let playback = self.playback.clone();
                let preload_track = track.clone();
                let preload: Option<Box<dyn Fn(&mut App)>> = Some(Box::new(move |cx| {
                    playback.update(cx, |playback, _| playback.preload(&preload_track));
                }));
                let provider = self.provider.clone();
                let table = self.table.clone();
                let row = cell.row;
                let display = cell.display;
                let press =
                    cells::toggle(
                        &self.playback,
                        state.clone(),
                        move |playback, cx| match table.as_ref().and_then(|table| table.upgrade()) {
                            Some(table) => playback.start(ordered(&table, cx), display, cx),
                            None => playback.start(provider.tracks(cx).to_vec(), row, cx),
                        },
                    );
                (preload, press)
            }
        };

        cells::index(cell, state, track.playable, preload, press, cx)
    }

    fn now_playing(&self, track: &Track, cx: &App) -> Option<PlaybackState> {
        let playback = self.playback.read(cx);
        let current = playback.track()?;
        (current.id.is_some() && current.id == track.id).then(|| playback.state().clone())
    }

    pub(crate) fn at(&self, row: usize, cx: &App) -> Option<Track> {
        self.provider.tracks(cx).get(row).cloned()
    }
}

impl GridSource for TrackSource {
    type Field = TrackField;

    fn columns(&self) -> &'static [ColumnSpec<TrackField>] {
        self.columns
    }

    fn rows(&self, cx: &App) -> usize {
        self.provider.tracks(cx).len()
    }

    fn matches(&self, row: usize, query: &str, cx: &App) -> bool {
        self.at(row, cx).is_some_and(|track| {
            let haystack = format!("{} {} {}", track.name, track.artists, track.album);
            haystack.to_lowercase().contains(query)
        })
    }

    fn playing(&self, row: usize, cx: &App) -> bool {
        self.provider
            .tracks(cx)
            .get(row)
            .is_some_and(|track| self.now_playing(track, cx).is_some())
    }

    fn is_loading(&self, cx: &App) -> bool {
        self.provider.is_loading(cx)
    }

    fn cell(&self, cell: Cell<TrackField>, cx: &mut App) -> AnyElement {
        let muted = cx.theme().muted_foreground;

        let Some(track) = self.provider.tracks(cx).get(cell.row) else {
            return cells::blank(&cell);
        };

        if cell.field == TrackField::Index {
            return self.index_cell(&cell, track, cx);
        }
        let faded = muted.opacity(0.5);
        let (title, detail) = match track.playable {
            true => (None, muted),
            false => (Some(faded), faded),
        };

        match cell.field {
            TrackField::Cover => cells::artwork(&cell, track.cover.clone()),
            TrackField::Title => cells::title(
                &cell,
                track.name.clone(),
                title,
                track.explicit,
                track.id.clone(),
            ),
            TrackField::Artists => self.artist_cell(&cell, track, detail),
            TrackField::Album => self.album_cell(&cell, track, detail),
            TrackField::AddedAt => cells::dim(
                &cell,
                track
                    .added_at
                    .and_then(|seconds| Timestamp::new(seconds, 0).ok())
                    .map(|timestamp| {
                        release_date_label(&timestamp.strftime("%Y-%m-%d").to_string())
                    })
                    .unwrap_or_default(),
                detail,
            ),
            TrackField::Plays => cells::dim(
                &cell,
                track.playcount.map(cells::count).unwrap_or_default(),
                detail,
            ),
            TrackField::Duration => cells::dim(&cell, clock(track.duration), detail),
            TrackField::Index => cells::blank(&cell),
        }
    }

    fn context_menu(&self, row: usize, cx: &App) -> Option<Menu> {
        let track = self.provider.tracks(cx).get(row)?;
        Some(self.menu.for_track(track, cx))
    }

    fn context_menu_will_open(&self, _row: usize, _cx: &App) {
        self.menu.reset();
    }

    fn compare(&self, field: TrackField, a: usize, b: usize, cx: &App) -> Ordering {
        let tracks = self.provider.tracks(cx);
        let text = |index: usize, pick: fn(&Track) -> &String| {
            tracks
                .get(index)
                .map(|track| pick(track).to_lowercase())
                .unwrap_or_default()
        };

        match field {
            TrackField::Title => text(a, |track| &track.name).cmp(&text(b, |track| &track.name)),
            TrackField::Artists => {
                text(a, |track| &track.artists).cmp(&text(b, |track| &track.artists))
            }
            TrackField::Album => text(a, |track| &track.album).cmp(&text(b, |track| &track.album)),
            TrackField::AddedAt => tracks
                .get(a)
                .and_then(|track| track.added_at)
                .cmp(&tracks.get(b).and_then(|track| track.added_at)),
            TrackField::Plays => tracks
                .get(a)
                .and_then(|track| track.playcount)
                .cmp(&tracks.get(b).and_then(|track| track.playcount)),
            TrackField::Duration => tracks
                .get(a)
                .map(|track| track.duration)
                .cmp(&tracks.get(b).map(|track| track.duration)),
            TrackField::Index | TrackField::Cover => a.cmp(&b),
        }
    }
}
