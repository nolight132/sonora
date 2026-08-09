// SPDX-License-Identifier: GPL-3.0-or-later

use gpui::TextAlign;
use ui::{ColumnSpec, Width};

use crate::shared::cells::{ALWAYS, DATE, NUMBER, ROOMY, SNUG, TRAILING, WIDE};

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

const COLUMN: ColumnSpec<TrackField> = ColumnSpec {
    field: TrackField::Index,
    key: "",
    header: "",
    align: TextAlign::Left,
    width: Width::Fill(1.),
    anchored: false,
    sortable: true,
    hide_below: ALWAYS,
};

const INDEX: ColumnSpec<TrackField> = ColumnSpec {
    field: TrackField::Index,
    key: "index",
    header: "column-index",
    align: TextAlign::Center,
    width: Width::Fixed(NUMBER),
    anchored: true,
    sortable: false,
    ..COLUMN
};

const COVER: ColumnSpec<TrackField> = ColumnSpec {
    field: TrackField::Cover,
    key: "cover",
    width: Width::Thumb,
    anchored: true,
    sortable: false,
    ..COLUMN
};

const TITLE: ColumnSpec<TrackField> = ColumnSpec {
    field: TrackField::Title,
    key: "title",
    header: "column-title",
    width: Width::Fill(0.42),
    ..COLUMN
};

const ARTISTS: ColumnSpec<TrackField> = ColumnSpec {
    field: TrackField::Artists,
    key: "artists",
    header: "column-artist",
    width: Width::Fill(0.29),
    hide_below: ROOMY,
    ..COLUMN
};

const ALBUM: ColumnSpec<TrackField> = ColumnSpec {
    field: TrackField::Album,
    key: "album",
    header: "column-album",
    width: Width::Fill(0.29),
    hide_below: WIDE,
    ..COLUMN
};

const ADDED_AT: ColumnSpec<TrackField> = ColumnSpec {
    field: TrackField::AddedAt,
    key: "added-at",
    header: "column-date-added",
    width: Width::Fixed(DATE),
    hide_below: WIDE,
    ..COLUMN
};

const PLAYS: ColumnSpec<TrackField> = ColumnSpec {
    field: TrackField::Plays,
    key: "plays",
    header: "column-plays",
    width: Width::Fixed(DATE),
    hide_below: WIDE,
    ..COLUMN
};

const DURATION: ColumnSpec<TrackField> = ColumnSpec {
    field: TrackField::Duration,
    key: "duration",
    header: "column-length",
    align: TextAlign::Right,
    width: Width::Fixed(TRAILING),
    hide_below: SNUG,
    ..COLUMN
};

pub(crate) const LIBRARY_COLUMNS: &[ColumnSpec<TrackField>] =
    &[INDEX, COVER, TITLE, ARTISTS, ALBUM, ADDED_AT, DURATION];

pub(crate) const ARTIST_COLUMNS: &[ColumnSpec<TrackField>] =
    &[INDEX, COVER, TITLE, ARTISTS, ALBUM, PLAYS, DURATION];

pub(crate) const ALBUM_COLUMNS: &[ColumnSpec<TrackField>] =
    &[INDEX, TITLE, ARTISTS, PLAYS, DURATION];
