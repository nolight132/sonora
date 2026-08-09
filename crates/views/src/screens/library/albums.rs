// SPDX-License-Identifier: GPL-3.0-or-later

use std::cmp::Ordering;
use ui::ActiveTheme as _;

use gpui::{AnyElement, App, Entity, SharedString, TextAlign};
use i18n::t;
use router::Destination;
use spotify::Album;
use state::{Library, LibraryState, Origin, Playback};
use ui::{Cell, ColumnSpec, GridSource, Menu, Width};

use crate::shared::cells::{self, ALWAYS, NUMBER, ROOMY, TRAILING, WIDE, YEAR};
use crate::shared::menu::album_menu;
use crate::shared::tracks::initial;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum AlbumField {
    Index,
    Cover,
    Name,
    Artists,
    Year,
    TrackCount,
}

const COLUMN: ColumnSpec<AlbumField> = ColumnSpec {
    field: AlbumField::Index,
    key: "",
    header: "",
    align: TextAlign::Left,
    width: Width::Fill(1.),
    anchored: false,
    sortable: true,
    hide_below: ALWAYS,
};

const INDEX: ColumnSpec<AlbumField> = ColumnSpec {
    field: AlbumField::Index,
    key: "index",
    header: "column-index",
    align: TextAlign::Center,
    width: Width::Fixed(NUMBER),
    anchored: true,
    sortable: false,
    ..COLUMN
};

const COVER: ColumnSpec<AlbumField> = ColumnSpec {
    field: AlbumField::Cover,
    key: "cover",
    width: Width::Thumb,
    anchored: true,
    sortable: false,
    ..COLUMN
};

const NAME: ColumnSpec<AlbumField> = ColumnSpec {
    field: AlbumField::Name,
    key: "name",
    header: "column-album",
    width: Width::Fill(0.55),
    ..COLUMN
};

const ARTISTS: ColumnSpec<AlbumField> = ColumnSpec {
    field: AlbumField::Artists,
    key: "artists",
    header: "column-artist",
    width: Width::Fill(0.45),
    ..COLUMN
};

const RELEASE_YEAR: ColumnSpec<AlbumField> = ColumnSpec {
    field: AlbumField::Year,
    key: "year",
    header: "column-year",
    align: TextAlign::Right,
    width: Width::Fixed(YEAR),
    hide_below: ROOMY,
    ..COLUMN
};

const TRACK_COUNT: ColumnSpec<AlbumField> = ColumnSpec {
    field: AlbumField::TrackCount,
    key: "tracks",
    header: "column-tracks",
    align: TextAlign::Right,
    width: Width::Fixed(TRAILING),
    hide_below: WIDE,
    ..COLUMN
};

pub(super) const COLUMNS: &[ColumnSpec<AlbumField>] =
    &[INDEX, COVER, NAME, ARTISTS, RELEASE_YEAR, TRACK_COUNT];

pub(super) struct AlbumSource {
    library: Entity<Library>,
    playback: Entity<Playback>,
    span: Option<(f32, f32)>,
}

impl AlbumSource {
    pub(super) fn new(library: Entity<Library>, playback: Entity<Playback>) -> Self {
        Self {
            library,
            playback,
            span: None,
        }
    }

    fn index_cell(&self, cell: &Cell<AlbumField>, album: &Album, cx: &App) -> AnyElement {
        let origin = Origin::Album(album.id.clone());
        let state = self.playback.read(cx).playing_from(&origin);
        let id = album.id.clone();
        let press = cells::toggle(&self.playback, state.clone(), move |playback, cx| {
            playback.play_album(&id, cx)
        });

        cells::index(cell, state, true, None, press, cx)
    }

    pub(super) fn at(&self, row: usize, cx: &App) -> Option<Album> {
        self.albums(cx).get(row).cloned()
    }

    pub(super) fn years(&self, query: &str, cx: &App) -> Vec<f32> {
        let mut years: Vec<f32> = self
            .albums(cx)
            .iter()
            .filter(|album| album.year > 0 && hits(album, query))
            .map(|album| album.year as f32)
            .collect();
        years.sort_by(f32::total_cmp);
        years.dedup();
        years
    }

    pub(super) fn span(&self) -> Option<(f32, f32)> {
        self.span
    }

    pub(super) fn set_span(&mut self, span: Option<(f32, f32)>) {
        self.span = span;
    }

    fn albums<'a>(&self, cx: &'a App) -> &'a [Album] {
        match self.library.read(cx).state() {
            LibraryState::Ready { albums, .. } => albums.as_slice(),
            _ => &[],
        }
    }
}

impl GridSource for AlbumSource {
    type Field = AlbumField;

    fn columns(&self) -> &'static [ColumnSpec<AlbumField>] {
        COLUMNS
    }

    fn rows(&self, cx: &App) -> usize {
        self.albums(cx).len()
    }

    fn matches(&self, row: usize, query: &str, cx: &App) -> bool {
        self.at(row, cx).is_some_and(|album| {
            if let Some((low, high)) = self.span {
                let year = album.year as f32;
                if album.year == 0 || year < low - 0.5 || year > high + 0.5 {
                    return false;
                }
            }
            hits(&album, query)
        })
    }

    fn filtered(&self, _cx: &App) -> bool {
        self.span.is_some()
    }

    fn playing(&self, row: usize, cx: &App) -> bool {
        self.albums(cx).get(row).is_some_and(|album| {
            let origin = Origin::Album(album.id.clone());
            self.playback.read(cx).playing_from(&origin).is_some()
        })
    }

    fn is_loading(&self, cx: &App) -> bool {
        self.library.read(cx).is_loading()
    }

    fn context_menu(&self, row: usize, _visible: &[AlbumField], cx: &App) -> Option<Menu> {
        Some(album_menu(
            self.at(row, cx)?.id,
            self.playback.clone(),
            false,
        ))
    }

    fn cell(&self, cell: Cell<AlbumField>, cx: &mut App) -> AnyElement {
        let theme = *cx.theme();
        let muted = theme.muted_foreground;

        let Some(album) = self.albums(cx).get(cell.row) else {
            return cells::blank(&cell);
        };

        if cell.field == AlbumField::Index {
            return self.index_cell(&cell, album, cx);
        }

        match cell.field {
            AlbumField::Cover => cells::artwork(&cell, album.cover.clone()),
            AlbumField::Name => cells::link(
                &cell,
                "album-name",
                album.name.clone(),
                theme.foreground,
                Destination::Album(album.id.clone().into()),
            ),
            AlbumField::Artists => cells::artists(
                &cell,
                album.artist_refs.clone(),
                album.artists.clone(),
                muted,
            ),
            AlbumField::Year => cells::dim(&cell, year(album), muted),
            AlbumField::TrackCount => cells::dim(&cell, format!("{}", album.track_count), muted),
            AlbumField::Index => cells::blank(&cell),
        }
    }

    fn compare(&self, field: AlbumField, a: usize, b: usize, cx: &App) -> Ordering {
        let albums = self.albums(cx);
        let text = |index: usize, pick: fn(&Album) -> &String| {
            albums
                .get(index)
                .map(|album| pick(album).to_lowercase())
                .unwrap_or_default()
        };

        match field {
            AlbumField::Name => text(a, |album| &album.name).cmp(&text(b, |album| &album.name)),
            AlbumField::Artists => {
                text(a, |album| &album.artists).cmp(&text(b, |album| &album.artists))
            }
            AlbumField::Year => albums
                .get(a)
                .map(|album| album.year)
                .cmp(&albums.get(b).map(|album| album.year)),
            AlbumField::TrackCount => albums
                .get(a)
                .map(|album| album.track_count)
                .cmp(&albums.get(b).map(|album| album.track_count)),
            AlbumField::Index | AlbumField::Cover => a.cmp(&b),
        }
    }

    fn group(&self, field: AlbumField, row: usize, cx: &App) -> Option<SharedString> {
        let album = self.albums(cx).get(row)?;

        match field {
            AlbumField::Name => Some(initial(&album.name)),
            AlbumField::Artists => Some(initial(&album.artists)),
            AlbumField::Year => Some(match album.year {
                0 => t!("common-unknown"),
                year => SharedString::from(year.to_string()),
            }),
            _ => None,
        }
    }
}

fn hits(album: &Album, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let haystack = format!("{} {} {}", album.name, album.artists, album.year);
    haystack.to_lowercase().contains(query)
}

fn year(album: &Album) -> String {
    if album.year > 0 {
        format!("{}", album.year)
    } else {
        String::new()
    }
}
