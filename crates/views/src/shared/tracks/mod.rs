// SPDX-License-Identifier: GPL-3.0-or-later

mod columns;
mod sieve;
mod sort;

use std::cmp::Ordering;
use std::rc::Rc;
use ui::ActiveTheme as _;

use crate::shared::menu::{ItemMenu, TrackColumns};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, Entity, Hsla, InteractiveElement as _, IntoElement as _, SharedString,
    Styled as _, WeakEntity,
};
use jiff::Timestamp;
use router::Destination;
use spotify::Track;
use state::{Detail, Library, Playback, PlaybackState};
use ui::{Button, Cell, ColumnSpec, GridSource, GridState, Menu, ROW_GROUP, Scrollbar, clock};

use crate::shared::cells;
use crate::shared::hero::release_date_label;

pub(crate) use columns::{ALBUM_COLUMNS, ARTIST_COLUMNS, LIBRARY_COLUMNS, TrackField};
pub(crate) use sieve::TrackSieve;
pub(crate) use sort::initial;

use sort::hits;

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
    is_liked: Option<Entity<Library>>,
    album: Option<Entity<Detail>>,
    playlist: Option<Entity<Detail>>,
    menu: ItemMenu,
    table: Option<WeakEntity<GridState<TrackSource>>>,
    sieve: TrackSieve,
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
            is_liked: None,
            album: None,
            playlist: None,
            menu: ItemMenu::new(playlist_scrollbar),
            table: None,
            sieve: TrackSieve::default(),
        }
    }

    pub(crate) fn sieve(&self) -> TrackSieve {
        self.sieve
    }

    pub(crate) fn set_sieve(&mut self, sieve: TrackSieve) {
        self.sieve = sieve;
    }

    pub(crate) fn extent(&self, query: &str, cx: &App) -> Option<(f32, f32)> {
        let open = TrackSieve {
            duration: None,
            ..self.sieve
        };
        let mut low = f32::MAX;
        let mut high = f32::MIN;
        for track in self.provider.tracks(cx) {
            if !open.keeps(track) || !hits(track, query) {
                continue;
            }
            let seconds = track.duration.as_secs_f32();
            low = low.min(seconds);
            high = high.max(seconds);
        }
        (low <= high).then_some((low, high))
    }

    pub(crate) fn table(mut self, table: WeakEntity<GridState<TrackSource>>) -> Self {
        self.table = Some(table);
        self
    }

    pub(crate) fn with_liked(mut self, library: Entity<Library>) -> Self {
        self.is_liked = Some(library);
        self
    }

    pub(crate) fn with_playlist(mut self, detail: Entity<Detail>) -> Self {
        self.playlist = Some(detail);
        self
    }

    pub(crate) fn with_album(mut self, detail: Entity<Detail>) -> Self {
        self.album = Some(detail);
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

    fn title_cell(
        &self,
        cell: &Cell<TrackField>,
        track: &Track,
        color: Option<Hsla>,
        cx: &App,
    ) -> AnyElement {
        let press: Option<Box<dyn Fn(&mut App)>> = match track.playable {
            true => {
                let playback = self.playback.clone();
                let provider = self.provider.clone();
                let table = self.table.clone();
                let row = cell.row;
                let display = cell.display;
                Some(Box::new(move |cx| {
                    playback.update(cx, |playback, cx| {
                        match table.as_ref().and_then(|table| table.upgrade()) {
                            Some(table) => playback.start(ordered(&table, cx), display, cx),
                            None => playback.start(provider.tracks(cx).to_vec(), row, cx),
                        }
                    });
                }))
            }
            false => None,
        };

        let is_liked = self.liked_button(cell, track, cx);

        cells::title(
            cell,
            track.name.clone(),
            color,
            track.explicit,
            press,
            is_liked,
        )
    }

    fn liked_button(&self, cell: &Cell<TrackField>, track: &Track, cx: &App) -> Option<AnyElement> {
        let library = self.is_liked.as_ref()?;
        let id = track.id.clone()?;
        let theme = *cx.theme();
        let state = library.read(cx);
        let saved = state.saved(&id);
        let pending = state.pending(&id);
        let library = library.clone();
        let track = track.clone();

        Some(
            Button::new(("toggle-liked-track", cell.row))
                .ghost()
                .backgroundless()
                .small()
                .icon(match saved {
                    true => "icons/heart-filled.svg",
                    false => "icons/heart.svg",
                })
                .tooltip(match saved {
                    true => "menu-remove-from-library",
                    false => "menu-add-to-library",
                })
                .tint(match saved {
                    true => theme.primary,
                    false => theme.muted_foreground,
                })
                .when(!saved, |this| {
                    this.invisible()
                        .group_hover(ROW_GROUP, |style| style.visible())
                })
                .disabled(pending)
                .on_click(move |_, _, cx| {
                    library.update(cx, |library, cx| library.toggle(track.clone(), cx));
                })
                .into_any_element(),
        )
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
            if !self.sieve.keeps(&track) {
                return false;
            }
            hits(&track, query)
        })
    }

    fn filtered(&self, _cx: &App) -> bool {
        self.sieve.active()
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
            TrackField::Title => self.title_cell(&cell, track, title, cx),
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

    fn context_menu(&self, row: usize, visible: &[TrackField], cx: &App) -> Option<Menu> {
        let track = self.provider.tracks(cx).get(row)?;
        let columns = TrackColumns {
            album: visible.contains(&TrackField::Album),
            artists: visible.contains(&TrackField::Artists),
        };
        Some(match (&self.album, &self.playlist) {
            (Some(detail), _) => {
                let id = detail.read(cx).id()?;
                self.menu.for_album_track(track, id, columns, cx)
            }
            (_, Some(detail)) => self
                .menu
                .for_playlist_track(track, detail.clone(), columns, cx),
            (None, None) => self.menu.for_table_track(track, columns, cx),
        })
    }

    fn context_menu_will_open(&self, _row: usize, _cx: &App) {
        self.menu.reset();
    }

    fn compare(&self, field: TrackField, a: usize, b: usize, cx: &App) -> Ordering {
        sort::compare(self.provider.tracks(cx), field, a, b)
    }

    fn group(&self, field: TrackField, row: usize, cx: &App) -> Option<SharedString> {
        sort::group(self.provider.tracks(cx), field, row)
    }
}

#[cfg(test)]
mod fixture {
    use std::time::Duration;

    use spotify::Track;

    pub(super) fn track(seconds: u64, explicit: bool, playable: bool) -> Track {
        Track {
            id: Some("id".to_owned()),
            name: String::new(),
            playable,
            artists: String::new(),
            artist_refs: Vec::new(),
            album: String::new(),
            album_id: None,
            cover: None,
            duration: Duration::from_secs(seconds),
            added_at: None,
            playcount: None,
            popularity: 0,
            explicit,
            track_number: 0,
            disc_number: 0,
            tags: Vec::new(),
            languages: Vec::new(),
            credits: Vec::new(),
        }
    }
}
