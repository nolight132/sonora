// SPDX-License-Identifier: GPL-3.0-or-later

use std::cmp::Ordering;
use ui::ActiveTheme as _;

use gpui::{AnyElement, App, Entity, TextAlign, WeakEntity};
use i18n::t;
use router::{Destination, LibraryTab, navigate};
use spotify::Playlist;
use state::{Library, LibraryState, Origin, Playback, Sonora};
use ui::{Cell, ColumnSpec, GridSource, Menu, MenuItem, Width};

use super::{Edit, LibraryView};

use crate::shared::cells::{self, ALWAYS, NUMBER, ROOMY, SNUG, TRAILING};

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
        anchored: true,
        sortable: false,
        hide_below: ALWAYS,
    },
    ColumnSpec {
        field: PlaylistField::Cover,
        key: "cover",
        header: "",
        align: TextAlign::Left,
        width: Width::Thumb,
        anchored: true,
        sortable: false,
        hide_below: ALWAYS,
    },
    ColumnSpec {
        field: PlaylistField::Name,
        key: "name",
        header: "column-name",
        align: TextAlign::Left,
        width: Width::Fill(0.55),
        anchored: false,
        sortable: true,
        hide_below: ALWAYS,
    },
    ColumnSpec {
        field: PlaylistField::Owner,
        key: "owner",
        header: "column-owner",
        align: TextAlign::Left,
        width: Width::Fill(0.45),
        anchored: false,
        sortable: true,
        hide_below: ROOMY,
    },
    ColumnSpec {
        field: PlaylistField::TrackCount,
        key: "tracks",
        header: "column-tracks",
        align: TextAlign::Right,
        width: Width::Fixed(TRAILING),
        anchored: false,
        sortable: true,
        hide_below: SNUG,
    },
];

pub(super) struct PlaylistSource {
    library: Entity<Library>,
    playback: Entity<Playback>,
    view: WeakEntity<LibraryView>,
}

impl PlaylistSource {
    pub(super) fn new(
        library: Entity<Library>,
        playback: Entity<Playback>,
        view: WeakEntity<LibraryView>,
    ) -> Self {
        Self {
            library,
            playback,
            view,
        }
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

pub(crate) fn context_menu(
    playlist: Playlist,
    playback: Entity<Playback>,
    view: WeakEntity<LibraryView>,
    open_editor: bool,
) -> Menu {
    let opened = playlist.id.clone();
    let played = playlist.id.clone();
    let queued = playlist.id.clone();
    let playing = playback.clone();
    let queueing = playback;
    let renamed = view.clone();
    let deleted = view;
    let visibility = match playlist.owned {
        true => {
            let id = playlist.id.clone();
            let public = playlist.public;
            MenuItem::new(
                "playlist-visibility",
                match public {
                    true => t!("menu-make-playlist-private"),
                    false => t!("menu-make-playlist-public"),
                },
            )
            .icon("icons/user.svg")
            .on_click(move |_, _, cx| {
                let library = Sonora::global(cx).library.clone();
                library.update(cx, |library, cx| {
                    library.set_playlist_public(id.clone(), !public, cx)
                });
            })
        }
        false => MenuItem::new("playlist-visibility", t!("menu-make-playlist-public"))
            .icon("icons/user.svg")
            .disabled(),
    };
    let rename = match playlist.owned {
        true => {
            let playlist = playlist.clone();
            MenuItem::new("rename-playlist", t!("menu-rename-playlist")).on_click(
                move |_, window, cx| {
                    if open_editor {
                        navigate(Destination::Library(LibraryTab::Playlists), cx);
                    }
                    renamed
                        .update(cx, |view, cx| {
                            view.edit(Edit::Rename(playlist.clone()), window, cx)
                        })
                        .ok();
                },
            )
        }
        false => MenuItem::new("rename-playlist", t!("menu-rename-playlist")).disabled(),
    };
    let delete = match playlist.owned {
        true => MenuItem::new("delete-playlist", t!("menu-delete-playlist")).on_click(
            move |_, window, cx| {
                if open_editor {
                    navigate(Destination::Library(LibraryTab::Playlists), cx);
                }
                deleted
                    .update(cx, |view, cx| {
                        view.edit(Edit::Delete(playlist.clone()), window, cx)
                    })
                    .ok();
            },
        ),
        false => MenuItem::new("delete-playlist", t!("menu-delete-playlist")).disabled(),
    };

    let menu = match open_editor {
        true => Menu::new("playlist-context-menu"),
        false => Menu::new("playlist-context-menu").item(
            MenuItem::new("open-playlist", t!("menu-open-playlist"))
                .icon("icons/info.svg")
                .on_click(move |_, _, cx| {
                    navigate(Destination::Playlist(opened.clone().into()), cx)
                }),
        ),
    };

    menu.item(
        MenuItem::new("play-playlist", t!("menu-play-playlist"))
            .icon("icons/play.svg")
            .on_click(move |_, _, cx| {
                playing.update(cx, |playback, cx| playback.play_playlist(&played, cx));
            }),
    )
    .item(
        MenuItem::new("enqueue-playlist", t!("menu-add-to-queue"))
            .icon("icons/list-end.svg")
            .on_click(move |_, _, cx| {
                queueing.update(cx, |playback, cx| playback.enqueue_playlist(&queued, cx));
            }),
    )
    .item(visibility)
    .item(MenuItem::separator("playlist-actions"))
    .item(rename)
    .item(delete)
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

    fn context_menu(&self, row: usize, cx: &App) -> Option<Menu> {
        Some(context_menu(
            self.at(row, cx)?,
            self.playback.clone(),
            self.view.clone(),
            false,
        ))
    }

    fn cell(&self, cell: Cell<PlaylistField>, cx: &mut App) -> AnyElement {
        let theme = *cx.theme();
        let muted = theme.muted_foreground;

        let Some(playlist) = self.playlists(cx).get(cell.row) else {
            return cells::blank(&cell);
        };

        if cell.field == PlaylistField::Index {
            return self.index_cell(&cell, playlist, cx);
        }

        match cell.field {
            PlaylistField::Cover => cells::artwork(&cell, playlist.cover.clone()),
            PlaylistField::Name => cells::link(
                &cell,
                "playlist-name",
                playlist.name.clone(),
                theme.foreground,
                Destination::Playlist(playlist.id.clone().into()),
            ),
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
